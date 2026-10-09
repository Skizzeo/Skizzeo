//! Herkunft aus Erweiterungen im Detail einer Position (E8b: B15, E8-6,
//! keine Folgepositionen).

use super::LvPosition;
use sk_cost::Katalog;
use sk_model::{ElementId, Model};

/// Zeilen unter „Preis“, wenn eine Erweiterung beteiligt ist: Preis der
/// Bauleistung aus der Definition, Stoffe aus der Definition oder statt
/// ihrer der gleichnamige Katalogartikel, und der Hinweis, dass für Bauteile
/// aus Erweiterungen keine Folgepositionen entstehen.
pub(super) fn herkunft(
    m: &Model,
    kat: &Katalog,
    p: &LvPosition,
    elemente: &[ElementId],
) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(s) = kat.aus_erweiterung("service", p.quelle) {
        v.push(format!("Preis {}, nicht im Firmenkatalog", s.herkunft()));
    }
    let mut gesehen = Vec::new();
    for g in kat.anteile_von(p.quelle).filter_map(|a| a.artikel) {
        if gesehen.contains(&g) {
            continue;
        }
        gesehen.push(g);
        let Some(s) = kat.aus_erweiterung("article", g) else {
            continue;
        };
        if s.statt {
            let name = kat.artikel(g).map_or(s.name.as_str(), |a| a.name.as_str());
            // Wortlaut der Prüfung E8 (Nachtrag 19:00, E8-6)
            v.push(format!(
                "Stoff {name} (Katalog, statt Erweiterungsartikel {})",
                s.key
            ));
        } else {
            v.push(format!("Stoff {} {}", s.name, s.herkunft()));
        }
    }
    if elemente.iter().any(|e| m.ext_def_of(*e).is_some()) {
        v.push("Folgepositionen gelten nicht für Erweiterungen".into());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::super::detail;
    use sk_cost::lesen;
    use sk_cost::lv::{lv_aus, LvWahl};
    use sk_model::erweiterung::{ExtDef, ExtPart};
    use sk_model::qto::{schedule, Umfang};
    use sk_model::{Guid, Model};

    /// Stütze und Bodenplatte im EG: Herkunft und der Hinweis zu den
    /// Folgepositionen im Detail der Position.
    #[test]
    fn herkunft_im_detail() {
        let mut m = Model::with_seed(1);
        m.add_building(1);
        // Stütze mit eigenem Beton, Name wie der Werks-Artikel (E8-6)
        let stuetze = include_str!("../../../crates/sk-szb/beispiele/werk.stuetze.szb")
            .replace(
                "[leistung] key=stuetze_beton",
                "[artikel] key=beton_c25 name=\"Transportbeton  C25/30 XC1-XC2 F3\" einheit=m3 preis=190\n[leistung] key=stuetze_beton",
            )
            .replace("stoffe=\"1S7bUW0010080100000006:1\"", "stoffe=\"beton_c25:1\"");
        for t in [
            include_str!("../../../crates/sk-szb/beispiele/werk.bodenplatte.szb"),
            &stuetze,
        ] {
            m.put_ext_def(ExtDef::lesen(t).unwrap()).unwrap();
        }
        let eg = m
            .storeys()
            .iter()
            .find(|(_, s)| s.short == "EG" && s.building.is_some())
            .map(|(id, _)| id)
            .unwrap();
        for (key, at) in [
            ("werk.bodenplatte", [0.0, 0.0]),
            ("werk.stuetze", [1000.0, 1000.0]),
        ] {
            let p = ExtPart::new(m.ext_def(key).unwrap(), at);
            m.add_ext(eg, p).unwrap();
        }
        let k = lesen::katalog(&m, None);
        let b = lesen::kosten(&m, &schedule(&m), &k, &Umfang::projekt());
        let bp = Guid::from_ifc("1S7bUW0010080200000001").unwrap();
        let w = LvWahl {
            los: k
                .los(k.leistung(bp).unwrap().titel)
                .unwrap()
                .parent
                .unwrap(),
            untertitel: false,
            preise: true,
            heute: None,
        };
        let lv = lv_aus(&m, &b, &k, &w);
        let oz = |g: Guid| {
            lv.titel
                .iter()
                .flat_map(|t| &t.positionen)
                .find(|p| p.quelle == g)
                .unwrap()
                .oz
                .clone()
        };
        let schal = sk_cost::erweiterung::kennung("werk.stuetze", "leistung", "stuetze_schalung");
        let d = detail(&m, &k, &lv, &oz(schal)).unwrap();
        let folge = "Folgepositionen gelten nicht für Erweiterungen";
        for z in [
            "Preis aus Erweiterung werk.stuetze v1 · grob, nicht im Firmenkatalog",
            "Stoff Stützenschalung, Vorhaltung und Verschleiß aus Erweiterung werk.stuetze v1 · grob",
            folge,
        ] {
            assert!(d.preis.iter().any(|l| l == z), "{z}: {:#?}", d.preis);
        }
        let beton = sk_cost::erweiterung::kennung("werk.stuetze", "leistung", "stuetze_beton");
        let werk = Guid::from_ifc("1S7bUW0010080100000006").unwrap();
        let z = format!(
            "Stoff {} (Katalog, statt Erweiterungsartikel beton_c25)",
            k.artikel(werk).unwrap().name
        );
        let d = detail(&m, &k, &lv, &oz(beton)).unwrap();
        assert!(d.preis.contains(&z), "{z}: {:#?}", d.preis);
        // Werks-Leistung der Bodenplatte: Katalogpreis, aber keine Folgen
        let d = detail(&m, &k, &lv, &oz(bp)).unwrap();
        assert!(d.preis.iter().all(|l| !l.contains("aus Erweiterung")));
        assert!(d.preis.iter().any(|l| l == folge), "{:#?}", d.preis);
    }
}

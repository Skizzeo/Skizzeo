//! Die Kostengruppen des Werksbestands (`bestand::KGS`) decken alle `kg=`
//! der Werks-Leistungen in `werk.szk` ab (Gleichstand R8, Werksstand 16:
//! KG 336 an der Laibung Innenputz).

use sk_szb::bestand::KGS;

#[test]
fn kgs_wie_werk_szk() {
    let werk = include_str!("../../sk-cost/werk.szk");
    let mut fehlt = Vec::new();
    for zeile in werk.lines().filter(|z| z.starts_with("[service]")) {
        for feld in zeile.split_whitespace() {
            if let Some(kg) = feld.strip_prefix("kg=") {
                let kg: u32 = kg.parse().expect("kg= ist eine Zahl");
                if !KGS.contains(&kg) && !fehlt.contains(&kg) {
                    fehlt.push(kg);
                }
            }
        }
    }
    assert!(fehlt.is_empty(), "KG aus werk.szk fehlen in KGS: {fehlt:?}");
}

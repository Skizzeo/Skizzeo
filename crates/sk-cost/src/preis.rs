//! Aufbau eines EP für das Preisblatt (KA-2c, paket-ka2 §1 und §4,
//! Einstellungen §3 KA-2 Punkt 9): Aufwandswert, Lohn, Stoffanteile, Gerät,
//! Sonstiges und NU so, wie die Rechnung sie für eine Position nimmt, dazu
//! „gilt auch für“ und die Sätze, die im Projekt vom Firmenkatalog
//! abweichen (Regel 89). Rechnet mit derselben Stoffrechnung wie die
//! Kosten; der EP des Aufbaus ist der EP der Position, auf den Cent.

use crate::befund::Ort;
use crate::geld::{Cent, Dez};
use crate::katalog::{Einheit, Katalog};
use crate::op::SatzId;
use crate::rechnung::{feste_ep, mit_zuschlag, stoff_teile, Position, Quelle, Stoffteil};
use sk_model::{Guid, Model};

/// EP einer Position in seinen Teilen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aufbau {
    pub leistung: Guid,
    pub kurz: String,
    pub einheit: Einheit,
    /// Aufwandswert (h je Einheit) und Verrechnungslohn (€/h).
    pub stunden: Dez,
    pub lohnsatz: Dez,
    pub lohn: Cent,
    pub stoffe: Vec<Stoffteil>,
    /// Stoff-EP mit Zuschlag.
    pub stoff: Cent,
    pub geraet: Cent,
    pub sonst: Cent,
    pub nu: Option<Cent>,
    pub ep: Cent,
}

impl Aufbau {
    /// Betrag eines Stoffanteils auf den Cent (ohne Zuschlag), für die
    /// Zeile „4,4 kg × 1,10 = 4,84“.
    pub fn betrag(t: &Stoffteil) -> Cent {
        Cent(crate::geld::runden(t.wert, 10_000_000_000) as i64)
    }
}

/// Aufbau des EP der Position `p` mit dem Katalog `k` (wirksam, Firma oder
/// mit noch nicht ausgeführten Operationen). `None` beim Richtpreis oder
/// wenn die Bauleistung im Katalog fehlt.
pub fn aufbau(m: &Model, k: &Katalog, p: &Position) -> Option<Aufbau> {
    let (g, geschaetzt) = match p.quelle {
        Quelle::Leistung(g) => (g, false),
        Quelle::Geschaetzt(g) => (g, true),
        Quelle::Richtpreis(_) => return None,
    };
    let l = k.leistung(g)?;
    let mat = p.schicht.and_then(|(b, _)| {
        m.materials()
            .iter()
            .find(|(_, x)| x.guid == b)
            .map(|(_, x)| x)
    });
    let schicht = p.schicht.map(|(_, t)| (t, mat));
    let mut befunde = Vec::new();
    let (stoffe, _) = stoff_teile(k, l, schicht, geschaetzt, &Ort::Datei, &mut befunde);
    let stoff = mit_zuschlag(k, stoffe.iter().map(|t| t.wert).sum());
    let (lohn, geraet, sonst, nu) = feste_ep(k, l);
    let ep = nu.unwrap_or(lohn + stoff + geraet + sonst);
    Some(Aufbau {
        leistung: g,
        kurz: l.kurz.clone(),
        einheit: l.einheit,
        stunden: l.stunden,
        lohnsatz: k.werte.lohn,
        lohn,
        stoffe,
        stoff,
        geraet,
        sonst,
        nu,
        ep,
    })
}

/// Weitere Bauleistungen, deren EP der Artikel `artikel` mitbestimmt (fest
/// als Anteil oder als Artikel der Schicht über Baustoff und Dicke); mit
/// Kurztext, ohne `ohne`.
pub fn auch_fuer(k: &Katalog, artikel: Guid, ohne: Guid) -> Vec<String> {
    let Some(a) = k.artikel(artikel) else {
        return Vec::new();
    };
    let passt_dicke = |tmin: Option<Dez>, tmax: Option<Dez>| match a.t {
        Some(t) => tmin.is_none_or(|x| t >= x) && tmax.is_none_or(|x| t <= x),
        None => true,
    };
    let mut out = Vec::new();
    for l in k.leistungen.iter().filter(|l| !l.retired && l.guid != ohne) {
        let nutzt = k.anteile_von(l.guid).any(|s| match s.artikel {
            Some(g) => g == artikel,
            None => a.mat.is_some() && l.mat == a.mat && passt_dicke(l.tmin, l.tmax),
        });
        if nutzt && !out.contains(&l.kurz) {
            out.push(l.kurz.clone());
        }
    }
    out
}

/// Sätze hinter dem Aufbau, die im Projekt geändert sind (`[origin]` mit
/// `proj=1`, Regel 89): die Bauleistung und ihre Artikel. Leer, wenn die
/// Position wie die Firma rechnet.
pub fn abweichend(k: &Katalog, a: &Aufbau) -> Vec<SatzId> {
    let mut out = Vec::new();
    let mut pruefe = |rec: &'static str, g: Guid| {
        let key = g.to_ifc();
        if k.herkunft_von(rec, &key).is_some_and(|u| u.proj) {
            let id = SatzId::neu(rec, key);
            if !out.contains(&id) {
                out.push(id);
            }
        }
    };
    pruefe("service", a.leistung);
    for t in &a.stoffe {
        if let Some(g) = t.artikel {
            pruefe("article", g);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesen;
    use sk_model::{qto, szo, GuidGen};

    fn rh1() -> Model {
        szo::read_with(
            include_str!("../referenz/rh1-standardhaus.szo"),
            GuidGen::with_seed(1),
            &lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model
    }

    /// Der Aufbau trifft den EP jeder Position auf den Cent; Mauerwerk
    /// 17,5 hat Planstein als Artikel der Schicht und den Mörtel fest.
    #[test]
    fn aufbau_gleich_ep() {
        let m = rh1();
        let k = lesen::katalog(&m, None);
        let b = lesen::kosten(&m, &qto::schedule(&m), &k, &crate::Umfang::projekt());
        let mut n = 0;
        for p in &b.positionen {
            let Some(a) = aufbau(&m, &k, p) else {
                assert!(matches!(p.quelle, Quelle::Richtpreis(_)), "{}", p.kurz);
                continue;
            };
            n += 1;
            assert_eq!(a.ep, p.ep, "{}", p.kurz);
            assert_eq!(a.stoff, p.stoff, "{}", p.kurz);
            assert_eq!(a.lohn, p.lohn, "{}", p.kurz);
        }
        assert!(n >= 5, "{n}");
        let mw = b
            .positionen
            .iter()
            .find(|p| p.kurz.contains("Porenbeton") && p.kurz.contains("17,5"))
            .expect("Mauerwerk 17,5");
        let a = aufbau(&m, &k, mw).unwrap();
        assert_eq!(
            a.stoffe.iter().map(|t| t.haupt).collect::<Vec<_>>(),
            [true, false],
            "Planstein Hauptstoff, Mörtel Nebenstoff: {:?}",
            a.stoffe
        );
        assert_eq!(Aufbau::betrag(&a.stoffe[1]), Cent(484));
        assert!(abweichend(&k, &a).is_empty());
        let stein = &a.stoffe[0];
        let weitere = auch_fuer(&k, stein.artikel.unwrap(), a.leistung);
        assert!(!weitere.is_empty(), "Planstein auch in IW");
    }
}

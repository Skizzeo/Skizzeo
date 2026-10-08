//! Aufbau eines EP für das Preisblatt (KA-2c, paket-ka2 §1 und §4,
//! Einstellungen §3 KA-2 Punkt 9): Aufwandswert, Lohn, Stoffanteile, Gerät,
//! Sonstiges und NU so, wie die Rechnung sie für eine Position nimmt, dazu
//! „gilt auch für“ und die Sätze, die im Projekt vom Firmenkatalog
//! abweichen (Regel 89). Rechnet mit derselben Stoffrechnung wie die
//! Kosten; der EP des Aufbaus ist der EP der Position, auf den Cent.

use crate::befund::Ort;
use crate::geld::{Cent, Dez};
use crate::katalog::{fnv, Einheit, Katalog, Leistung};
use crate::op::{Bauleistung, Op, SatzId};
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

/// Felder einer Bauleistung, wie `BauleistungAendern` sie nimmt.
pub fn bauleistung(l: &Leistung) -> Bauleistung {
    Bauleistung {
        kurz: l.kurz.clone(),
        gewerk: l.gewerk,
        titel: l.titel,
        pos: l.pos,
        einheit: l.einheit,
        bezug: l.bezug,
        stunden: l.stunden,
        geraet: l.geraet,
        sonst: l.sonst,
        nu: l.nu,
        kg: l.kg,
        kategorien: l.kategorien.clone(),
        mat: l.mat,
        tmin: l.tmin,
        tmax: l.tmax,
        funktion: l.funktion.clone(),
    }
}

/// Was im Preisblatt steht: Aufwandswert und Preise der Hauptstoffe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Eingabe {
    pub stunden: Option<Dez>,
    pub preise: Vec<(Guid, Dez)>,
}

/// Operationen zum Preisblatt (paket-ka2 §4): `PreisSetzen` je geändertem
/// Stoffpreis, `BauleistungAendern` bei geändertem Aufwandswert. Leer, wenn
/// nichts anders ist. `stand`: Monat des Preises („10/2026“).
pub fn preis_ops(k: &Katalog, a: &Aufbau, e: &Eingabe, stand: &str) -> Vec<Op> {
    let mut ops = Vec::new();
    for (g, p) in &e.preise {
        let alt = k.artikel(*g).and_then(|x| x.preis);
        if alt != Some(*p) {
            ops.push(Op::PreisSetzen {
                artikel: *g,
                preis: Some(*p),
                stand: stand.to_string(),
                quelle: "Preisblatt".into(),
            });
        }
    }
    if let (Some(h), Some(l)) = (e.stunden, k.leistung(a.leistung)) {
        if h != l.stunden {
            ops.push(Op::BauleistungAendern {
                bauleistung: l.guid,
                daten: Bauleistung {
                    stunden: h,
                    ..bauleistung(l)
                },
            });
        }
    }
    ops
}

impl Katalog {
    /// Katalog mit noch nicht ausgeführten Operationen des Preisblatts
    /// (paket-ka2 §3, „Katalog::mit“): `PreisSetzen` und
    /// `BauleistungAendern` direkt auf einer Kopie, mit neuem Stempel. Rein
    /// im Speicher und ohne Prüfung; geschrieben und geprüft wird über
    /// `ausfuehren_folge`. `None` bei anderen Operationen oder unbekannten
    /// Sätzen, dann gilt [`crate::vorschau`].
    pub fn mit(&self, ops: &[Op]) -> Option<Katalog> {
        let mut k = self.clone();
        let mut h = fnv(k.stempel, b"mit");
        for op in ops {
            match op {
                Op::PreisSetzen { artikel, preis, .. } => {
                    let a = k.artikel.iter_mut().find(|a| a.guid == *artikel)?;
                    a.preis = *preis;
                    h = fnv(h, artikel.to_ifc().as_bytes());
                    h = fnv(h, preis.map_or(String::new(), |p| p.text()).as_bytes());
                }
                Op::BauleistungAendern { bauleistung, daten } => {
                    let l = k.leistungen.iter_mut().find(|l| l.guid == *bauleistung)?;
                    let d = daten.clone();
                    l.kurz = d.kurz;
                    l.gewerk = d.gewerk;
                    l.titel = d.titel;
                    l.pos = d.pos;
                    l.einheit = d.einheit;
                    l.bezug = d.bezug;
                    l.stunden = d.stunden;
                    l.geraet = d.geraet;
                    l.sonst = d.sonst;
                    l.nu = d.nu;
                    l.kg = d.kg;
                    l.kategorien = d.kategorien;
                    l.mat = d.mat;
                    l.tmin = d.tmin;
                    l.tmax = d.tmax;
                    l.funktion = d.funktion;
                    h = fnv(h, bauleistung.to_ifc().as_bytes());
                    h = fnv(h, format!("{:?}", l).as_bytes());
                }
                _ => return None,
            }
        }
        k.stempel = h;
        Some(k)
    }
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
        // KA-2c Abnahme 11: Planstein 20,50 → EP 27,00 + 20,50 + 4,84
        let e = Eingabe {
            stunden: Some(a.stunden),
            preise: vec![(stein.artikel.unwrap(), Dez::lesen("20.5", 4).unwrap())],
        };
        let ops = preis_ops(&k, &a, &e, "10/2026");
        assert_eq!(ops.len(), 1, "nur der Preis");
        let plan = crate::vorschau(&m, None, crate::Rolle::Admin, &ops).unwrap();
        let neu = aufbau(&m, &plan.katalog, mw).unwrap();
        assert_eq!(neu.ep, Cent(5_234));
        assert_eq!(neu.lohn, Cent(2_700));
        // Katalog::mit rechnet dasselbe wie der Plan, auf den Cent
        let schnell = k.mit(&ops).expect("Preis");
        assert_ne!(schnell.stempel, k.stempel);
        let sched = qto::schedule(&m);
        let u = crate::Umfang::projekt();
        assert_eq!(
            lesen::kosten(&m, &sched, &schnell, &u).netto,
            lesen::kosten(&m, &sched, &plan.katalog, &u).netto
        );
        let h = Dez::lesen("0.5", 4).unwrap();
        let ops2 = preis_ops(
            &k,
            &a,
            &Eingabe {
                stunden: Some(h),
                preise: vec![],
            },
            "",
        );
        let plan2 = crate::vorschau(&m, None, crate::Rolle::Admin, &ops2).unwrap();
        assert_eq!(
            lesen::kosten(&m, &sched, &k.mit(&ops2).unwrap(), &u).netto,
            lesen::kosten(&m, &sched, &plan2.katalog, &u).netto
        );
        assert!(k
            .mit(&[Op::AbweichungZuruecknehmen { saetze: vec![] }])
            .is_none());
        assert!(preis_ops(
            &k,
            &a,
            &Eingabe {
                stunden: Some(a.stunden),
                preise: vec![]
            },
            ""
        )
        .is_empty());
    }
}

//! Verwaltung am Einzelplatz (KA-3a1, paket-ka3a §2): Protokoll des
//! Firmenkatalogs nach Ständen und „Diese Änderung zurücknehmen“.
//!
//! Ein Firmen-Rückgängig gibt es nicht (Entscheid VK-05): Eine Änderung
//! nimmt man zurück, indem man die alten Werte als neuen Stand schreibt.
//! [`umkehr`] liefert dafür die benannten Operationen; geschrieben wird
//! über `firma_anwenden` wie jede andere Firmenänderung.

use crate::befund::{self, Befund, Ort};
use crate::geld::Dez;
use crate::katalog::{Katalog, RATEN};
use crate::op::{Op, SatzId, Stoff};
use sk_model::Guid;

/// Eine Zeile des Protokolls (`[log]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eintrag {
    pub key: u32,
    /// Operationsname („preis_setzen“).
    pub op: String,
    /// Abschnitt und Kennung des Satzes.
    pub rec: String,
    pub of: String,
    /// Kurzform alt und neu (nur die geänderten Felder).
    pub alt: String,
    pub neu: String,
}

/// Ein Stand mit seinen Protokollzeilen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stand {
    pub stand: u32,
    /// `2026-10-08T07:00`.
    pub zeit: String,
    pub rolle: String,
    pub eintraege: Vec<Eintrag>,
}

/// Abschnitte, deren Änderung sich zurücknehmen lässt; Kopf, Herkunft und
/// Protokoll selbst nicht (die Herkunft schreibt die Rücknahme neu).
const UMKEHRBAR: [&str; 6] = ["article", "service", "svcpart", "svcfollow", "rate", "lot"];

/// Protokoll des Firmenkatalogs nach Ständen, der neueste zuerst.
pub fn protokoll(k: &Katalog) -> Vec<Stand> {
    let mut out: Vec<Stand> = Vec::new();
    let mut zeilen: Vec<_> = k.protokoll.iter().collect();
    zeilen.sort_by_key(|p| (std::cmp::Reverse(p.stand), p.key));
    for p in zeilen {
        let s = &p.satz;
        let text = |f: &str| s.text(f).unwrap_or_default().to_string();
        let e = Eintrag {
            key: p.key,
            op: text("op"),
            rec: text("rec"),
            of: text("of"),
            alt: text("old"),
            neu: text("new"),
        };
        match out.last_mut().filter(|x| x.stand == p.stand) {
            Some(x) => x.eintraege.push(e),
            None => out.push(Stand {
                stand: p.stand,
                zeit: text("time"),
                rolle: text("role"),
                eintraege: vec![e],
            }),
        }
    }
    out
}

/// Firmenwert `schluessel` im Katalog (mit dem Werkswert als Rückfall).
fn firmenwert(k: &Katalog, schluessel: &str) -> Option<Dez> {
    let w = &k.werte;
    match schluessel {
        "wage" => Some(w.lohn),
        "surcharge" => Some(w.zuschlag),
        "vat" => Some(w.mwst),
        s => s.strip_prefix("steel.").and_then(|art| {
            w.stahl
                .iter()
                .find(|(a, _)| a == art)
                .map(|(_, v)| *v)
                .or_else(|| RATEN.iter().find(|r| r.0 == s).map(|r| Dez::ganz(r.3)))
        }),
    }
}

/// Name eines Satzes für Menschen: Artikelname, Kurztext, Firmenwert.
pub fn satz_name(k: &Katalog, rec: &str, of: &str) -> String {
    let g = Guid::from_ifc(of);
    let name = match rec {
        "article" => g.and_then(|g| k.artikel(g)).map(|a| a.name.clone()),
        "service" => g.and_then(|g| k.leistung(g)).map(|l| l.kurz.clone()),
        "svcpart" => g
            .and_then(|g| k.anteile.iter().find(|a| a.guid == g))
            .and_then(|a| k.leistung(a.leistung))
            .map(|l| format!("Stoffanteil von {}", l.kurz)),
        "svcfollow" => g
            .and_then(|g| k.folgen.iter().find(|f| f.guid == g))
            .and_then(|f| k.leistung(f.leistung))
            .map(|l| format!("Folgeposition von {}", l.kurz)),
        "lot" => g.and_then(|g| k.los(g)).map(|l| l.name.clone()),
        "rate" => Some(crate::wort::firmenwert(of)),
        _ => None,
    };
    name.unwrap_or_else(|| format!("{} {of}", crate::wort::abschnitt(rec)))
}

fn abgelehnt(stand: u32, grund: String) -> Vec<Befund> {
    vec![Befund::fehler(
        93,
        befund::r93(&format!("Stand {stand} zurücknehmen"), &grund),
        Ort::Datei,
    )]
}

/// Ausmustern bzw. Wiederherstellen, wenn `retired` vorher anders war.
fn ruhestand(ops: &mut Vec<Op>, rec: &'static str, of: &str, vorher: bool, jetzt: bool) {
    if vorher != jetzt {
        let satz = SatzId::neu(rec, of);
        ops.push(if vorher {
            Op::Ausmustern { satz }
        } else {
            Op::Wiederherstellen { satz }
        });
    }
}

/// „Diese Änderung zurücknehmen“ für Stand `stand` (paket-ka3a §2 KA-3a1):
/// die Operationen, die jeden in diesem Stand geänderten Satz auf seinen
/// Wert davor setzen. `jetzt` ist der Firmenkatalog heute, `vorher` der
/// Stand davor (aus dem Archiv; ein Stand ohne eigene Kostensätze liest
/// sich als Werksbestand). Neu angelegte Sätze werden ausgemustert, nie
/// gelöscht (Regel 87). Hat ein späterer Stand denselben Satz wieder
/// geändert, gibt es nur den Befund mit dem Satz.
pub fn umkehr(jetzt: &Katalog, vorher: &Katalog, stand: u32) -> Result<Vec<Op>, Vec<Befund>> {
    let st = protokoll(jetzt);
    let Some(dieser) = st.iter().find(|s| s.stand == stand) else {
        return Err(abgelehnt(stand, "diesen Stand gibt es nicht".into()));
    };
    let mut saetze: Vec<(&str, &str)> = Vec::new();
    for e in &dieser.eintraege {
        let x = (e.rec.as_str(), e.of.as_str());
        if UMKEHRBAR.contains(&x.0) && !saetze.contains(&x) {
            saetze.push(x);
        }
    }
    if saetze.is_empty() {
        return Err(abgelehnt(
            stand,
            "er enthält keine Änderung an Kostensätzen".into(),
        ));
    }
    // Später wieder geändert: nichts geschieht
    for s in st.iter().filter(|s| s.stand > stand) {
        if let Some(e) = s
            .eintraege
            .iter()
            .find(|e| saetze.contains(&(e.rec.as_str(), e.of.as_str())))
        {
            let name = satz_name(jetzt, &e.rec, &e.of);
            return Err(abgelehnt(
                stand,
                format!("{name} wurde in Stand {} wieder geändert", s.stand),
            ));
        }
    }
    let mut ops = Vec::new();
    for (rec, of) in saetze {
        let g = Guid::from_ifc(of);
        match rec {
            "rate" => {
                let (Some(a), Some(n)) = (firmenwert(vorher, of), firmenwert(jetzt, of)) else {
                    continue;
                };
                if a != n {
                    ops.push(Op::FirmenwertSetzen {
                        schluessel: of.to_string(),
                        wert: a,
                    });
                }
            }
            "article" => {
                let Some(n) = g.and_then(|g| jetzt.artikel(g)) else {
                    continue;
                };
                match g.and_then(|g| vorher.artikel(g)) {
                    None => ruhestand(&mut ops, "article", of, true, n.retired),
                    Some(a) => {
                        if a.preis != n.preis {
                            let text = |f: &str| a.satz.text(f).unwrap_or_default().to_string();
                            ops.push(Op::PreisSetzen {
                                artikel: n.guid,
                                preis: a.preis,
                                stand: text("date"),
                                quelle: text("source"),
                            });
                        }
                        ruhestand(&mut ops, "article", of, a.retired, n.retired);
                    }
                }
            }
            "service" => {
                let Some(n) = g.and_then(|g| jetzt.leistung(g)) else {
                    continue;
                };
                match g.and_then(|g| vorher.leistung(g)) {
                    None => ruhestand(&mut ops, "service", of, true, n.retired),
                    Some(a) => {
                        let daten = crate::preis::bauleistung(a);
                        if daten != crate::preis::bauleistung(n) {
                            ops.push(Op::BauleistungAendern {
                                bauleistung: n.guid,
                                daten,
                            });
                        }
                        ruhestand(&mut ops, "service", of, a.retired, n.retired);
                    }
                }
            }
            "svcpart" => {
                let finde = |k: &Katalog| -> Option<crate::katalog::Anteil> {
                    g.and_then(|g| k.anteile.iter().find(|x| x.guid == g).cloned())
                };
                let (a, n) = (finde(vorher), finde(jetzt));
                let (a, n) = (a.as_ref(), n.as_ref());
                let Some((leistung, nr)) = a.or(n).map(|x| (x.leistung, x.nr)) else {
                    continue;
                };
                let stoff = |x: &crate::katalog::Anteil| match x.artikel {
                    Some(artikel) => Stoff::Artikel {
                        artikel,
                        menge: x.menge,
                    },
                    None => Stoff::Schicht { faktor: x.menge },
                };
                if a.map(stoff) != n.map(stoff) {
                    ops.push(Op::StoffanteilSetzen {
                        bauleistung: leistung,
                        nr,
                        anteil: a.map(stoff),
                    });
                }
            }
            "svcfollow" => {
                let finde = |k: &Katalog| -> Option<crate::katalog::Folge> {
                    g.and_then(|g| k.folgen.iter().find(|x| x.guid == g).cloned())
                };
                let (a, n) = (finde(vorher), finde(jetzt));
                let (a, n) = (a.as_ref(), n.as_ref());
                let Some((leistung, nr)) = a.or(n).map(|x| (x.leistung, x.nr)) else {
                    continue;
                };
                let folge = |x: &crate::katalog::Folge| (x.folge, x.faktor);
                if a.map(folge) != n.map(folge) {
                    ops.push(Op::FolgeSetzen {
                        bauleistung: leistung,
                        nr,
                        folge: a.map(folge),
                    });
                }
            }
            "lot" => {
                let Some(n) = g.and_then(|g| jetzt.los(g)) else {
                    continue;
                };
                let vorher_aus = g.and_then(|g| vorher.los(g)).is_none_or(|a| a.retired);
                ruhestand(&mut ops, "lot", of, vorher_aus, n.retired);
            }
            _ => {}
        }
    }
    if ops.is_empty() {
        return Err(abgelehnt(
            stand,
            "die Werte stehen schon wie vor diesem Stand".into(),
        ));
    }
    Ok(ops)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op::{firma_anwenden, Herkunft, HerkunftArt, Rolle};
    use sk_model::{Library, Model};

    fn hand() -> Herkunft {
        Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "15:30")
    }

    fn katalog(text: &str) -> Katalog {
        let lib = sk_model::read_szk_with(text, &crate::satz::ABSCHNITTE_SZK).unwrap();
        crate::lesen::firma_oder_werk(&Model::new(), Some(&lib))
    }

    fn schreiben(text: &str, ops: &[Op]) -> String {
        firma_anwenden(text, text, Rolle::Admin, &hand(), ops)
            .unwrap()
            .text
    }

    /// KA-3a Abnahme 15: zwei Stände, „Diese Änderung zurücknehmen“ am
    /// ersten setzt den alten Preis als dritten Stand; der Lohn aus Stand 2
    /// bleibt. Ändert ein späterer Stand denselben Satz, gibt es nur den
    /// Befund mit dem Satz.
    #[test]
    fn stand_zuruecknehmen() {
        let t0 = sk_model::write_szk(&Library::standard());
        let k0 = katalog(&t0);
        let a = k0.artikel.iter().find(|a| a.preis.is_some()).unwrap();
        let alt = a.preis.unwrap();
        let preis = |p: i64| Op::PreisSetzen {
            artikel: a.guid,
            preis: Some(Dez(alt.0 + p * 10_000)),
            stand: "10/2026".into(),
            quelle: "Händler".into(),
        };
        let t1 = schreiben(&t0, &[preis(100)]);
        let lohn = Op::FirmenwertSetzen {
            schluessel: "wage".into(),
            wert: Dez::ganz(65),
        };
        let t2 = schreiben(&t1, &[lohn]);
        let k2 = katalog(&t2);
        let p = protokoll(&k2);
        assert_eq!(p.iter().map(|s| s.stand).collect::<Vec<_>>(), [2, 1]);
        assert!(p[1].eintraege.iter().any(|e| e.op == "preis_setzen"));
        assert_eq!(p[0].eintraege[0].op, "firmenwert_setzen");
        assert_eq!(
            (
                p[0].eintraege[0].alt.as_str(),
                p[0].eintraege[0].neu.as_str()
            ),
            ("60", "65")
        );
        // Stand 2 zurück: Lohn wieder 60
        let ops = umkehr(&k2, &katalog(&t1), 2).unwrap();
        assert_eq!(
            ops,
            [Op::FirmenwertSetzen {
                schluessel: "wage".into(),
                wert: Dez::ganz(60)
            }]
        );
        // Stand 1 zurück (davor Werksbestand): alter Preis als Stand 3
        let ops = umkehr(&k2, &k0, 1).unwrap();
        assert_eq!(ops.len(), 1, "{ops:?}");
        assert!(matches!(&ops[0], Op::PreisSetzen { preis, .. } if *preis == Some(alt)));
        let t3 = schreiben(&t2, &ops);
        let k3 = katalog(&t3);
        assert_eq!(k3.artikel(a.guid).unwrap().preis, Some(alt));
        assert_eq!(k3.werte.lohn, Dez::ganz(65), "Lohn aus Stand 2 bleibt");
        assert_eq!(protokoll(&k3)[0].stand, 3);
        // Stand 3 hat denselben Preis wieder geändert: Stand 1 geht nicht mehr
        let e = umkehr(&k3, &k0, 1).unwrap_err();
        assert_eq!(e[0].regel, 93);
        assert!(
            e[0].satz.contains("in Stand 3 wieder geändert"),
            "{}",
            e[0].satz
        );
        assert!(e[0].satz.contains(&a.name), "{}", e[0].satz);
        // Unbekannter Stand
        assert!(umkehr(&k3, &k0, 9).is_err());
    }

    /// Bauleistung geändert und Artikel neu angelegt: zurück heißt alte
    /// Felder bzw. ausgemustert, nie gelöscht (Regel 87).
    #[test]
    fn bauleistung_und_neuer_artikel() {
        let t0 = sk_model::write_szk(&Library::standard());
        let k0 = katalog(&t0);
        let l = k0.leistungen.iter().find(|l| !l.retired).unwrap();
        let mut d = crate::preis::bauleistung(l);
        d.stunden = Dez(d.stunden.0 + 50_000);
        let t1 = schreiben(
            &t0,
            &[
                Op::BauleistungAendern {
                    bauleistung: l.guid,
                    daten: d,
                },
                Op::ArtikelAnlegen {
                    baustoff: None,
                    name: "Probeartikel".into(),
                    dicke: None,
                    guete: String::new(),
                    format: String::new(),
                    einheit: crate::katalog::Einheit::M2,
                    preis: Some(Dez::ganz(3)),
                    stand: "10/2026".into(),
                    quelle: String::new(),
                    lieferant: String::new(),
                    standard: false,
                },
            ],
        );
        let k1 = katalog(&t1);
        let neu = k1
            .artikel
            .iter()
            .find(|a| a.name == "Probeartikel")
            .unwrap();
        let ops = umkehr(&k1, &k0, 1).unwrap();
        assert!(ops.contains(&Op::BauleistungAendern {
            bauleistung: l.guid,
            daten: crate::preis::bauleistung(l)
        }));
        assert!(ops.contains(&Op::Ausmustern {
            satz: SatzId::neu("article", neu.guid.to_ifc())
        }));
        let t2 = schreiben(&t1, &ops);
        let k2 = katalog(&t2);
        assert_eq!(k2.leistung(l.guid).unwrap().stunden, l.stunden);
        assert!(
            k2.artikel(neu.guid).unwrap().retired,
            "ausgemustert, nicht gelöscht"
        );
    }
}

//! KA-0 Abnahme Nr. 29 (Zwischenspeicher, Bausteingrenze sk-cost §5).
//!
//! Je 50 zufällige Änderungen an RH-1 (Standardhaus) und RH-2 (mehrschalig):
//! Wand verschieben, Typ tauschen, Schicht ändern, Preis im Projekt, Lohn,
//! Umfang. Nach jeder Änderung ist `kosten_mit` gleich `kosten` (jede Zeile,
//! jede Summe, Rundungsausgleich).
//!
//! Zähler `neu_zugeordnet()`: Er zählt Schlüssel (Bauteilart, Typ,
//! Schicht-Fingerabdruck), nicht Typen. Darum prüft der Test die genaue
//! Erwartung: Bleibt der Katalog gleich, ist er die Zahl der Schlüssel, die im
//! letzten Aufruf nicht vorkamen; ändert sich der Katalog (Preis, Lohn), ist er
//! gleich einem kalten Lauf. Nach einer verschobenen Wand heißt das 0, außer
//! die Verschiebung erzeugt ein Bauteil mit einem neuen Schlüssel (etwa eine
//! Untersicht unter einem Überstand); dann genau so viele, wie neu sind.

use sk_cost::{lesen, Dez, Herkunft, HerkunftArt, Kostenspeicher, Op, Rolle, Umfang};
use sk_model::{qto, szo, Guid, GuidGen, Model, TypeCategory};
use std::collections::BTreeSet;

struct Zufall(u64);
impl Zufall {
    fn n(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

fn laden(text: &str) -> Model {
    szo::read_with(text, GuidGen::with_seed(29), &lesen::ABSCHNITTE_SZO)
        .expect("lädt")
        .model
}

/// Was `kosten_mit` als Schlüssel nimmt, von außen nachgebildet.
type Schluessel = (
    String,
    Option<Guid>,
    Vec<(Option<Guid>, u64, &'static str, Option<Guid>, u64)>,
);

fn schluessel(m: &Model, sched: &qto::Schedule, u: &Umfang) -> BTreeSet<Schluessel> {
    let s;
    let s = if u.alles() {
        sched
    } else {
        s = sched.restrict(m, u);
        &s
    };
    let mut out = BTreeSet::new();
    for (_, r) in s.layer_rows(m) {
        let Some(e) = m.element(r.element) else {
            continue;
        };
        let schichten = m
            .element_layers(r.element)
            .iter()
            .map(|l| {
                let mat = m.material(l.material);
                (
                    mat.map(|x| x.guid),
                    l.thickness.to_bits(),
                    szo::layer_function(l.function),
                    l.svc,
                    mat.map_or(0, |x| x.density.to_bits()),
                )
            })
            .collect();
        out.insert((
            format!("{:?}", r.category),
            e.layer_set.and_then(|t| m.layer_set(t)).map(|t| t.guid),
            schichten,
        ));
    }
    out
}

/// Art 0 Wand verschieben, 1 Typ tauschen, 2 Schicht ändern, 3 Preis im
/// Projekt, 4 Lohn, 5 nur Umfang. `true`, wenn sich das Modell geändert hat.
fn aendern(m: &mut Model, art: u64, z: &mut Zufall, u: &mut Umfang) -> bool {
    let h = Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "10:30");
    let rev = (m.revision(), m.ext_revision());
    m.begin("Nr. 29");
    match art {
        0 => {
            let runs: Vec<_> = m.runs().ids().collect();
            let run = runs[z.n(runs.len() as u64) as usize];
            let c = m.chain(run).unwrap();
            let d = if z.n(2) == 0 { -100.0 } else { 100.0 };
            if let Some(neu) =
                c.with_segment_moved(z.n(c.points.len().saturating_sub(1) as u64) as usize, d)
            {
                m.set_run_points(run, &neu.points);
            }
        }
        1 => {
            let runs: Vec<_> = m.runs().ids().collect();
            let run = runs[z.n(runs.len() as u64) as usize];
            let typen: Vec<_> = m
                .layer_sets()
                .iter()
                .filter(|(_, t)| {
                    matches!(
                        t.category,
                        TypeCategory::ExteriorWall | TypeCategory::InteriorWall
                    )
                })
                .map(|(id, _)| id)
                .collect();
            let t = typen[z.n(typen.len() as u64) as usize];
            m.set_run_type(run, t);
        }
        2 => {
            let typen: Vec<_> = m
                .layer_sets()
                .iter()
                .filter(|(id, _)| !m.type_users(*id).is_empty())
                .map(|(id, t)| (id, t.clone()))
                .collect();
            let (id, mut t) = typen[z.n(typen.len() as u64) as usize].clone();
            let i = z.n(t.layers.len() as u64) as usize;
            t.layers[i].thickness += if z.n(2) == 0 { 10.0 } else { -10.0 };
            m.set_layer_set(id, t);
        }
        3 => {
            let k = lesen::katalog(m, None);
            let a = &k.artikel[z.n(k.artikel.len() as u64) as usize];
            let preis = Dez::ganz(10 + z.n(40) as i64);
            sk_cost::ausfuehren(
                m,
                None,
                Rolle::Admin,
                &h,
                Op::PreisSetzen {
                    artikel: a.guid,
                    preis: Some(preis),
                    stand: "10/2026".into(),
                    quelle: "Nr. 29".into(),
                },
            )
            .unwrap();
        }
        4 => {
            sk_cost::ausfuehren(
                m,
                None,
                Rolle::Admin,
                &h,
                Op::FirmenwertSetzen {
                    schluessel: "wage".into(),
                    wert: Dez::ganz(55 + z.n(15) as i64),
                },
            )
            .unwrap();
        }
        _ => {
            let bs: Vec<_> = m.buildings().ids().collect();
            let gs: Vec<_> = m.storeys().ids().collect();
            *u = match z.n(3) {
                0 => Umfang::projekt(),
                1 => Umfang::gebaeude(bs[z.n(bs.len() as u64) as usize]),
                _ => Umfang {
                    gebaeude: None,
                    ohne: vec![gs[z.n(gs.len() as u64) as usize]],
                },
            };
        }
    }
    m.commit();
    (m.revision(), m.ext_revision()) != rev
}

#[test]
fn ka0_29_kosten_mit_gleich_kosten() {
    for (name, text) in [
        ("RH-1", include_str!("../referenz/rh1-standardhaus.szo")),
        ("RH-2", include_str!("../referenz/rh2-mehrschalig.szo")),
    ] {
        let mut m = laden(text);
        let mut z = Zufall(29);
        let mut u = Umfang::projekt();
        let k = lesen::katalog(&m, None);
        let sched = qto::schedule(&m);
        let (erst, mut sp) = lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
        assert_eq!(erst, lesen::kosten(&m, &sched, &k, &u));
        let mut stempel = k.stempel;
        let mut vorher = schluessel(&m, &sched, &u);
        assert_eq!(
            sp.neu_zugeordnet(),
            vorher.len() as u64,
            "{name}: kalter Lauf"
        );
        let mut arten = [0u32; 6];
        let mut geaendert = [0u32; 6];
        let mut wand_neu = Vec::new();
        for schritt in 0..50 {
            let art = z.n(6);
            arten[art as usize] += 1;
            if aendern(&mut m, art, &mut z, &mut u) {
                geaendert[art as usize] += 1;
            }
            let k = lesen::katalog(&m, None);
            let sched = qto::schedule(&m);
            let soll = lesen::kosten(&m, &sched, &k, &u);
            let (ist, neu) = lesen::kosten_mit(sp, &m, &sched, &k, &u);
            assert_eq!(ist, soll, "{name} Schritt {schritt}, Art {art}");
            let jetzt = schluessel(&m, &sched, &u);
            let erwartet = if k.stempel == stempel {
                jetzt.difference(&vorher).count() as u64
            } else {
                jetzt.len() as u64
            };
            assert_eq!(
                neu.neu_zugeordnet(),
                erwartet,
                "{name} Schritt {schritt}, Art {art}: neu zugeordnet"
            );
            match art {
                0 if neu.neu_zugeordnet() > 0 => wand_neu.push((
                    schritt,
                    jetzt
                        .difference(&vorher)
                        .map(|s| s.0.clone())
                        .collect::<Vec<_>>(),
                )),
                0 => assert_eq!(neu.neu_zugeordnet(), 0),
                3 | 4 => {
                    let (_, kalt) =
                        lesen::kosten_mit(Kostenspeicher::default(), &m, &sched, &k, &u);
                    assert_eq!(
                        neu.neu_zugeordnet(),
                        kalt.neu_zugeordnet(),
                        "{name}: wie kalt"
                    );
                }
                _ => {}
            }
            sp = neu;
            stempel = k.stempel;
            vorher = jetzt;
        }
        eprintln!(
            "NR29 {name}: Arten {arten:?}, Modell geändert {geaendert:?}, Wand mit neuem Schlüssel {wand_neu:?}"
        );
    }
}

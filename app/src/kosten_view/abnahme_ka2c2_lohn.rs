//! Abnahme KA-2c2 Nr. 8 (paket-ka2 §6): Lohnfeld für dieses und neue Häuser,
//! Strg+Z nur für dieses Haus, Abgleichzeile, „Nur dieses Haus“ mit Marke.

use super::*;
use crate::catalog::Company;
use sk_cost::{Herkunft, HerkunftArt, Op};

fn haus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .unwrap()
    .model;
    Scene::with_model(m)
}

fn h() -> Herkunft {
    Herkunft::neu(HerkunftArt::Manual, "2026-10-08", "12:50")
}

fn lohn(w: i64) -> Op {
    Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: Dez::ganz(w),
    }
}

fn firma(name: &str) -> (std::path::PathBuf, Company) {
    let d = std::env::temp_dir().join(format!("skizzeo-abnahme-lohn-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    let p = d.join("firmenkatalog.szk");
    let (mut c, _) = Company::load(&p, true);
    c.fuer_firma(&h(), &[lohn(60)]).unwrap();
    (p, c)
}

fn datei_stand(p: &std::path::Path) -> u32 {
    let t = std::fs::read_to_string(p).unwrap();
    let l = t.lines().find(|l| l.starts_with("[catalog]")).unwrap();
    l.split_whitespace()
        .find_map(|w| w.strip_prefix("stand="))
        .unwrap()
        .parse()
        .unwrap()
}

fn logs(p: &std::path::Path) -> usize {
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("[log]"))
        .count()
}

/// (EP, Stunden) je Position.
fn eps(s: &mut Scene, c: &Company) -> Vec<(Cent, Dez)> {
    let f = Some((c.library(), c.stand()));
    let k = s.katalog(f);
    let b = s.kostenblatt(f, &Umfang::projekt());
    b.positionen
        .iter()
        .filter_map(|p| {
            let a = sk_cost::preis::aufbau(s.model(), &k, p)?;
            Some((a.ep, a.stunden))
        })
        .collect()
}

fn lohn_von(s: &mut Scene, c: &Company) -> Dez {
    s.katalog(Some((c.library(), c.stand()))).werte.lohn
}

/// Lohnfeld 65,00 mit Enter: alle Lohn-EP ändern sich um Stunden × 5,
/// `.szk` hat 65, Stand + 1, eine `[log]`-Zeile, voriger Stand abgelegt,
/// Projekt ohne Marke, neues Haus mit 65. Strg+Z: Projekt 60, `.szk` 65,
/// Statuszeile, Abgleichzeile; „so lassen“ blendet sie aus.
#[test]
fn nr8_lohnfeld_fuer_neue_haeuser() {
    let (p, mut c) = firma("nr8");
    let mut s = haus();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, Some((c.library(), c.stand())));
    let vorher = eps(&mut s, &c);
    assert!(vorher.iter().any(|(_, h)| *h != Dez::NULL));
    // Hinweiskarte: 65, Enter
    v.lohn_karte();
    v.sync(&mut s, Some((c.library(), c.stand())));
    let mods = sk_platform::Modifiers::default();
    let t = Theme::dark();
    v.key(&t, sk_platform::Key::Backspace, mods);
    v.key(&t, sk_platform::Key::Backspace, mods);
    for ch in "65".chars() {
        v.text(ch);
    }
    let Some(Some(ListOut::Kosten(Schreiben::Lohn { wert, gilt }))) =
        v.key(&t, sk_platform::Key::Enter, mods)
    else {
        panic!("Enter schreibt")
    };
    assert_eq!((wert, gilt), (Dez::ganz(65), Gilt::NeueHaeuser));
    // wie main::kosten_schreiben
    let (stand, n_log) = (datei_stand(&p), logs(&p));
    let label = s.bezeichnung(lohn_blatt::LohnBlatt::bezeichnung(wert, gilt));
    assert_eq!(label, "Lohn 65,00 €/h für dieses und neue Häuser");
    assert_eq!(s.fuer_firma(label, &mut c, &h(), &[lohn(65)]), Ok(None));
    assert_eq!(s.undo_label(), Some(label));
    let nachher = eps(&mut s, &c);
    for ((a, std), (b, _)) in vorher.iter().zip(&nachher) {
        // EP = Stunden × Lohn + …: genau Stunden × 5 mehr (gerundet)
        let soll = Cent(a.0 + (std.0 * 5 + 5_000) / 10_000);
        if *std != Dez::NULL {
            assert!((b.0 - soll.0).abs() <= 1, "{a:?} → {b:?}, {std:?}");
        } else {
            assert_eq!(a, b);
        }
    }
    assert_eq!(datei_stand(&p), stand + 1);
    assert_eq!(logs(&p), n_log + 1, "eine [log]-Zeile");
    assert!(p
        .parent()
        .unwrap()
        .join("firmenkatalog-staende")
        .join(format!("stand-{stand:04}.szk"))
        .exists());
    let firma_lohn = |c: &Company| {
        let m = sk_model::Model::from_library(c.library());
        sk_cost::lesen::firma_oder_werk(&m, Some(c.library()))
            .werte
            .lohn
    };
    assert_eq!(firma_lohn(&c), Dez::ganz(65));
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert!(v.eigen.is_empty(), "Projekt ohne Marke");
    assert_eq!(v.abgleich_zeile(), None);
    let mut neu = sk_model::Model::from_library(c.library());
    sk_cost::neues_projekt(&mut neu, Some(c.library()));
    assert_eq!(
        sk_cost::lesen::katalog(&neu, Some(c.library())).werte.lohn,
        Dez::ganz(65),
        "neue Datei"
    );
    // Strg+Z: nur dieses Haus
    let szk = std::fs::read(&p).unwrap();
    assert!(s.undo());
    assert_eq!(lohn_von(&mut s, &c), Dez::ganz(60));
    assert_eq!(eps(&mut s, &c), vorher);
    assert_eq!(std::fs::read(&p).unwrap(), szk, ".szk bleibt 65");
    assert_eq!(
        label.strip_suffix(preis_blatt::FUER_NEUE),
        Some("Lohn 65,00 €/h"),
        "Statuszeile „… rechnen weiter mit Lohn 65,00 €/h.“"
    );
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(
        v.abgleich_zeile().as_deref(),
        Some("Für neue Häuser gilt Lohn 65,00 €/h (hier 60,00)")
    );
    // „so lassen“ blendet sie aus, bis die Firma einen neuen Stand hat
    let stand_jetzt = datei_stand(&p);
    s.kosten_folge(
        "Werte für neue Häuser nicht übernommen",
        Some(c.library()),
        &h(),
        &[Op::AbgleichLassen { stand: stand_jetzt }],
    )
    .unwrap();
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(v.abgleich_zeile(), None);
    c.fuer_firma(&h(), &[lohn(66)]).unwrap();
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(
        v.abgleich_zeile().as_deref(),
        Some("Für neue Häuser gilt Lohn 66,00 €/h (hier 60,00)")
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// Kachel Lohnanteil → Lohnfeld: Kosten offen (1), Klick auf „60,00 €/h“
/// (2), 63 und Enter (3) schreibt „Nur dieses Haus“; `.szk` bytegleich,
/// das Projekt trägt den Lohn mit `proj=1`.
#[test]
fn nr8_kachel_nur_dieses_haus() {
    let (p, c) = firma("nr8-kachel");
    let mut s = haus();
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, Some((c.library(), c.stand())));
    let (_, link, _, (x, y, w, hh)) = v.lohnsatz_lage(&t, &fonts).unwrap();
    assert_eq!(link, "60,00 €/h");
    let (x, y) = ((x + w * 0.5) as f64, (y + hh * 0.5) as f64);
    let mut pk = Picking::default();
    let mods = sk_platform::Modifiers::default();
    v.mouse_move(&t, &fonts, &mut pk, x, y);
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut pk, (x, y), mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert!(v.blatt_offen());
    v.key(&t, sk_platform::Key::Backspace, mods);
    v.key(&t, sk_platform::Key::Backspace, mods);
    v.key(&t, sk_platform::Key::Backspace, mods);
    v.key(&t, sk_platform::Key::Backspace, mods);
    v.key(&t, sk_platform::Key::Backspace, mods);
    for ch in "63".chars() {
        v.text(ch);
    }
    let Some(Some(ListOut::Kosten(Schreiben::Lohn { wert, gilt }))) =
        v.key(&t, sk_platform::Key::Enter, mods)
    else {
        panic!("Enter schreibt")
    };
    assert_eq!((wert, gilt), (Dez::ganz(63), Gilt::NurHaus));
    let szk = std::fs::read(&p).unwrap();
    let label = s.bezeichnung(lohn_blatt::LohnBlatt::bezeichnung(wert, gilt));
    assert_eq!(label, "Lohn 63,00 €/h nur für dieses Haus");
    s.kosten_folge(label, Some(c.library()), &h(), &[lohn(63)])
        .unwrap();
    assert_eq!(s.undo_label(), Some(label));
    assert_eq!(std::fs::read(&p).unwrap(), szk, ".szk bytegleich");
    let k = s.katalog(Some((c.library(), c.stand())));
    assert_eq!(k.werte.lohn, Dez::ganz(63));
    assert!(
        k.herkunft_von("rate", "wage").is_some_and(|u| u.proj),
        "Lohn mit proj=1"
    );
    v.sync(&mut s, Some((c.library(), c.stand())));
    assert_eq!(v.lohnsatz_lage(&t, &fonts).unwrap().1, "63,00 €/h");
    assert_eq!(
        v.abgleich_zeile(),
        None,
        "eigener Wert, keine Abgleichzeile"
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

/// Marke „eigener Lohn“ (paket-ka2 §4 und §6 Nr. 8): Nach „Nur dieses
/// Haus“ zeigt die Ansicht, dass der Lohn vom Firmenkatalog abweicht,
/// mit dem Wert der Firma, am Stundenlohn der Kachel.
#[test]
fn nr8_marke_eigener_lohn() {
    let (p, c) = firma("nr8-marke");
    let mut s = haus();
    let t = Theme::dark();
    let fonts = Fonts {
        regular: None,
        bold: None,
        italic: None,
    };
    s.kosten_folge(
        "Lohn 63,00 €/h nur für dieses Haus",
        Some(c.library()),
        &h(),
        &[lohn(63)],
    )
    .unwrap();
    let mut v = KostenView::new();
    (v.w, v.h) = (1200, 900);
    v.sync(&mut s, Some((c.library(), c.stand())));
    let (_, _, _, (x, y, w, hh)) = v.lohnsatz_lage(&t, &fonts).unwrap();
    let tip = v.tip_at(&t, &fonts, (x + w * 0.5) as f64, (y + hh * 0.5) as f64);
    assert!(
        tip.as_deref()
            .is_some_and(|t| t.contains("eigener Lohn") || t.contains("Firma 60,00")),
        "keine Marke „eigener Lohn“ am Stundenlohn: Tooltip {tip:?}, Punkte an EP {}",
        v.eigen.len()
    );
    let _ = std::fs::remove_dir_all(p.parent().unwrap());
}

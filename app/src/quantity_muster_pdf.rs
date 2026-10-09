//! Muster-PDFs des LV-Blatts am Standardhaus RH-1 mit Jörns Projektdaten
//! (paket-projektdaten §6), über denselben Weg wie „LV Rohbau als PDF
//! speichern“: Druckvorschau, `AvaView::pdf`, `document::tabelle_schreiben`.
//! Läuft nur auf Wunsch:
//!
//! `SKIZZEO_MUSTER=<ordner> cargo test -p skizzeo muster_pdf -- --ignored`

use super::*;
use sk_paint::font::Font;
use std::path::{Path, PathBuf};

fn schriften() -> Option<Fonts> {
    let f = Fonts::system();
    if f.regular.is_some() {
        return Some(f);
    }
    let lib = Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| std::fs::read(lib.join(n)).ok().and_then(Font::parse);
    Some(Fonts {
        regular: Some(lade("LiberationSans-Regular.ttf")?),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: lade("LiberationSans-Italic.ttf"),
    })
}

#[test]
#[ignore = "legt Muster-PDFs ab, nur mit SKIZZEO_MUSTER"]
fn muster_pdf() {
    let Some(dir) = std::env::var_os("SKIZZEO_MUSTER").map(PathBuf::from) else {
        return;
    };
    let fonts = schriften().expect("Schrift");
    std::fs::create_dir_all(&dir).unwrap();
    let t = Theme::dark();
    let mods = sk_platform::Modifiers::default();
    let m = sk_model::szo::read_with(
        include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    let mut s = Scene::with_model(m);
    let mut pr = s.model().project().clone();
    pr.kind = "Neubau Einfamilienhaus".into();
    pr.site = "Haus Mustermann".into();
    pr.place = "Musterweg 1\n27777 Ganderkesee".into();
    pr.number = "01/26".into();
    pr.client = "Max Mustermann".into();
    pr.client_addr = "Phantasiestraße 7\n27777 Ganderkesee".into();
    pr.author = "Dipl.-Ing. (FH) Jörn Horstmann".into();
    pr.author_addr = "Denkmalsweg 18b\n27777 Ganderkesee".into();
    assert!(s.projekt_setzen("Projektdaten geändert", pr));
    for preise in [false, true] {
        for wahl in [(false, false), (true, true)] {
            let p = Picking::default();
            let mut q = QuantityWindow::new();
            q.datei = "haus.szo".into();
            q.blatt_wahl = wahl;
            (q.w, q.h) = (1440, 960);
            q.sync(&mut s, &p, false);
            q.waehlen(Blatt::Ava, &t);
            q.sync(&mut s, &p, false);
            let a = q.ava.as_mut().unwrap();
            if a.preise != preise {
                let xy = a.schalter_mitte(&t, &fonts, usize::from(preise));
                let mut p2 = Picking::default();
                a.mouse_down(&t, &fonts, &mut p2, xy, mods);
                q.sync(&mut s, &p, false);
            }
            let a = q.ava.as_mut().unwrap();
            assert_eq!(a.preise, preise);
            a.zeige_druckvorschau(0);
            q.sync(&mut s, &p, false);
            let (name, bytes) = q.ava.as_ref().unwrap().pdf(&fonts).expect("PDF");
            let zusatz = if wahl.0 {
                "-titelblatt-verzeichnis"
            } else {
                ""
            };
            let name = name.replace(".pdf", &format!("{zusatz}.pdf"));
            let pfad = dir.join(&name);
            crate::document::tabelle_schreiben(&pfad, &bytes).unwrap();
            println!("{name} {}", bytes.len());
        }
    }
}

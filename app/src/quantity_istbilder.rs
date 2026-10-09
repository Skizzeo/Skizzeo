//! Ist-Bilder des Mengenfensters am Standardhaus RH-1, passend zu den
//! Sollbildern `soll-ka-*` der Einstellungen. Läuft nur auf Wunsch:
//!
//! `SKIZZEO_ISTBILDER=<ordner> cargo test -p skizzeo istbilder -- --ignored`
//!
//! Ohne Windows-Schriften nimmt es Liberation Sans (gleich breit wie Arial),
//! wenn sie da ist; ohne Schrift legt es keine Bilder ab.

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

fn standardhaus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

struct Bilder {
    t: Theme,
    fonts: Fonts,
    dir: PathBuf,
    p: Picking,
}

impl Bilder {
    /// Fenster `w`×`h` auf `blatt`, Animationen fertig.
    fn fenster(&mut self, s: &mut Scene, w: u32, h: u32, blatt: Blatt) -> QuantityWindow {
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (w, h);
        q.sync(s, &self.p, false);
        q.waehlen(blatt, &self.t);
        q.sync(s, &self.p, false);
        q
    }

    fn ablegen(&self, q: &mut QuantityWindow, name: &str) {
        let spaeter = Instant::now() + Duration::from_secs(5);
        q.tick(&self.t, spaeter);
        let c = q.paint(&self.t, &self.fonts, spaeter);
        let pfad = self.dir.join(name);
        std::fs::write(&pfad, c.to_png()).unwrap_or_else(|e| panic!("{pfad:?}: {e}"));
    }
}

#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka1_ka2() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let Some(fonts) = schriften() else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut b = Bilder {
        t: Theme::dark(),
        fonts,
        dir,
        p: Picking::default(),
    };
    let mods = sk_platform::Modifiers::default();

    // KA-1: Karten über dem Mengenblatt
    let mut s = standardhaus();
    let mut q = b.fenster(&mut s, 1240, 820, Blatt::Mengen);
    b.ablegen(&mut q, "ist-ka-1-karten.png");

    // Jörn 08.10.: schmales Fenster bei 125 %, Gliedern nach Gewerk; der
    // Knopf liegt nicht mehr hinter dem Umschalter
    let mut q = QuantityWindow::new();
    q.title.scale = 1.25;
    q.grouping = Grouping::Trade;
    (q.w, q.h) = (820, 500);
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-mengen-schmal.png");

    // KA-1b: Umfang EG + OG (RH-1 hat ein Gebäude, also ohne Gebäudefeld)
    let mut q = b.fenster(&mut s, 1240, 400, Blatt::Kosten);
    let k = q.kosten.as_mut().unwrap();
    let chips = k.leiste.chips().to_vec();
    assert!(crate::umfang_view::klick(
        &mut k.leiste.umfang,
        &chips,
        1,
        false
    ));
    assert!(crate::umfang_view::klick(
        &mut k.leiste.umfang,
        &chips,
        2,
        false
    ));
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-1b-umfang.png");

    // KA-2: Reiter Kosten beim ersten Öffnen mit der Hinweiskarte zum Lohn
    let mut q = b.fenster(&mut s, 1240, 1060, Blatt::Kosten);
    q.kosten.as_mut().unwrap().lohn_karte();
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-2-kosten.png");

    // Befund Q: schmal bei 150 % rutscht „Gliedern“ unter „Preise“
    let mut q = QuantityWindow::new();
    q.title.scale = 1.5;
    (q.w, q.h) = (720, 1000);
    q.sync(&mut s, &b.p, false);
    q.waehlen(Blatt::Kosten, &b.t);
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-kosten-schmal-150.png");

    // KA-2b: nur EG
    let mut q = b.fenster(&mut s, 1240, 880, Blatt::Kosten);
    let k = q.kosten.as_mut().unwrap();
    let chips = k.leiste.chips().to_vec();
    assert!(crate::umfang_view::klick(
        &mut k.leiste.umfang,
        &chips,
        1,
        true
    ));
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-2b-kosten-eg.png");

    // KA-2c: Preisblatt am EP Mauerwerk 17,5, Planstein 20,50 getippt
    let mut q = b.fenster(&mut s, 1240, 930, Blatt::Kosten);
    b.p = Picking::default();
    let k = q.kosten.as_mut().unwrap();
    let xy = k
        .ep_mitte(&b.t, "Porenbeton-Planstein")
        .expect("EP Mauerwerk");
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s, &b.p, false);
    let k = q.kosten.as_mut().unwrap();
    assert!(k.preis_offen());
    for ch in "20,50".chars() {
        k.text(ch);
    }
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-2c-preis-haus.png");

    // KA-3a7: dasselbe Preisblatt, Planstein je Stück 0,85
    let mut q = b.fenster(&mut s, 1240, 930, Blatt::Kosten);
    b.p = Picking::default();
    let k = q.kosten.as_mut().unwrap();
    let xy = k
        .ep_mitte(&b.t, "Porenbeton-Planstein")
        .expect("EP Mauerwerk");
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s, &b.p, false);
    let k = q.kosten.as_mut().unwrap();
    assert!(k.preis_offen());
    let je = k.je_mitte(&b.t, &b.fonts, 2).expect("Segment je Stück");
    k.mouse_down(&b.t, &b.fonts, &mut b.p, je, mods);
    assert!(k.preis_offen(), "Klick aufs Segment lässt das Blatt offen");
    for ch in "0,85".chars() {
        k.text(ch);
    }
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-3a7-preisblatt-je-stueck.png");

    // KA-3b4: dasselbe Preisblatt, Kennwort hier nicht eingegeben
    let mut q = b.fenster(&mut s, 1240, 930, Blatt::Kosten);
    b.p = Picking::default();
    let k = q.kosten.as_mut().unwrap();
    k.set_vorschlag(true, false);
    // paket-ka3b §3: der Werksablauf `kind=user` oben links vom Knopf
    let werk: Vec<_> = sk_cost::ablauf::lesen(&[sk_cost::WERK])
        .into_iter()
        .filter(|a| a.zugang == sk_cost::ablauf::Zugang::User)
        .map(|a| (a.guid, a.name))
        .collect();
    k.set_ablaeufe(&werk);
    let xy = k
        .ep_mitte(&b.t, "Porenbeton-Planstein")
        .expect("EP Mauerwerk");
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    k.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s, &b.p, false);
    let k = q.kosten.as_mut().unwrap();
    assert!(k.preis_offen());
    for ch in "20,50".chars() {
        k.text(ch);
    }
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-3b4-preisblatt-vorschlagen.png");

    // KA-2d: Firma Lohn 65 nach der Projektkopie, Maus auf „übernehmen“
    let d = std::env::temp_dir().join(format!("skizzeo-istbilder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let (mut c, _) = crate::catalog::Company::laden(&d.join("firmenkatalog.szk"), true);
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "11:34");
    let lohn = |v: i64| sk_cost::Op::FirmenwertSetzen {
        schluessel: "wage".into(),
        wert: sk_cost::Dez::ganz(v),
    };
    c.fuer_firma(&h, &[lohn(60)]).unwrap();
    let mut s = standardhaus();
    let k = s.katalog(Some((c.library(), c.stand())));
    let art = k.artikel.iter().find(|a| a.preis.is_some()).unwrap();
    let preis = sk_cost::Op::PreisSetzen {
        artikel: art.guid,
        preis: art.preis,
        stand: "10/2026".into(),
        quelle: "Preisblatt".into(),
        eingabe: String::new(),
    };
    s.kosten_folge("Preis", Some(c.library()), &h, &[preis])
        .unwrap();
    c.fuer_firma(&h, &[lohn(65)]).unwrap();
    let firma = Some((c.library(), c.stand()));
    b.p = Picking::default();
    let mut q = QuantityWindow::new();
    (q.w, q.h) = (1240, 900);
    q.sync_mit(&mut s, &b.p, firma, false);
    q.waehlen(Blatt::Kosten, &b.t);
    q.sync_mit(&mut s, &b.p, firma, false);
    let k = q.kosten.as_mut().unwrap();
    assert!(k.abgleich_zeile().is_some(), "Abgleichzeile");
    let (x, y) = k.uebernehmen_mitte(&b.t, &b.fonts).unwrap();
    k.mouse_move(&b.t, &b.fonts, &mut b.p, x, y);
    q.sync_mit(&mut s, &b.p, firma, false);
    b.ablegen(&mut q, "ist-ka-2d-abgleich.png");
    // KA-3a5: „Unterschiede ansehen“ offen
    let k = q.kosten.as_mut().unwrap();
    let (x, y) = k.ansehen_mitte(&b.t, &b.fonts).unwrap();
    let mods = sk_platform::Modifiers::default();
    k.mouse_down(&b.t, &b.fonts, &mut b.p, (x, y), mods);
    k.mouse_move(&b.t, &b.fonts, &mut b.p, x, y);
    assert!(k.unterschiede_offen());
    q.sync_mit(&mut s, &b.p, firma, false);
    b.ablegen(&mut q, "ist-ka-3a5-unterschiede.png");
    let _ = std::fs::remove_dir_all(&d);

    // KA-2e: „Bauleistung wählen …“ an Dachterrasse · Dämmung hart
    let mut s = standardhaus();
    b.p = Picking::default();
    let mut q = b.fenster(&mut s, 1240, 960, Blatt::Kosten);
    let k = q.kosten.as_mut().unwrap();
    let (x, y) = k
        .waehlen_mitte(&b.t, &b.fonts, "Dämmung hart")
        .or_else(|| k.waehlen_mitte(&b.t, &b.fonts, ""))
        .expect("graue Zeile mit Wahl");
    k.mouse_move(&b.t, &b.fonts, &mut b.p, x, y);
    k.mouse_down(&b.t, &b.fonts, &mut b.p, (x, y), mods);
    q.sync(&mut s, &b.p, false);
    assert!(q.kosten.as_ref().unwrap().blatt_offen());
    b.ablegen(&mut q, "ist-ka-2e-bauleistung-waehlen.png");
}

#[test]
#[ignore = "legt Ist-Bilder ab, nur mit SKIZZEO_ISTBILDER"]
fn istbilder_ka4() {
    let Some(dir) = std::env::var_os("SKIZZEO_ISTBILDER").map(PathBuf::from) else {
        return;
    };
    let Some(fonts) = schriften() else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut b = Bilder {
        t: Theme::dark(),
        fonts,
        dir,
        p: Picking::default(),
    };
    let mods = sk_platform::Modifiers::default();

    // KA-4: LV beim Öffnen, Maus auf der Stb-Decke
    let mut s = standardhaus();
    let mut q = b.fenster(&mut s, 1440, 760, Blatt::Ava);
    let a = q.ava.as_mut().unwrap();
    let (x, y) = a.position_mitte(&b.t, "Stb-Decke").expect("Stb-Decke");
    a.mouse_move(&b.t, &b.fonts, &mut b.p, x, y);
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-4-lv.png");

    // Jörn 09.10.: LV in schmaleren Fenstern bei 125 %; jede Position
    // zeigt ihren Kurztext, Menge und Kurztext überdecken sich nicht
    for (name, w, h, scale) in [
        ("ist-lv-schmal-150.png", 1160, 1170, 1.5),
        ("ist-lv-schmal-125.png", 1000, 900, 1.25),
        ("ist-lv-breit-100.png", 1440, 900, 1.0),
    ] {
        b.p = Picking::default();
        let mut q = QuantityWindow::new();
        q.title.scale = scale;
        (q.w, q.h) = (w, h);
        q.sync(&mut s, &b.p, false);
        q.waehlen(Blatt::Ava, &b.t);
        q.sync(&mut s, &b.p, false);
        b.ablegen(&mut q, name);
    }

    // Bedienbarkeit 23: unter 800 dip wird der Baum zur Leiste „LV ▾“;
    // 480 dip: Kurztext ganz in der zweiten Zeile; aufgeklappt als Blatt
    for (name, w, h, scale, auf) in [
        ("ist-lv-leiste-150.png", 960, 1100, 1.5, false),
        ("ist-lv-leiste-100.png", 480, 900, 1.0, false),
        ("ist-lv-leiste-offen-150.png", 960, 1100, 1.5, true),
    ] {
        b.p = Picking::default();
        let mut q = QuantityWindow::new();
        q.title.scale = scale;
        (q.w, q.h) = (w, h);
        q.sync(&mut s, &b.p, false);
        q.waehlen(Blatt::Ava, &b.t);
        q.sync(&mut s, &b.p, false);
        if auf {
            let a = q.ava.as_mut().unwrap();
            let xy = a.leiste_mitte(&b.t, &b.fonts).expect("Leiste");
            a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
            q.sync(&mut s, &b.p, false);
        }
        b.ablegen(&mut q, name);
    }

    // KA-4b: Position Planstein gewählt, Detail mit Mengenansatz
    b.p = Picking::default();
    let mut q = b.fenster(&mut s, 1440, 960, Blatt::Ava);
    let a = q.ava.as_mut().unwrap();
    let xy = a.position_mitte(&b.t, "Planstein").expect("Planstein");
    a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s, &b.p, false);
    b.ablegen(&mut q, "ist-ka-4b-position.png");

    // KA-4c: Für Anfrage (leer)
    b.p = Picking::default();
    let mut q = b.fenster(&mut s, 1440, 760, Blatt::Ava);
    let a = q.ava.as_mut().unwrap();
    let xy = a.schalter_mitte(&b.t, &b.fonts, 0);
    a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s, &b.p, false);
    assert!(!q.ava.as_ref().unwrap().preise);
    b.ablegen(&mut q, "ist-ka-4c-anfrage.png");

    // KA-4c: Geschosse als Untertitel, „Mehr“ offen, für Anfrage
    let mut s4 = standardhaus();
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "15:00");
    let op = sk_cost::Op::LvGliederungSetzen { untertitel: true };
    s4.kosten_folge("LV nach Geschossen gegliedert", None, &h, &[op])
        .unwrap();
    b.p = Picking::default();
    let mut q = QuantityWindow::new();
    q.datei = "haus.szo".into();
    (q.w, q.h) = (1440, 1000);
    q.sync(&mut s4, &b.p, false);
    q.waehlen(Blatt::Ava, &b.t);
    q.sync(&mut s4, &b.p, false);
    let a = q.ava.as_mut().unwrap();
    let xy = a.schalter_mitte(&b.t, &b.fonts, 0);
    a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    let xy = a.kopf_mitte(&b.t, &b.fonts, "Mehr").expect("Mehr");
    a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut s4, &b.p, false);
    b.ablegen(&mut q, "ist-ka-4c-untertitel.png");

    // KA-4c, Paket PD-3: Kopf aufgeklappt, nur anzeigend, mit Jörns
    // Projektdaten (Aufsteller ohne Verfasser: „fehlt“)
    b.p = Picking::default();
    let mut sp = standardhaus();
    let mut pr = sp.model().project().clone();
    pr.kind = "Neubau Einfamilienhaus".into();
    pr.site = "Haus Mustermann".into();
    pr.place = "Musterweg 1\n27777 Ganderkesee".into();
    pr.number = "01/26".into();
    pr.client = "Max Mustermann".into();
    pr.client_addr = "Phantasiestraße 7\n27777 Ganderkesee".into();
    assert!(sp.projekt_setzen("Projektdaten geändert", pr));
    let mut q = QuantityWindow::new();
    q.datei = "haus.szo".into();
    (q.w, q.h) = (1440, 860);
    q.sync(&mut sp, &b.p, false);
    q.waehlen(Blatt::Ava, &b.t);
    q.sync(&mut sp, &b.p, false);
    let a = q.ava.as_mut().unwrap();
    let xy = a.kopf_mitte(&b.t, &b.fonts, "Kopf").expect("Kopf");
    a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
    q.sync(&mut sp, &b.p, false);
    b.ablegen(&mut q, "ist-ka-4c-kopf.png");

    // PD-4: Druckvorschau des LV-Blatts mit Jörns Projektdaten und Planung;
    // breit Seite 1, mit Titelblatt und Verzeichnis die Verzeichnisseite,
    // schmal die Leiste über dem Blatt
    let mut pr = sp.model().project().clone();
    pr.author = "Dipl.-Ing. (FH) Jörn Horstmann".into();
    pr.author_addr = "Denkmalsweg 18b\n27777 Ganderkesee".into();
    assert!(sp.projekt_setzen("Projektdaten geändert", pr));
    for (name, w, h, scale, wahl, seite) in [
        (
            "ist-lv-druckvorschau.png",
            1440,
            960,
            1.0,
            (false, false),
            0,
        ),
        (
            "ist-lv-druckvorschau-verzeichnis.png",
            1440,
            960,
            1.0,
            (true, true),
            1,
        ),
        (
            "ist-lv-druckvorschau-schmal-150.png",
            720,
            1100,
            1.5,
            (false, false),
            1,
        ),
    ] {
        b.p = Picking::default();
        let mut q = QuantityWindow::new();
        q.datei = "haus.szo".into();
        q.title.scale = scale;
        q.blatt_wahl = wahl;
        (q.w, q.h) = (w, h);
        q.sync(&mut sp, &b.p, false);
        q.waehlen(Blatt::Ava, &b.t);
        q.sync(&mut sp, &b.p, false);
        let a = q.ava.as_mut().unwrap();
        a.zeige_druckvorschau(seite);
        q.sync(&mut sp, &b.p, false);
        b.ablegen(&mut q, name);
    }

    // KA-4d: Zusammenstellung und Prüfen
    for (name, zusammen) in [
        ("ist-ka-4d-zusammenstellung.png", true),
        ("ist-ka-4d-pruefen.png", false),
    ] {
        b.p = Picking::default();
        let mut q = b.fenster(&mut s, 1440, 760, Blatt::Ava);
        let a = q.ava.as_mut().unwrap();
        let xy = a
            .knoten_mitte(&b.t, |k| match k {
                crate::ava_view::Knoten::Zusammenstellung => zusammen,
                crate::ava_view::Knoten::Pruefen(_) => !zusammen,
                _ => false,
            })
            .expect("Knoten");
        a.mouse_down(&b.t, &b.fonts, &mut b.p, xy, mods);
        q.sync(&mut s, &b.p, false);
        b.ablegen(&mut q, name);
    }
}

use super::*;
use sk_ui::theme::Theme;

impl AvaView {
    fn zeilen(&self) -> &[Zeile] {
        &self.zeilen
    }

    fn baum(&self) -> &[Knoten] {
        &self.baum
    }

    fn lv(&self) -> Option<&Lv> {
        self.lv.as_deref()
    }

    fn ansicht(&self) -> Ansicht {
        self.ansicht
    }

    fn detail(&self) -> Option<&Detail> {
        self.detail.as_ref()
    }
}

fn haus() -> Scene {
    let m = sk_model::szo::read_with(
        include_str!("../../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
        sk_model::GuidGen::with_seed(1),
        &sk_cost::lesen::ABSCHNITTE_SZO,
    )
    .expect("lädt")
    .model;
    Scene::with_model(m)
}

fn leer() -> Fonts {
    Fonts {
        regular: None,
        bold: None,
        italic: None,
    }
}

fn blatt(s: &mut Scene) -> AvaView {
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 900);
    v.top = 120.0;
    v.sync(s, None);
    v
}

fn mitte(v: &AvaView, i: usize) -> (f64, f64) {
    let t = Theme::dark();
    let (tx, _) = v.tabelle_x(&t);
    let (_, y, h) = v
        .sichtbar()
        .into_iter()
        .find(|(j, _, _)| *j == i)
        .expect("sichtbar");
    ((tx + 40.0) as f64, (y + h * 0.5) as f64)
}

/// KA-4b: Beim Öffnen steht das erste Los aufgeklappt mit seinen Titeln
/// im Baum, die Tabelle zeigt nur Titel mit Positionen, Titelsumme = Σ GP,
/// die Karte nennt Los und Anzahl.
#[test]
fn oeffnen_zeigt_das_erste_los() {
    let mut s = haus();
    let v = blatt(&mut s);
    let lv = v.lv().expect("LV");
    let Knoten::Los {
        name,
        anzahl,
        offen,
        ..
    } = &v.baum()[0]
    else {
        panic!("{:?}", v.baum()[0]);
    };
    assert!(offen);
    assert_eq!(*anzahl, lv.anzahl());
    assert_eq!(*name, format!("Los {}", lv.kopf.los));
    assert_eq!(
        v.karte,
        format!("LV {} · {} Pos.", lv.kopf.los, lv.anzahl())
    );
    // Alle Titel des Loses im Baum, auch leere
    let titel_im_baum = v
        .baum()
        .iter()
        .filter(|k| matches!(k, Knoten::Titel { .. }))
        .count();
    assert!(titel_im_baum >= lv.titel.len());
    // Unten Prüfen und die Druckvorschau
    let n = v.baum().len();
    assert!(matches!(v.baum()[n - 2], Knoten::Pruefen(_)));
    assert_eq!(v.baum().last(), Some(&Knoten::Blatt));
    // Tabelle: Titel mit Positionen, Summe = Σ GP der Positionen
    let z = v.zeilen();
    let titel: Vec<&Zeile> = z.iter().filter(|x| x.art == Art::Titel).collect();
    assert_eq!(
        titel.len(),
        lv.titel.iter().filter(|t| !t.positionen.is_empty()).count()
    );
    for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
        let summe: i64 = t.positionen.iter().filter_map(|p| p.gp).map(|c| c.0).sum();
        if let Some(ts) = t.summe {
            assert_eq!(ts.0, summe, "{}", t.name);
        }
    }
    assert_eq!(
        z.iter().filter(|x| x.art == Art::Position).count(),
        lv.anzahl()
    );
    assert!(
        v.subtitle.starts_with(&format!("LV {}", lv.kopf.los)),
        "{}",
        v.subtitle
    );
}

/// Klick auf eine Position: Auswahl im Modell, Detail mit Mengenansatz,
/// dessen Zeilen die Positionsmenge ergeben; Esc-Weg `schliessen`.
#[test]
fn klick_oeffnet_den_mengenansatz() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.art == Art::Position && !z.elemente.is_empty())
        .expect("Position");
    let oz = v.zeilen()[i].oz.clone();
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let out = v.mouse_down(&t, &leer(), &mut p, mitte(&v, i), mods);
    assert_eq!(out, Some(ListOut::Picking { selection: true }));
    assert_eq!(p.selected, v.zeilen()[i].elemente);
    v.sync(&mut s, None);
    let d = v.detail().expect("Detail");
    assert_eq!(d.oz, oz);
    let pos = v
        .lv()
        .unwrap()
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.oz == oz)
        .unwrap();
    assert_eq!(d.ansatz.len(), pos.ansatz.len());
    let summe: i64 = pos.ansatz.iter().map(|a| a.menge.0).sum();
    assert_eq!(summe, pos.menge.0, "Ansatz ergibt die Menge");
    assert!(
        d.preis.iter().any(|l| l.starts_with("Lohn ")),
        "{:?}",
        d.preis
    );
    assert!(
        d.preis.iter().any(|l| l.starts_with("Gewerk ")),
        "{:?}",
        d.preis
    );
    assert!(v.schliessen());
    v.sync(&mut s, None);
    assert!(v.detail().is_none());
}

/// Test B2 (KA-3a2): „Bauleistung öffnen ↗“ unter der Kurzform der
/// Preisanteile öffnet die Verwaltung mit der Bauleistung der Position.
#[test]
fn bauleistung_oeffnen() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let i = v
        .zeilen()
        .iter()
        .position(|z| z.art == Art::Position && !z.elemente.is_empty())
        .expect("Position");
    let oz = v.zeilen()[i].oz.clone();
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    v.mouse_down(&t, &leer(), &mut p, mitte(&v, i), mods);
    v.sync(&mut s, None);
    let quelle = v
        .lv()
        .unwrap()
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.oz == oz)
        .unwrap()
        .quelle;
    let ((x, y, w, h), _) = v.oeffnen_lage(&t, &leer()).expect("Verweis");
    let mitte = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    assert_eq!(v.hit(&t, &leer(), mitte.0, mitte.1), Some(Hot::Oeffnen));
    let out = v.mouse_down(&t, &leer(), &mut p, mitte, mods);
    assert_eq!(out, Some(ListOut::Verwaltung(quelle)));
    // Andere Reiter: kein Verweis
    v.reiter = Reiter::Eigenschaften;
    assert!(v.oeffnen_lage(&t, &leer()).is_none());
}

/// „Für Anfrage (leer)“: keine Preise im LV, Zusammenstellung ohne
/// Beträge; zurück „Mit Preisen“.
#[test]
fn anfrage_ohne_preise() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let netto = v.lv().unwrap().zusammenstellung.netto;
    assert!(netto.is_some());
    let (_, segs) = v.schalter(&t, &leer());
    let (_, (x, y, w, h)) = segs[0];
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let xy = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    assert_eq!(
        v.mouse_down(&t, &leer(), &mut p, xy, mods),
        Some(ListOut::Repaint)
    );
    v.sync(&mut s, None);
    assert!(!v.preise);
    let lv = v.lv().unwrap();
    assert!(lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .all(|p| p.ep.is_none()));
    assert_eq!(lv.zusammenstellung.netto, None);
    v.preise = true;
    v.sync(&mut s, None);
    assert_eq!(v.lv().unwrap().zusammenstellung.netto, netto);
}

/// Baum: Zusammenstellung zeigt netto, MwSt., brutto; Prüfen die Befunde
/// des Loses; ein Klick auf einen Titel springt zurück ins LV.
#[test]
fn baum_wechselt_die_ansicht() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let klick = |v: &mut AvaView, p: &mut Picking, pred: &dyn Fn(&Knoten) -> bool| {
        let (i, (x, y, w, h)) = v
            .baum_lage(&t)
            .into_iter()
            .find(|(i, _)| pred(&v.baum()[*i]))
            .expect("Knoten");
        let _ = i;
        v.mouse_down(
            &t,
            &leer(),
            p,
            ((x + w * 0.5) as f64, (y + h * 0.5) as f64),
            mods,
        )
    };
    klick(&mut v, &mut p, &|k| *k == Knoten::Zusammenstellung);
    v.sync(&mut s, None);
    assert_eq!(v.ansicht(), Ansicht::Zusammenstellung);
    let summen: Vec<&str> = v
        .zeilen()
        .iter()
        .filter(|z| z.art == Art::Summe)
        .map(|z| z.text.as_str())
        .collect();
    assert_eq!(summen.len(), 3, "{summen:?}");
    assert_eq!(summen[0], "Summe netto");
    klick(&mut v, &mut p, &|k| matches!(k, Knoten::Pruefen(_)));
    v.sync(&mut s, None);
    assert_eq!(v.ansicht(), Ansicht::Pruefen);
    let n = v.lv().unwrap().befunde.len();
    assert_eq!(
        v.zeilen()
            .iter()
            .filter(|z| matches!(z.art, Art::Befund(_)))
            .count(),
        n
    );
    klick(
        &mut v,
        &mut p,
        &|k| matches!(k, Knoten::Titel { anzahl, .. } if *anzahl > 0),
    );
    v.sync(&mut s, None);
    assert_eq!(v.ansicht(), Ansicht::Lv);
}

/// Zeichnen mit echten Schriften (wenn da) in mehreren Größen, mit Detail
/// und in allen Ansichten: kein Absturz, Pixel geschrieben.
#[test]
fn zeichnet_in_allen_lagen() {
    let t = Theme::dark();
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| std::fs::read(lib.join(n)).ok().and_then(Font::parse);
    let fonts = Fonts {
        regular: lade("LiberationSans-Regular.ttf"),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    };
    let mut s = haus();
    for (w, h, scale) in [(1400, 900, 1.0), (700, 500, 1.25), (300, 200, 1.5)] {
        let mut v = AvaView::new();
        (v.w, v.h, v.scale) = (w, h, scale);
        v.top = 120.0;
        v.sync(&mut s, None);
        v.gewaehlt = v
            .zeilen()
            .iter()
            .find(|z| z.art == Art::Position)
            .map(|z| z.oz.clone());
        v.sync(&mut s, None);
        for a in [Ansicht::Lv, Ansicht::Zusammenstellung, Ansicht::Pruefen] {
            v.zeige(a);
            v.sync(&mut s, None);
            let mut c = Canvas::new(w as usize, h as usize);
            v.paint(&mut c, &t, &fonts, Instant::now());
        }
    }
}

/// Bauteilnummern im Mengenansatz: durchgehende Reihe kurz, sonst Komma.
#[test]
fn nummern_kurz_als_reihe() {
    let n = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(
        nummern_kurz(&n(&["AW-001", "AW-002", "AW-003", "AW-004"])),
        "AW-001 … 004"
    );
    assert_eq!(nummern_kurz(&n(&["AW-001", "AW-002"])), "AW-001, AW-002");
    assert_eq!(
        nummern_kurz(&n(&["AW-001", "AW-003", "AW-004"])),
        "AW-001, AW-003, AW-004"
    );
    assert_eq!(
        nummern_kurz(&n(&["AW-001", "IW-002", "IW-003"])),
        "AW-001, IW-002, IW-003"
    );
    assert_eq!(nummern_kurz(&n(&["DT-001"])), "DT-001");
}

/// Planstein: Kostengruppe aus den Bauteilen (331), nie „Ohne
/// Kostengruppe“ in der Preisspalte; Rundungsausgleich ohne Geschoss.
#[test]
fn detail_kostengruppe_und_ausgleich() {
    let mut s = haus();
    let v = blatt(&mut s);
    let lv = v.lv().unwrap();
    let kat = s.katalog(None);
    for p in lv.titel.iter().flat_map(|t| &t.positionen) {
        let d = detail(s.model(), &kat, lv, &p.oz).expect("Detail");
        assert!(
            d.preis.iter().all(|l| !l.contains("Ohne Kostengruppe")),
            "{:?}",
            d.preis
        );
        for a in &d.ansatz {
            if a[1] == "Rundungsausgleich" {
                assert!(a[0].is_empty() && a[3].is_empty(), "{a:?}");
            }
        }
    }
    let p = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.kurztext.contains("Planstein"))
        .unwrap();
    assert_eq!(p.kg, Some(331), "{}", p.kurztext);
}

/// Liberation Sans, wenn da (der Kopf misst seine Knöpfe mit der Schrift).
fn schrift() -> Option<Fonts> {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| std::fs::read(lib.join(n)).ok().and_then(Font::parse);
    let f = Fonts::system();
    if f.regular.is_some() {
        return Some(f);
    }
    Some(Fonts {
        regular: Some(lade("LiberationSans-Regular.ttf")?),
        bold: lade("LiberationSans-Bold.ttf"),
        italic: None,
    })
}

fn klick_auf(v: &mut AvaView, fonts: &Fonts, p: &mut Picking, hot: Hot) -> Option<ListOut> {
    let t = Theme::dark();
    let s = v.scale;
    let (x0, cw) = v.content_x(&t);
    let y0 = v.top_px();
    // Raster über Kopfzeile und Kopf absuchen
    let mut y = y0;
    while y < y0 + 400.0 * s {
        let mut x = x0;
        while x < x0 + cw {
            if v.hit(&t, fonts, x as f64, y as f64) == Some(hot) {
                let mods = sk_platform::Modifiers::default();
                return v.mouse_down(&t, fonts, p, (x as f64, y as f64), mods);
            }
            x += 4.0;
        }
        y += 4.0;
    }
    panic!("{hot:?} nicht gefunden");
}

/// KA-4c, Paket PD-3: „Bauherr fehlt“, „Projektdaten …“ und jede Zeile
/// des aufgeklappten Kopfs öffnen die Maske „Projektdaten“ im passenden
/// Feld; der Kopf selbst schreibt nichts. Nach einem Schritt über die Maske
/// ist der Hinweis weg.
#[test]
fn bauherr_fehlt_oeffnet_die_maske() {
    let Some(fonts) = schrift() else {
        return;
    };
    let mut s = haus();
    let mut v = blatt(&mut s);
    let mut p = Picking::default();
    assert!(s.model().project().client.is_empty());
    assert!(v.bauherr_fehlt());
    assert_eq!(
        klick_auf(&mut v, &fonts, &mut p, Hot::BauherrFehlt),
        Some(ListOut::Projektdaten(4))
    );
    assert_eq!(
        klick_auf(&mut v, &fonts, &mut p, Hot::Projektdaten),
        Some(ListOut::Projektdaten(0))
    );
    let vorher = v.body_top();
    assert_eq!(
        klick_auf(&mut v, &fonts, &mut p, Hot::Kopf),
        Some(ListOut::Repaint)
    );
    assert!(
        v.kopf_offen && v.body_top() > vorher,
        "Kopf schiebt Baum und Tabelle"
    );
    for (f, i) in [
        (kopf::Feld::Bauvorhaben, 1),
        (kopf::Feld::Projekt, 0),
        (kopf::Feld::Projektnummer, 3),
        (kopf::Feld::Bauherr, 4),
        (kopf::Feld::Aufsteller, 6),
    ] {
        assert_eq!(
            klick_auf(&mut v, &fonts, &mut p, Hot::Feld(f)),
            Some(ListOut::Projektdaten(i)),
            "{f:?}"
        );
    }
    assert!(s.undo_label().is_none(), "der Kopf schreibt nichts");
    let mut pr = s.model().project().clone();
    pr.client = "Familie Muster".into();
    assert!(s.projekt_setzen("Projektdaten geändert", pr));
    v.sync(&mut s, None);
    assert!(!v.bauherr_fehlt());
    assert_eq!(
        v.lv().unwrap().kopf.bauherr.as_deref(),
        Some("Familie Muster")
    );
    assert!(s.undo());
    v.sync(&mut s, None);
    assert!(v.bauherr_fehlt());
}

/// Bauvorhaben ohne `Project.site` aus dem Dateinamen; „Mehr“ ›
/// „Geschosse als Untertitel“ gibt `LvGliederungSetzen`, danach sind die
/// OZ dreistufig.
#[test]
fn bauvorhaben_und_untertitel() {
    let Some(fonts) = schrift() else {
        return;
    };
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 900);
    v.top = 120.0;
    v.datei = "haus.szo".into();
    v.sync(&mut s, None);
    assert!(v.subtitle.contains("Bauvorhaben Haus"), "{}", v.subtitle);
    let mut p = Picking::default();
    assert_eq!(
        klick_auf(&mut v, &fonts, &mut p, Hot::Mehr),
        Some(ListOut::Repaint)
    );
    let Some(ListOut::Kosten(crate::kosten_view::Schreiben::Gliederung(true))) =
        klick_auf(&mut v, &fonts, &mut p, Hot::Untertitel)
    else {
        panic!("keine Gliederung");
    };
    assert!(!v.mehr_offen);
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "15:00");
    let op = sk_cost::Op::LvGliederungSetzen { untertitel: true };
    s.kosten_folge("LV nach Geschossen gegliedert", None, &h, &[op])
        .unwrap();
    v.sync(&mut s, None);
    assert!(v.untertitel);
    let oz: Vec<&str> = v
        .zeilen()
        .iter()
        .filter(|z| z.art == Art::Position)
        .map(|z| z.oz.as_str())
        .collect();
    assert!(!oz.is_empty());
    assert!(oz.iter().all(|o| o.split('.').count() == 3), "{oz:?}");
    // Zeichnen mit offenem Kopf und offener Karte
    v.kopf_offen = true;
    v.mehr_offen = true;
    v.sync(&mut s, None);
    let mut c = Canvas::new(1400, 900);
    v.paint(&mut c, &Theme::dark(), &fonts, Instant::now());
}

fn csv_zeilen(v: &AvaView) -> Vec<Vec<String>> {
    let b = v.csv();
    let t = String::from_utf8(b).unwrap();
    let t = t.strip_prefix('\u{feff}').expect("BOM");
    t.split("\r\n")
        .map(|z| z.split(';').map(text_zelle).collect())
        .collect()
}

/// Zellinhalt wie in der Tabellenkalkulation: die Zelle `="01.02"` (in der
/// Datei gequotet) ist der Text „01.02“; sonst macht Excel ein Datum daraus.
fn text_zelle(roh: &str) -> String {
    let z = match roh.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(r) => r.replace("\"\"", "\""),
        None => roh.to_string(),
    };
    match z.strip_prefix("=\"").and_then(|r| r.strip_suffix('"')) {
        Some(r) => r.to_string(),
        None => z,
    }
}

fn cent(s: &str) -> i64 {
    let (e, c) = s.split_once(',').expect(s);
    e.parse::<i64>().unwrap() * 100 + c.parse::<i64>().unwrap()
}

/// KA-4d: CSV mit Kopf, Positionen und Zusammenstellung; GP = Menge × EP
/// auf den Cent, Titelsumme = Summe der Zeilen, Netto = Summe der Titel,
/// Einheiten als GAEB-Wörter; Werte wie im LV.
#[test]
fn csv_rechnet_nach() {
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 900);
    v.datei = "haus.szo".into();
    v.sync(&mut s, None);
    assert_eq!(v.knopf_text(), "LV Rohbau als Tabelle speichern");
    let z = csv_zeilen(&v);
    let wert = |k: &str| z.iter().find(|r| r[0] == k).map(|r| r[1].clone());
    assert_eq!(wert("Leistungsverzeichnis").as_deref(), Some("LV Rohbau"));
    assert_eq!(wert("Bauvorhaben").as_deref(), Some("Haus"));
    assert_eq!(wert("LV-Art").as_deref(), Some(v.lv().unwrap().kopf.art));
    assert!(wert("Preisquelle").is_some_and(|p| p.starts_with("Referenzpreise")));
    assert!(wert("Vorbemerkungen").is_some_and(|p| p.contains("VOB/C")));
    let kopf = z.iter().position(|r| r[0] == "OZ").expect("Spaltenkopf");
    assert_eq!(z[kopf], ["OZ", "Kurztext", "Menge", "ME", "EP", "GP"]);
    let lv = v.lv().unwrap();
    let mut titel_summe = 0;
    let mut n = 0;
    let mut netto_titel = 0;
    for r in &z[kopf + 1..] {
        if r.len() < 6 {
            continue;
        }
        if r[0].matches('.').count() == 1 && !r[4].is_empty() {
            // Position: GP = Menge × EP auf den Cent
            let menge: f64 = r[2].replace(',', ".").parse().unwrap();
            let ep = cent(&r[4]);
            let gp = cent(&r[5]);
            assert_eq!(gp, (menge * ep as f64).round() as i64, "{r:?}");
            assert!(
                ["m2", "m3", "m", "t", "St"].contains(&r[3].as_str()),
                "{r:?}"
            );
            let p = lv
                .titel
                .iter()
                .flat_map(|t| &t.positionen)
                .find(|p| p.oz == r[0])
                .unwrap();
            assert_eq!(Some(Cent(gp)), p.gp, "{r:?}");
            titel_summe += gp;
            n += 1;
        } else if r[1].starts_with("Summe ") && r[1] != "Summe netto" && r[1] != "Summe brutto" {
            assert_eq!(cent(&r[5]), titel_summe, "{r:?}");
            netto_titel += titel_summe;
            titel_summe = 0;
        } else if r[1] == "Summe netto" {
            assert_eq!(cent(&r[5]), netto_titel);
            assert_eq!(Some(Cent(netto_titel)), lv.zusammenstellung.netto);
        }
    }
    assert_eq!(n, lv.anzahl());
}

/// „Für Anfrage (leer)“: EP, GP und alle Summen leer.
#[test]
fn csv_anfrage_leer() {
    let mut s = haus();
    let mut v = blatt(&mut s);
    v.preise = false;
    v.sync(&mut s, None);
    let z = csv_zeilen(&v);
    let kopf = z.iter().position(|r| r[0] == "OZ").unwrap();
    assert!(!z.iter().any(|r| r[0] == "Preisquelle"));
    for r in z[kopf + 1..].iter().filter(|r| r.len() >= 6) {
        assert!(r[4].is_empty() && r[5].is_empty(), "{r:?}");
    }
    let pos = z[kopf + 1..]
        .iter()
        .filter(|r| r.len() >= 6 && !r[2].is_empty())
        .count();
    assert_eq!(pos, v.lv().unwrap().anzahl());
}

/// Der Knopf: drücken und loslassen auf ihm gibt „Als Tabelle speichern“.
#[test]
fn knopf_speichert() {
    let Some(fonts) = schrift() else {
        return;
    };
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let (x, y, w, h) = v.knopf_rect(&t, &fonts);
    let xy = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    assert_eq!(
        v.mouse_down(&t, &fonts, &mut p, xy, mods),
        Some(ListOut::Repaint)
    );
    assert_eq!(v.mouse_up(&t, &fonts, xy.0, xy.1), Some(ListOut::SaveCsv));
    // Daneben losgelassen: nichts gespeichert
    v.mouse_down(&t, &fonts, &mut p, xy, mods);
    assert_eq!(v.mouse_up(&t, &fonts, 5.0, 5.0), Some(ListOut::Repaint));
}

/// Prüfen führt zur Stelle (paket-ka4 §4, Bedienbarkeit 12.2): „Ohne
/// Bauleistung“ öffnet im Reiter Kosten „Bauleistung wählen …“ an der
/// grauen Zeile, „Bauherr fehlt“ und „Aufsteller fehlt“ öffnen ihr Feld;
/// Erfülltes steht grün darunter. Der Werksbestand ist kein Aufsteller
/// (Kosten A1).
#[test]
fn pruefen_fuehrt_zur_stelle() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let lv = v.lv().unwrap();
    assert_eq!(lv.kopf.aufsteller, None, "nie „Skizzeo-Werksbestand“");
    assert!(lv.befunde.iter().any(|b| b.satz == "Aufsteller fehlt"));
    v.zeige(Ansicht::Pruefen);
    v.sync(&mut s, None);
    let zeile =
        |v: &AvaView, f: &dyn Fn(&Zeile) -> bool| v.zeilen().iter().position(f).expect("Zeile");
    let i = zeile(&v, &|z| z.text.starts_with("Ohne Bauleistung"));
    let Some(Ziel::Waehlen(el)) = v.zeilen()[i].ziel.clone() else {
        panic!("{:?}", v.zeilen()[i].ziel);
    };
    let (x, y) = mitte(&v, i);
    assert_eq!(
        v.mouse_down(&t, &leer(), &mut p, (x, y), mods),
        Some(ListOut::WaehlenIm(el))
    );
    // Der Reiter Kosten öffnet das Blatt an der grauen Zeile
    let mut k = crate::kosten_view::KostenView::new();
    (k.w, k.h) = (1200, 900);
    k.sync(&mut s, None);
    assert!(!k.blatt_offen());
    k.waehlen_fuer(el);
    k.sync(&mut s, None);
    assert!(k.blatt_offen(), "Bauleistung wählen …");
    // Kopf: zuerst der Bauherr, dann der Aufsteller
    for (satz, feld) in [
        ("Bauherr fehlt", kopf::Feld::Bauherr),
        ("Aufsteller fehlt", kopf::Feld::Aufsteller),
    ] {
        let i = zeile(&v, &|z| z.text == satz);
        assert_eq!(v.zeilen()[i].ziel, Some(Ziel::Kopf(feld)));
        assert_eq!(
            v.zeilen()[i].ziel.as_ref().unwrap().verweis(),
            "Eintragen …"
        );
    }
    let gruen: Vec<&str> = v
        .zeilen()
        .iter()
        .filter(|z| z.art == Art::Erfuellt)
        .map(|z| z.text.as_str())
        .collect();
    assert!(
        gruen.contains(&"Kurztexte höchstens 70 Zeichen"),
        "{gruen:?}"
    );
    assert!(!gruen.contains(&"Kopf vollständig"));
    assert!(!gruen.contains(&"Alle Bauteile haben eine Bauleistung"));
    let i = zeile(&v, &|z| z.text == "Bauherr fehlt");
    let (x, y) = mitte(&v, i);
    assert_eq!(
        v.mouse_down(&t, &leer(), &mut p, (x, y), mods),
        Some(ListOut::Projektdaten(4))
    );
}

/// Jörns Excel-Handtest: OZ wie „01.02“ (Untertitel) oder „01.0010“ würde
/// das deutsche Excel zum Datum bzw. zur Zahl machen. Jede OZ-Zelle steht
/// deshalb als `="…"` in der CSV, auch Titel („01“) und die
/// Zusammenstellung; Mengen und Beträge bleiben Zahlen.
#[test]
fn csv_oz_bleibt_text() {
    let mut s = haus();
    let mut v = blatt(&mut s);
    let h = sk_cost::Herkunft::neu(sk_cost::HerkunftArt::Manual, "2026-10-08", "12:00");
    s.kosten_folge(
        "LV nach Geschossen gegliedert",
        None,
        &h,
        &[sk_cost::Op::LvGliederungSetzen { untertitel: true }],
    )
    .unwrap();
    v.sync(&mut s, None);
    let text = String::from_utf8(v.csv()).unwrap();
    let zeilen: Vec<Vec<&str>> = text.split("\r\n").map(|z| z.split(';').collect()).collect();
    let kopf = zeilen.iter().position(|r| r[0] == "OZ").unwrap();
    let mut gesehen = 0;
    for r in &zeilen[kopf + 1..] {
        let oz = r[0];
        if oz.is_empty() || oz == "Zusammenstellung" {
            continue;
        }
        // "=""01.02.0030""" → in Excel die Formel ="01.02.0030", also Text
        assert!(
            oz.starts_with("\"=\"\"") && oz.ends_with("\"\"\""),
            "OZ nicht als Text: {r:?}"
        );
        let innen = &oz[4..oz.len() - 3];
        assert!(
            innen.chars().all(|c| c.is_ascii_digit() || c == '.'),
            "{innen}"
        );
        gesehen += 1;
        // Mengen und Beträge bleiben Zahlen mit Dezimalkomma
        for zahl in r.iter().skip(2).filter(|x| !x.is_empty() && x.len() < 20) {
            assert!(!zahl.starts_with('"'), "{r:?}");
        }
    }
    assert!(gesehen > 10, "{gesehen}");
    // Untertitel „01.02“ steht als Text da
    assert!(text.contains("\"=\"\"01.02\"\"\";Erdgeschoss"), "{text}");
}

/// Jörn 09.10.: In jeder Fensterbreite von 480 bis 1920 dip (100, 125,
/// 150 %) zeigt jede Positionszeile ihren Kurztext, mindestens 20 Zeichen,
/// sonst ganz, in mindestens 120 dip, und keine Spalte überdeckt eine
/// andere, auch nicht über die zweite Zeile hinweg (außer der Kurztext
/// steht ganz in der zweiten Zeile); im Spaltenkopf ebenso; mit und ohne
/// Preise. Befund M (Test): die erste Zeile endet nie mit „…“, solange die
/// zweite Platz hat, und eine zweite Zeile gibt es nur, wenn der Text
/// nicht in eine passt.
#[test]
fn lv_spalten_ueberdecken_sich_nie() {
    let Some(fonts) = schrift() else {
        return;
    };
    let f = fonts.regular.as_ref().unwrap();
    let t = Theme::dark();
    let mut s = haus();
    for scale in [1.0_f32, 1.25, 1.5] {
        for dip in (480..=1920).step_by(20) {
            let mut v = AvaView::new();
            v.scale = scale;
            (v.w, v.h) = ((dip as f32 * scale) as u32, (900.0 * scale) as u32);
            v.top = 120.0;
            v.sync(&mut s, None);
            let (tx, r) = v.tabelle_x(&t);
            let wo = format!("{dip} dip bei {scale}");
            // Unter 800 dip ist der Baum eine Leiste, die Tabelle hat die
            // ganze Breite; der Kurztext hat immer mindestens 120 dip
            // (Bedienbarkeit 23)
            assert_eq!(dip < 800, tx == v.content_x(&t).0, "{wo}: Baum");
            assert!(
                v.kurz_platz(&t).1 >= 120.0 * scale - 0.5,
                "{wo}: Kurztext nur {} px",
                v.kurz_platz(&t).1
            );
            // Steht der Kurztext ganz in der zweiten Zeile, zählt die Zeile
            let unter = v.spalten(&t).unter;
            let luft = 4.0 * scale;
            let pruefe = |zellen: &[(String, f32, f32, u8)], was: &str| {
                for z in zellen {
                    assert!(
                        z.1 >= tx - 0.5 && z.1 + z.2 <= r + 0.5,
                        "{wo}, {was}: {z:?} außerhalb {tx}..{r}"
                    );
                }
                for a in zellen {
                    for b in zellen {
                        if a.1 < b.1 && a.2 > 0.0 && b.2 > 0.0 && (!unter || a.3 == b.3) {
                            assert!(
                                a.1 + a.2 + luft <= b.1,
                                "{wo}, {was}: {a:?} überdeckt {b:?}"
                            );
                        }
                    }
                }
            };
            let kopf = v
                .kopf_zellen(&t, f, 10.0 * scale)
                .map(|(text, x, w)| (text.to_string(), x, w, 0));
            pruefe(&kopf, "Spaltenkopf");
            let mut n = 0;
            for preise in [true, false] {
                v.preise = preise;
                v.messen(&fonts);
                let positionen = v.zeilen().iter().enumerate();
                let positionen: Vec<usize> = positionen
                    .filter(|(_, z)| z.art == Art::Position)
                    .map(|(i, _)| i)
                    .collect();
                for i in positionen {
                    let z = &v.zeilen()[i];
                    let zellen = v.position_zellen(&t, f, 11.0 * scale, i);
                    assert!(
                        !zellen[1].0.ends_with('…'),
                        "{wo}: Zeile 1 von {} endet mit „…“: {}",
                        z.oz,
                        zellen[1].0
                    );
                    // Zweizeilig genau dann, wenn der Text nicht passt
                    let passt = f.width(&z.text, 11.0 * scale) <= v.kurz_platz(&t).1 - 0.5 * scale;
                    assert_eq!(v.zwei_bei(i), unter || !passt, "{wo}: Höhe von {}", z.oz);
                    let kurz = format!("{} {}", zellen[1].0, zellen[2].0);
                    let kurz = kurz.trim();
                    let gekuerzt = kurz.strip_suffix('…').map(str::trim_end);
                    assert!(
                        kurz == z.text
                            || gekuerzt.is_some_and(|g| {
                                g.chars().count() >= 20 && z.text.starts_with(g)
                            }),
                        "{wo}: Kurztext von {} ist „{kurz}“ statt „{}“",
                        z.oz,
                        z.text
                    );
                    // beide Kurztextzeilen stehen in derselben Spalte
                    let mut ohne_zweite = zellen.to_vec();
                    ohne_zweite.remove(2);
                    pruefe(&ohne_zweite, &z.oz);
                    let mut ohne_erste = zellen.to_vec();
                    ohne_erste.remove(1);
                    pruefe(&ohne_erste, &z.oz);
                    n += 1;
                }
            }
            assert!(n > 0, "{wo}: keine Positionen");
        }
    }
}

/// Umbruch in zwei Zeilen: am Leerzeichen, ein zu langes Wort hart, der
/// Rest mit „…“; was passt, bleibt in einer Zeile.
#[test]
fn kurztext_umbrechen() {
    let Some(fonts) = schrift() else {
        return;
    };
    let f = fonts.regular.as_ref().unwrap();
    let text = "Stahlbetondecke C25/30, d = 20 cm, Ortbeton, einschließlich Bewehrung";
    let w = f.width("Stahlbetondecke C25/30,", 11.0) + 1.0;
    let (a, b) = umbrechen(f, text, 11.0, w);
    assert_eq!(a, "Stahlbetondecke C25/30,");
    assert!(b.starts_with("d = 20 cm") && b.ends_with('…'), "{b}");
    assert!(f.width(&b, 11.0) <= w);
    let (a, b) = umbrechen(f, "Wand", 11.0, w);
    assert_eq!((a.as_str(), b.as_str()), ("Wand", ""));
    let (a, _) = umbrechen(f, "Stahlbetondecke", 11.0, f.width("Stahl", 11.0) + 0.5);
    assert_eq!(a, "Stahl");
}

/// Paket PD-3, Abnahme 4: Kopf und CSV lesen die Projektdaten aus einer
/// Quelle; nach einer Änderung über die Maske stehen sie sofort da,
/// mehrzeilige Felder in der CSV mit „, “.
#[test]
fn projektdaten_in_kopf_und_csv() {
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1400, 900);
    v.datei = "haus.szo".into();
    v.sync(&mut s, None);
    let k = &v.lv().unwrap().kopf;
    assert_eq!(
        (k.projektnummer.as_deref(), k.bauort.as_deref()),
        (None, None)
    );
    let mut p = s.model().project().clone();
    p.kind = "Neubau Einfamilienhaus".into();
    p.place = "Musterweg 1\n27777 Ganderkesee".into();
    p.number = "01/26".into();
    p.client = "Max Mustermann".into();
    p.client_addr = "Phantasiestraße 7\n27777 Ganderkesee".into();
    assert!(s.projekt_setzen("Projektdaten geändert", p.clone()));
    v.sync(&mut s, None);
    let k = &v.lv().unwrap().kopf;
    assert_eq!(k.projektnummer.as_deref(), Some("01/26"));
    assert_eq!(k.projektart.as_deref(), Some("Neubau Einfamilienhaus"));
    assert_eq!(k.bauherr_anschrift.as_deref(), Some(p.client_addr.as_str()));
    assert_eq!(
        k.aufsteller_anschrift, None,
        "ohne Verfasser keine Anschrift"
    );
    let z = csv_zeilen(&v);
    let wert = |k: &str| z.iter().find(|r| r[0] == k).map(|r| r[1].clone());
    assert_eq!(wert("Projekt-Nr.").as_deref(), Some("01/26"));
    assert_eq!(
        wert("Bauort").as_deref(),
        Some("Musterweg 1, 27777 Ganderkesee")
    );
    assert_eq!(wert("Bauherr").as_deref(), Some("Max Mustermann"));
    assert_eq!(
        wert("Anschrift Bauherr").as_deref(),
        Some("Phantasiestraße 7, 27777 Ganderkesee")
    );
    // Abnahme 3/4: 02/26 steht nach dem Schritt sofort im Kopf
    p.number = "02/26".into();
    assert!(s.projekt_setzen("Projektdaten geändert", p));
    v.sync(&mut s, None);
    assert_eq!(v.lv().unwrap().kopf.projektnummer.as_deref(), Some("02/26"));
    assert!(s.undo());
    v.sync(&mut s, None);
    assert_eq!(v.lv().unwrap().kopf.projektnummer.as_deref(), Some("01/26"));
}

/// Bedienbarkeit 23: Unter 800 dip ist der Baum eine Leiste „LV …“; ein
/// Klick klappt ihn als Blatt auf, eine Wahl darin zeigt die Ansicht und
/// schließt das Blatt, ein Klick daneben oder Esc schließt es ohne Folgen.
#[test]
fn baum_als_leiste_im_schmalen_fenster() {
    let Some(fonts) = schrift() else {
        return;
    };
    let t = Theme::dark();
    let mut s = haus();
    let mut p = Picking::default();
    let mut v = AvaView::new();
    (v.w, v.h) = (1000, 900);
    v.sync(&mut s, None);
    assert!(!v.baum_als_leiste() && !v.baum_lage(&t).is_empty());
    v.w = 640;
    v.sync(&mut s, None);
    assert!(v.baum_als_leiste());
    assert!(v.baum_lage(&t).is_empty(), "zu: kein Baum");
    assert!(v.leiste_rect(&t, fonts.bold.as_ref()).1.starts_with("LV "));
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumLeiste);
    assert!(v.baum_blatt && !v.baum_lage(&t).is_empty());
    // Klick in die Tabelle schließt nur das Blatt
    let (tx, r) = v.tabelle_x(&t);
    let (_, unten) = v.liste_y();
    let mods = sk_platform::Modifiers::default();
    let xy = (((tx + r) * 0.5) as f64, (unten - 4.0) as f64);
    v.mouse_down(&t, &fonts, &mut p, xy, mods);
    assert!(!v.baum_blatt && v.ansicht == Ansicht::Lv);
    // Wahl im Blatt: Zusammenstellung
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumLeiste);
    let xy = v
        .knoten_mitte(&t, |k| *k == Knoten::Zusammenstellung)
        .expect("im Blatt");
    v.mouse_down(&t, &fonts, &mut p, xy, mods);
    assert!(!v.baum_blatt && v.ansicht == Ansicht::Zusammenstellung);
    assert_eq!(v.leiste_rect(&t, fonts.bold.as_ref()).1, "Zusammenstellung");
    // Esc schließt das offene Blatt
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumLeiste);
    assert!(v.baum_blatt_schliessen() && !v.baum_blatt);
    assert!(!v.baum_blatt_schliessen());
    // Wahl eines Titels: die Leiste nennt ihn
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumLeiste);
    let xy = v
        .knoten_mitte(&t, |k| matches!(k, Knoten::Titel { .. }))
        .expect("Titel im Blatt");
    v.mouse_down(&t, &fonts, &mut p, xy, mods);
    v.sync(&mut s, None);
    let text = v.leiste_rect(&t, fonts.bold.as_ref()).1;
    assert!(text.starts_with("LV ") && text.contains(" › "), "{text}");
    // Bedienbarkeit 25.1: mit gewählter Position nennt sie deren Titel
    // wie der Baum, nicht den zuletzt angeklickten
    let lv = v.lv().unwrap().clone();
    for t0 in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
        v.gewaehlt = Some(t0.positionen[0].oz.clone());
        let text = v.leiste_rect(&t, fonts.bold.as_ref()).1;
        assert!(
            text.ends_with(&format!("› {} {}", t0.nr, t0.name)),
            "{text}"
        );
    }
    v.gewaehlt = None;
    // Pille: Tooltip, und bei offenem Blatt genügt ein Klick
    let (zx, zy, zw, zh) = v.pruef_rect(&t, fonts.bold.as_ref()).expect("Zähler");
    let mitte = ((zx + zw * 0.5) as f64, (zy + zh * 0.5) as f64);
    let tip = v.tip_at(&t, &fonts, mitte.0, mitte.1).unwrap_or_default();
    assert!(tip.ends_with("offene Punkte im LV"), "{tip}");
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumLeiste);
    assert!(v.baum_blatt);
    v.mouse_down(&t, &fonts, &mut p, mitte, mods);
    assert!(v.ansicht == Ansicht::Pruefen && !v.baum_blatt);
    v.zeige(Ansicht::Lv);
    // Der Prüfzähler steht neben der zugeklappten Leiste und zeigt „Prüfen“
    assert!(!v.baum_blatt && v.befunde() > 0);
    let (lx, _, lw, _) = v.leiste_rect(&t, fonts.bold.as_ref()).0;
    let (zx, _, zw, _) = v.pruef_rect(&t, fonts.bold.as_ref()).expect("Zähler");
    let (x0, cw) = v.content_x(&t);
    assert!(zx >= lx + lw && zx + zw <= x0 + cw);
    klick_auf(&mut v, &fonts, &mut p, Hot::BaumPruefen);
    assert!(v.ansicht == Ansicht::Pruefen && !v.baum_blatt);
    assert_eq!(v.leiste_rect(&t, fonts.bold.as_ref()).1, "Prüfen");
}

/// Wechsel zwischen breitem und schmalem Fenster behält Lage und Wahl.
#[test]
fn breit_und_schmal_behalten_die_lage() {
    let t = Theme::dark();
    let mut s = haus();
    let mut v = AvaView::new();
    (v.w, v.h) = (1000, 520);
    v.sync(&mut s, None);
    // Nur neun Zeilen: mit dem Detail darunter lässt sich trotzdem rollen
    let oz = v
        .zeilen
        .iter()
        .find(|z| z.art == Art::Position)
        .map(|z| z.oz.clone());
    v.gewaehlt = oz.clone();
    v.sync(&mut s, None);
    assert!(v.detail.is_some());
    v.scroll = 40.0;
    v.clamp();
    let vorher = v.scroll;
    assert_eq!(vorher, 40.0, "{:?}, {}", v.liste_y(), v.inhalt_h());
    for w in [640, 1000] {
        v.w = w;
        v.sync(&mut s, None);
        v.clamp();
        assert_eq!(v.baum_als_leiste(), w < 800);
        assert_eq!(v.scroll, vorher, "Lage bei {w}");
        assert_eq!(v.gewaehlt, oz, "Wahl bei {w}");
        assert!(!v.baum_lage(&t).is_empty() || v.baum_als_leiste());
    }
}

/// Druckvorschau (kosten/lv-blatt-a4.md §9): im Baum unter „Prüfen“, Seite
/// für Seite mit „Seite x von y“, Häkchen Titelblatt und Verzeichnis, der
/// Knopf oben speichert genau diese Seiten als PDF.
#[test]
fn druckvorschau_blaettern_und_pdf() {
    let Some(fonts) = schrift() else {
        return;
    };
    let t = Theme::dark();
    let mut s = haus();
    let mut v = blatt(&mut s);
    let mut p = Picking::default();
    let mods = sk_platform::Modifiers::default();
    let xy = v
        .knoten_mitte(&t, |k| *k == Knoten::Blatt)
        .expect("Druckvorschau im Baum");
    v.mouse_down(&t, &fonts, &mut p, xy, mods);
    assert_eq!(v.ansicht, Ansicht::Blatt);
    assert!(
        v.knopf_text().ends_with(" als PDF speichern"),
        "{}",
        v.knopf_text()
    );
    let n = v.blatt(&fonts).expect("Blatt").seiten.len();
    assert!(n >= 2, "{n} Seiten");
    // Blättern mit „›“ und „‹“, nie über das Ende
    assert_eq!(v.seite_jetzt(&fonts), 0);
    assert_eq!(klick_auf(&mut v, &fonts, &mut p, Hot::Seite(false)), None);
    for _ in 0..n + 2 {
        klick_auf(&mut v, &fonts, &mut p, Hot::Seite(true));
    }
    assert_eq!(v.seite_jetzt(&fonts), n - 1);
    assert_eq!(v.wheel(1.0, &t), Some(ListOut::Repaint));
    assert_eq!(v.seite_jetzt(&fonts), n - 2);
    // Titelblatt und Verzeichnis: zwei Seiten mehr, die Wahl geht in die
    // Einstellungen
    let out = klick_auf(&mut v, &fonts, &mut p, Hot::BlattWahl(false));
    assert_eq!(out, Some(ListOut::BlattWahl((true, false))));
    let out = klick_auf(&mut v, &fonts, &mut p, Hot::BlattWahl(true));
    assert_eq!(out, Some(ListOut::BlattWahl((true, true))));
    assert_eq!(v.blatt(&fonts).unwrap().seiten.len(), n + 2);
    // Der Knopf speichert das PDF
    let (x, y, w, h) = v.knopf_rect(&t, &fonts);
    let k = ((x + w * 0.5) as f64, (y + h * 0.5) as f64);
    v.mouse_down(&t, &fonts, &mut p, k, mods);
    assert_eq!(v.mouse_up(&t, &fonts, k.0, k.1), Some(ListOut::SavePdf));
    let (name, bytes) = v.pdf(&fonts).expect("PDF");
    assert!(
        name.starts_with("LV-Rohbau-") && name.ends_with(".pdf"),
        "{name}"
    );
    let datum = name.trim_end_matches(".pdf");
    assert!(datum.contains("-20"), "Datum im Namen: {name}");
    assert!(bytes.starts_with(b"%PDF-"));
    let seiten = String::from_utf8_lossy(&bytes)
        .matches("/Type /Page ")
        .count();
    assert_eq!(seiten, n + 2, "PDF wie die Vorschau");
    // Ein anderes Los fängt wieder auf Seite 1 an
    v.seite = 2;
    let los = v.baum.iter().find_map(|k| match k {
        Knoten::Los { guid, .. } if Some(*guid) != v.los => Some(*guid),
        _ => None,
    });
    if let Some(g) = los {
        v.waehle_los(g);
        assert_eq!((v.seite, v.ansicht), (0, Ansicht::Lv));
    }
}

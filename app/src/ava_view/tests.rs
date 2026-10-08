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
    assert!(matches!(v.baum().last(), Some(Knoten::Pruefen(_))));
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

//! Abnahme LV-Blatt (kosten/lv-blatt-a4.md §11, paket-projektdaten §6.6–7)
//! am Standardhaus RH-1 mit dem Werksbestand.

use super::*;
use crate::scene::Scene;
use sk_cost::lv::LvWahl;
use sk_model::qto::Umfang;

fn schriften() -> Option<(Font, Font)> {
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let lade = |n: &str| std::fs::read(lib.join(n)).ok().and_then(Font::parse);
    Some((
        lade("LiberationSans-Regular.ttf")?,
        lade("LiberationSans-Bold.ttf")?,
    ))
}

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

fn lv(s: &mut Scene, preise: bool, untertitel: bool) -> Lv {
    let k = s.katalog(None);
    let los = k.lose.iter().find(|l| l.parent.is_none()).unwrap().guid;
    let w = LvWahl {
        los,
        preise,
        untertitel,
        heute: None,
    };
    (*s.lv(None, &Umfang::projekt(), &w)).clone()
}

/// Titel 03 Betonarbeiten (vor ihm stehen die Automatiktitel 01 und 02).
fn beton(lv: &mut Lv) -> &mut sk_cost::lv::LvTitel {
    lv.titel.iter_mut().find(|t| t.nr == "03").unwrap()
}

fn angaben(preise: bool) -> Angaben {
    Angaben {
        bauvorhaben: "Haus".into(),
        umfang: "Gebäude 1 · alle Geschosse".into(),
        stand: "09.10.2026".into(),
        preisquelle: preise.then(|| "Referenzpreise 10/2026, unverbindlich".into()),
    }
}

/// Alle Texte einer Seite.
fn texte(s: &Seite) -> Vec<&str> {
    s.ops
        .iter()
        .filter_map(|o| match o {
            Op::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn alle(b: &Blatt) -> Vec<String> {
    b.seiten
        .iter()
        .flat_map(|s| texte(s).into_iter().map(String::from))
        .collect()
}

/// Kein Text überdeckt einen anderen auf derselben Grundlinie.
fn ueberdeckt_nichts(b: &Blatt, f: Schriften) {
    for (i, s) in b.seiten.iter().enumerate() {
        let mut r: Vec<(f32, f32, f32, &str)> = Vec::new();
        for o in &s.ops {
            if let Op::Text {
                x,
                y,
                pt,
                fett,
                text,
                ..
            } = o
            {
                r.push((*y, *x, x + f.f(*fett).width(text, *pt), text));
            }
        }
        for a in &r {
            for c in &r {
                if (a.0 - c.0).abs() < 1.0 && a.1 < c.1 {
                    assert!(
                        a.2 + 1.0 <= c.1,
                        "Seite {}: „{}“ über „{}“",
                        i + 1,
                        a.3,
                        c.3
                    );
                }
            }
            assert!(
                a.2 <= mm(LINKS + BREITE) + 0.5,
                "Seite {}: „{}“ rechts",
                i + 1,
                a.3
            );
        }
    }
}

fn seite_x_von_y(b: &Blatt, ab: usize) {
    let n = b.seiten.len();
    for (i, s) in b.seiten.iter().enumerate().skip(ab) {
        let soll = format!("Seite {} von {n}", i + 1);
        assert!(texte(s).contains(&soll.as_str()), "Seite {}: {soll}", i + 1);
    }
}

/// §11.1 Mit Preisen: voller Kurztext, Mengen, EP, GP, Titel, netto, MwSt.,
/// brutto, Preisquelle.
#[test]
fn mit_preisen() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let lv = lv(&mut s, true, false);
    let b = blatt(
        &lv,
        &angaben(true),
        Wahl {
            preise: true,
            ..Wahl::default()
        },
        f,
    );
    let t = alle(&b);
    let p = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.oz == "04.0010")
        .unwrap();
    let kurz = t.join(" ");
    assert!(kurz.contains(&p.kurztext), "voller Kurztext von 04.0010");
    for z in [
        "172,224",
        "54,00",
        "9.300,10",
        "27.636,71",
        "38.800,86",
        "7.372,16",
        "46.173,02",
        "Referenzpreise 10/2026, unverbindlich",
        "mit Preisen",
    ] {
        assert!(t.iter().any(|x| x == z), "{z} fehlt");
    }
    assert!(!t.iter().any(|x| x == "Angaben des Bieters"));
    ueberdeckt_nichts(&b, f);
    seite_x_von_y(&b, 0);
    // Hinweis S: Firmenkatalog ohne doppeltes „Preise“
    let mut a = angaben(true);
    a.preisquelle = Some("Preise Firmenkatalog vom 08.10.2026".into());
    let mit = Wahl {
        preise: true,
        ..Wahl::default()
    };
    let t = alle(&blatt(&lv, &a, mit, f));
    assert!(t.iter().any(|x| x == "Firmenkatalog vom 08.10.2026"));
    assert!(!t.iter().any(|x| x.contains("Preise Firmenkatalog")));
}

/// §11.2 Für Anfrage: keine Zahl in EP, GP und Summen, Mengen wie mit
/// Preisen, keine Preisquelle, Bieterblock am Ende.
#[test]
fn fuer_anfrage() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let lv = lv(&mut s, false, false);
    let b = blatt(&lv, &angaben(false), Wahl::default(), f);
    let t = alle(&b);
    let betrag = |x: &str| {
        let (v, n) = match x.rsplit_once(',') {
            Some(p) => p,
            None => return false,
        };
        n.len() == 2
            && n.chars().all(|c| c.is_ascii_digit())
            && !v.is_empty()
            && v.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    assert!(!t.iter().any(|x| betrag(x)), "Betrag in der Anfrage");
    assert!(t.iter().any(|x| x == "172,224"));
    assert!(!t.iter().any(|x| x.contains("Referenzpreise")));
    assert!(t.iter().any(|x| x == "Anfrage ohne Preise"));
    let letzte = texte(b.seiten.last().unwrap());
    assert!(letzte.contains(&"Unterschrift und Stempel des Bieters"));
    ueberdeckt_nichts(&b, f);
    seite_x_von_y(&b, 0);
}

/// §11.3 Mit Untertiteln: 03.02.0030 mit 16,508, Summenzeile je
/// Untertitel.
#[test]
fn mit_untertiteln() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let lv = lv(&mut s, true, true);
    let b = blatt(
        &lv,
        &angaben(true),
        Wahl {
            preise: true,
            ..Wahl::default()
        },
        f,
    );
    let t = alle(&b);
    assert!(t.iter().any(|x| x == "03.02.0030"));
    assert!(t.iter().any(|x| x == "16,508"));
    let n_ut: usize = lv.titel.iter().map(|t| t.untertitel.len()).sum();
    assert!(n_ut > 0);
    let summen = t
        .iter()
        .filter(|x| x.len() > 9 && x.starts_with("Summe 0") && x.as_bytes()[8] == b'.')
        .count();
    assert_eq!(summen, n_ut, "eine Summe je Untertitel");
    ueberdeckt_nichts(&b, f);
}

/// §11.4: Ein Titel über mehrere Seiten; Übertrag unten und oben gleich,
/// am Ende gleich der Titelsumme; „Seite x von y“ stimmt.
#[test]
fn uebertrag_ueber_seiten() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let mut lv = lv(&mut s, true, false);
    // Titel 03 (Beton) mit 15facher Positionszahl
    let t = beton(&mut lv);
    let einmal = t.positionen.clone();
    for k in 1..15 {
        for p in &einmal {
            let mut q = p.clone();
            q.oz = format!("{}-{k}", p.oz);
            t.positionen.push(q);
        }
    }
    let summe: i64 = t.positionen.iter().filter_map(|p| p.gp).map(|c| c.0).sum();
    t.summe = Some(Cent(summe));
    let b = blatt(
        &lv,
        &angaben(true),
        Wahl {
            preise: true,
            ..Wahl::default()
        },
        f,
    );
    assert!(b.seiten.len() >= 3, "{} Seiten", b.seiten.len());
    seite_x_von_y(&b, 0);
    ueberdeckt_nichts(&b, f);
    assert!(uebertraege(&b, "") >= 2);
    assert!(alle(&b).contains(&Cent(summe).deutsch()));
    // Kosten A6: mit Titelblatt und Verzeichnis zählt „Übertrag von Seite
    // n“ wie die Fußzeile
    let w = Wahl {
        preise: true,
        titelblatt: true,
        verzeichnis: true,
    };
    let mit = blatt(&lv, &angaben(true), w, f);
    assert_eq!(mit.seiten.len(), b.seiten.len() + 2);
    assert!(uebertraege(&mit, "") >= 2);
    // Kosten B11: fehlt ein Preis, trägt der Übertrag die bekannten GP und
    // „(unvollständig)“ wie die Titelsumme
    beton(&mut lv).positionen[1].gp = None;
    beton(&mut lv).unvollstaendig = true;
    let b = blatt(
        &lv,
        &angaben(true),
        Wahl {
            preise: true,
            ..Wahl::default()
        },
        f,
    );
    assert!(uebertraege(&b, " (unvollständig)") >= 2);
}

/// Prüft jeden Übertrag: unten „Übertrag 01 …{zusatz}“, oben auf der
/// nächsten Seite „Übertrag von Seite n{zusatz}“ mit der Nummer aus der
/// Fußzeile der Seite davor und demselben Betrag. Gibt die Anzahl.
fn uebertraege(b: &Blatt, zusatz: &str) -> usize {
    let mut n = 0;
    for i in 0..b.seiten.len() - 1 {
        let tx = texte(&b.seiten[i]);
        let Some(k) = tx.iter().position(|x| x.starts_with("Übertrag 03")) else {
            continue;
        };
        assert!(tx[k].ends_with(zusatz), "{}", tx[k]);
        let unten = tx[k + 1];
        assert!(tx
            .iter()
            .any(|x| x.starts_with(&format!("Seite {} von", i + 1))));
        let nx = texte(&b.seiten[i + 1]);
        let von = format!("Übertrag von Seite {}{zusatz}", i + 1);
        let j = nx
            .iter()
            .position(|x| *x == von)
            .unwrap_or_else(|| panic!("{von} fehlt auf Seite {}", i + 2));
        assert_eq!(nx[j + 1], unten, "Seite {}", i + 1);
        n += 1;
    }
    n
}

/// Kosten B12 und B13: ohne Vorbemerkungen kein Eintrag im Verzeichnis;
/// läuft die Zusammenstellung über eine Seite, steht dort die Überschrift
/// mit „(Fortsetzung)“.
#[test]
fn ohne_vorbemerkungen_und_lange_zusammenstellung() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let mut lv = lv(&mut s, true, false);
    lv.kopf.vorbemerkungen = None;
    let z = lv.zusammenstellung.zeilen[0].clone();
    for k in 0..90 {
        let mut zeile = z.clone();
        zeile.0 = format!("{}", 10 + k);
        lv.zusammenstellung.zeilen.push(zeile);
    }
    let w = Wahl {
        preise: true,
        titelblatt: false,
        verzeichnis: true,
    };
    let b = blatt(&lv, &angaben(true), w, f);
    assert!(!b.inhalt.iter().any(|e| e.text == "Vorbemerkungen"));
    assert!(!alle(&b).iter().any(|t| t == "Vorbemerkungen"));
    let fort = alle(&b)
        .iter()
        .filter(|t| t.starts_with("Zusammenstellung · ") && t.ends_with("(Fortsetzung)"))
        .count();
    assert!(fort >= 1, "keine Fortsetzung");
    ueberdeckt_nichts(&b, f);
    seite_x_von_y(&b, 1);
}

/// §11.5: Ein Kurztext mit genau 70 Zeichen bricht um und wird nie gekürzt.
#[test]
fn kurztext_70_zeichen() {
    let Some((r, _)) = schriften() else {
        return;
    };
    let text = "Mauerwerk aus Porenbeton-Plansteinen PP2-0,35 d=17,5cm in Dünnbettmörtel";
    let text = &text[..text.char_indices().nth(70).map_or(text.len(), |(i, _)| i)];
    assert_eq!(text.chars().count(), 70);
    let z = umbrechen(&r, text, TEXT, mm(KURZ_W) - 4.0);
    assert_eq!(z.len(), 2, "{z:?}");
    assert_eq!(z.join(" "), text);
    assert!(!z.iter().any(|l| l.contains('…')));
}

/// Paket PD §6.6–7: Kopf mit Projektdaten, Kurzkopf und Fußzeile mit
/// Projekt-Nr. und Planung; Titelblatt und Verzeichnis an: Seite 1
/// Titelblatt, Seite 2 Verzeichnis, jede Zahl stimmt.
#[test]
fn titelblatt_und_verzeichnis() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let mut p = s.model().project().clone();
    p.kind = "Neubau Einfamilienhaus".into();
    p.site = "Haus Mustermann".into();
    p.place = "Musterweg 1\n27777 Ganderkesee".into();
    p.number = "01/26".into();
    p.client = "Max Mustermann".into();
    p.client_addr = "Phantasiestraße 7\n27777 Ganderkesee".into();
    p.author = "Dipl.-Ing. (FH) Jörn Horstmann".into();
    p.author_addr = "Denkmalsweg 18b\n27777 Ganderkesee".into();
    assert!(s.projekt_setzen("Projektdaten geändert", p));
    let mut lv = lv(&mut s, true, true);
    // Lang genug für Folgeseiten
    let t = beton(&mut lv);
    let einmal = t.positionen.clone();
    for k in 1..8 {
        for p in &einmal {
            let mut q = p.clone();
            q.oz = format!("{}-{k}", p.oz);
            t.positionen.push(q);
        }
    }
    let mut a = angaben(true);
    a.bauvorhaben = "Haus Mustermann".into();
    let ohne = blatt(
        &lv,
        &a,
        Wahl {
            preise: true,
            ..Wahl::default()
        },
        f,
    );
    let w = Wahl {
        preise: true,
        titelblatt: true,
        verzeichnis: true,
    };
    let b = blatt(&lv, &a, w, f);
    assert_eq!(b.seiten.len(), ohne.seiten.len() + 2);
    // Seite 1 Titelblatt ohne Fußzeile, Seite 2 Verzeichnis
    let t1 = texte(&b.seiten[0]);
    assert!(t1.contains(&"Leistungsverzeichnis") && t1.contains(&"01/26"));
    assert!(!t1.iter().any(|x| x.starts_with("Seite ")));
    assert!(texte(&b.seiten[1]).contains(&"Inhalt"));
    seite_x_von_y(&b, 1);
    // Jede Seitenzahl im Verzeichnis zeigt auf die Seite mit dem Eintrag
    assert!(b.inhalt.len() >= 4);
    for e in &b.inhalt {
        let tx = texte(&b.seiten[e.seite - 1]);
        let erstes = e.text.split_once(' ').map_or(e.text.as_str(), |(a, _)| a);
        let rest = e.text.split_once(' ').map_or("", |(_, b)| b);
        assert!(
            tx.iter().any(|x| x.contains(e.text.as_str()))
                || (tx.contains(&erstes) && tx.contains(&rest)),
            "„{}“ nicht auf Seite {}",
            e.text,
            e.seite
        );
        let verz = texte(&b.seiten[1]);
        assert!(verz.contains(&e.seite.to_string().as_str()));
    }
    // Kopf Seite 1 mit Projektdaten, Bauort nie gekürzt
    let kopf = texte(&b.seiten[2]).join(" ");
    for z in [
        "Haus Mustermann",
        "01/26",
        "Max Mustermann",
        "Phantasiestraße 7",
        "Denkmalsweg 18b",
    ] {
        assert!(kopf.contains(z), "{z}");
    }
    assert!(
        kopf.contains("Neubau Einfamilienhaus · Musterweg 1, 27777 Ganderkesee")
            || kopf.contains("Ganderkesee")
    );
    // Kurzkopf und Fußzeile auf Folgeseiten
    let folge = texte(&b.seiten[3]);
    assert!(folge.iter().any(
        |x| x.starts_with("Haus Mustermann · Projekt-Nr. 01/26 · LV") && x.ends_with("mit Preisen")
    ));
    assert!(folge.contains(&"Haus Mustermann · Projekt-Nr. 01/26"));
    assert!(folge.contains(&"Planung: Dipl.-Ing. (FH) Jörn Horstmann"));
    ueberdeckt_nichts(&b, f);
    // Ohne Projektnummer steht „Projekt-Nr.“ nie allein
    lv.kopf.projektnummer = None;
    let b = blatt(&lv, &a, w, f);
    assert!(!alle(&b).iter().any(|x| x.contains("Projekt-Nr.")));
}

/// Das PDF: Kopf, Seiten, eingebettete Teilmenge der Schrift; Dateiname.
#[test]
fn pdf_und_dateiname() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let lv = lv(&mut s, false, false);
    let b = blatt(&lv, &angaben(false), Wahl::default(), f);
    let pdf = sk_paint::pdf::schreiben(&b.seiten, A4, &r, &fe, "LV Rohbau");
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert!(pdf.ends_with(b"%%EOF\n"));
    // Hinweis P: die Schriften nur als Teilmenge, das LV bleibt klein
    assert!(pdf.len() < 120_000, "{} Bytes", pdf.len());
    if let Some(d) = std::env::var_os("SKIZZEO_LV_PDF") {
        std::fs::write(d, &pdf).unwrap();
    }
    assert_eq!(
        dateiname(&lv, "Haus", "2026-10-09", false),
        format!("LV-{}-Haus-2026-10-09-Anfrage.pdf", lv.kopf.los)
    );
    assert_eq!(
        dateiname(&lv, "Haus Mustermann", "2026-10-09", true),
        format!(
            "LV-{}-Haus-Mustermann-2026-10-09-mit-Preisen.pdf",
            lv.kopf.los
        )
    );
}

/// Test Hinweis N: Der Projekt-Eintrag bricht nach dem Komma um, nie
/// zwischen Postleitzahl und Ort; zu lange Glieder wie sonst am Wort.
/// Hinweis O: Bildschirm und Blatt nennen das Los gleich.
#[test]
fn projekt_bricht_nach_dem_komma() {
    let Some((r, _)) = schriften() else {
        return;
    };
    let text = "Neubau Einfamilienhaus · Musterweg 1, 27777 Ganderkesee";
    let z = umbrechen_glieder(&r, text, TEXT, mm(72.0));
    assert_eq!(
        z,
        ["Neubau Einfamilienhaus · Musterweg 1,", "27777 Ganderkesee"]
    );
    assert_eq!(umbrechen_glieder(&r, "Haus", TEXT, mm(72.0)), ["Haus"]);
    let lang = "Sehr langer Bauort mit vielen Wörtern ohne jedes Komma dazwischen bis ans Ende";
    let z = umbrechen_glieder(&r, lang, TEXT, mm(40.0));
    assert!(z.len() >= 2 && z.join(" ") == lang, "{z:?}");
    assert!(z.iter().all(|l| r.width(l, TEXT) <= mm(40.0)));
    let mut s = haus();
    let lv = lv(&mut s, true, false);
    assert_eq!(los_text(&lv), "LV 1 Rohbau");
}

/// Review 3bo: Fehlt einem Artikel der Preis, hat die Position zwar EP und
/// GP, die Titelsumme heißt aber „(unvollständig)“; der Übertrag ebenso,
/// sonst stünde eine unvollständige Summe als vollständig da.
#[test]
fn uebertrag_unvollstaendig_bei_fehlendem_artikelpreis() {
    let Some((r, fe)) = schriften() else {
        return;
    };
    let f = Schriften {
        regular: &r,
        fett: &fe,
    };
    let mut s = haus();
    let mut lv = lv(&mut s, true, false);
    let t = beton(&mut lv);
    let einmal = t.positionen.clone();
    for k in 1..15 {
        for p in &einmal {
            let mut q = p.clone();
            q.oz = format!("{}-{k}", p.oz);
            t.positionen.push(q);
        }
    }
    t.positionen[1].preis_fehlt = true;
    t.unvollstaendig = true;
    let w = Wahl {
        preise: true,
        ..Wahl::default()
    };
    let b = blatt(&lv, &angaben(true), w, f);
    let a = alle(&b);
    let ue: Vec<&String> = a.iter().filter(|x| x.starts_with("Übertrag")).collect();
    assert!(ue.len() >= 2, "{ue:?}");
    for x in ue {
        assert!(x.ends_with("(unvollständig)"), "{x}");
    }
    assert!(a
        .iter()
        .any(|x| x == "Summe 03 Betonarbeiten (unvollständig)"));
}

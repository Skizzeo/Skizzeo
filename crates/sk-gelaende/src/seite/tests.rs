//! Prüfungen des Inhaltsstroms an kleinen, von Hand gebauten Seiten und am
//! Beispiel-Lageplan.

use super::*;

/// PDF aus Objekten 1 bis n bauen (Objekt 1 ist der Katalog).
fn pdf_aus(objekte: &[Vec<u8>]) -> Vec<u8> {
    let mut roh = b"%PDF-1.4\n".to_vec();
    let mut lagen = Vec::new();
    for (i, o) in objekte.iter().enumerate() {
        lagen.push(roh.len());
        roh.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        roh.extend_from_slice(o);
        roh.extend_from_slice(b"\nendobj\n");
    }
    let x = roh.len();
    roh.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objekte.len() + 1).as_bytes(),
    );
    for l in lagen {
        roh.extend_from_slice(format!("{l:010} 00000 n \n").as_bytes());
    }
    roh.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n",
            objekte.len() + 1
        )
        .as_bytes(),
    );
    roh
}

fn strom(kopf: &str, d: &[u8]) -> Vec<u8> {
    let mut v = format!("<< {kopf} /Length {} >>\nstream\n", d.len()).into_bytes();
    v.extend_from_slice(d);
    v.extend_from_slice(b"\nendstream");
    v
}

/// Seite mit Inhalt, Schrift F1 (WinAnsi, Breiten 500) und weiteren
/// Objekten ab Nummer 6.
fn seite_aus(inhalt: &[u8], res_mehr: &str, mehr: &[Vec<u8>]) -> Seite {
    let mut o = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> {res_mehr} >> >>").into_bytes(),
        strom("", inhalt),
        b"<< /Type /Font /Subtype /TrueType /BaseFont /Arial /Encoding /WinAnsiEncoding /FirstChar 32 /Widths [278 278 355 556 556 889 667 191 333 333 389 584 278 333 278 278 556 556 556 556 556 556 556 556 556 556] >>".to_vec(),
    ];
    o.extend_from_slice(mehr);
    let roh = pdf_aus(&o);
    let pdf = Pdf::lesen(&roh).unwrap();
    let b = pdf.seiten().remove(0);
    seite(&pdf, &b)
}

fn nah(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
}

#[test]
fn zustand_pfade_und_farben() {
    let s = seite_aus(
        b"q 2 0 0 2 10 20 cm 0.5 w 1 0 0 RG 0 0 m 10 0 l 10 5 l h S Q
          0 0 1 rg 1 2 3 4 re f
          q 0.5 g 0 0 m 1 1 l n Q
          0 1 0 0 k 0 0 m 4 0 4 4 0 4 c B*
          5 5 m 6 6 l 7 5 l s",
        "",
        &[],
    );
    assert_eq!(s.mediabox, [0.0, 0.0, 200.0, 100.0]);
    assert_eq!(s.pfade.len(), 4, "n malt nichts");
    let p = &s.pfade[0];
    assert_eq!(p.strich, Some(([255, 0, 0], 1.0)), "Breite nach der Matrix");
    assert_eq!(p.fuellung, None);
    assert!(p.zuege[0].zu && nah(p.zuege[0].punkte[2], [30.0, 30.0]));
    let r = &s.pfade[1];
    assert_eq!((r.strich, r.fuellung), (None, Some([0, 0, 255])));
    assert_eq!(r.zuege[0].punkte.len(), 4);
    assert!(nah(r.zuege[0].punkte[2], [4.0, 6.0]));
    let k = &s.pfade[2];
    assert!(k.kurven && k.gerade_ungerade);
    assert_eq!(k.fuellung, Some([255, 0, 255]));
    assert_eq!(
        k.strich,
        Some(([0, 0, 0], 1.0)),
        "Q stellt Farbe und Breite zurück"
    );
    let pk = &k.zuege[0].punkte;
    assert!(pk.len() > 5 && nah(*pk.last().unwrap(), [0.0, 4.0]));
    // flach gemacht: Kurvenpunkte liegen auf der Bézierkurve
    let mitte = pk[pk.len() / 2];
    assert!(mitte[0] > 2.0 && mitte[1] > 2.0);
    assert!(s.pfade[3].zuege[0].zu, "s schließt");
}

#[test]
fn texte_tj_tj_td_und_anschluss() {
    let s = seite_aus(
        b"BT /F1 10 Tf 20 30 Td (Ma\\337stab 1:500) Tj ET
          BT /F1 1 Tf 0 14 -14 0 100 50 Tm (1)Tj 0.556 0 Td (0)Tj 0.556 0 Td (7)Tj ET
          BT /F1 12 Tf 1 0 0 1 10 80 Tm [(E)-600(=)] TJ 0 -20 Td (zweite) Tj T* (x) ' ET",
        "",
        &[],
    );
    let t: Vec<&str> = s.texte.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(t, ["Maßstab 1:500", "107", "E =", "zweite", "x"]);
    let a = &s.texte[0];
    assert!(nah(a.lage, [20.0, 30.0]) && (a.groesse - 10.0).abs() < 1e-9 && a.winkel.abs() < 1e-9);
    let n = &s.texte[1];
    assert!(nah(n.lage, [100.0, 50.0]));
    assert!((n.groesse - 14.0).abs() < 1e-9 && (n.winkel - 90.0).abs() < 1e-9);
    // drei Ziffern à 0,556 × 14 pt nach oben
    assert!(nah(n.ende, [100.0, 50.0 + 3.0 * 0.556 * 14.0]));
    assert!(nah(s.texte[3].lage, [10.0, 60.0]));
}

#[test]
fn type0_mit_unicode_und_formular() {
    let tu = b"begincmap 1 begincodespacerange <0000> <FFFF> endcodespacerange
               1 beginbfrange <0010> <0019> <0030> endbfrange 1 beginbfchar <0001> <002F> endbfchar endcmap";
    let s = seite_aus(
        b"q 1 0 0 1 50 0 cm /Fm1 Do Q /Bild Do",
        "/XObject << /Fm1 8 0 R /Bild 9 0 R >>",
        &[
            b"<< /Type /Font /Subtype /Type0 /BaseFont /Sym /Encoding /Identity-H /DescendantFonts [7 0 R] /ToUnicode 10 0 R >>".to_vec(),
            b"<< /Type /Font /Subtype /CIDFontType2 /DW 1000 /W [16 [500 500 500] 1 1 300] >>".to_vec(),
            strom(
                "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [2 0 0 2 0 0] /Resources << /Font << /S 6 0 R >> /XObject << /Selbst 8 0 R >> >>",
                b"BT /S 5 Tf 1 2 Td <001100170001001300100018> Tj ET 0 0 m 5 0 l S /Selbst Do",
            ),
            strom("/Type /XObject /Subtype /Image /Width 1 /Height 1", b"\0"),
            strom("", tu),
        ],
    );
    assert_eq!(s.texte.len(), 1);
    let t = &s.texte[0];
    assert_eq!(t.text, "17/308");
    assert!(nah(t.lage, [52.0, 4.0]) && (t.groesse - 10.0).abs() < 1e-9);
    // Breiten je CID: 17 und 16 aus dem Feld, 1 aus dem Bereich, sonst DW
    let breite = (500.0 + 1000.0 + 300.0 + 1000.0 + 500.0 + 1000.0) / 1000.0 * 5.0;
    assert!(nah(t.ende, [52.0 + 2.0 * breite, 4.0]));
    assert_eq!(
        s.pfade.len(),
        1,
        "Formular ruft sich nicht selbst endlos auf"
    );
    assert!(nah(s.pfade[0].zuege[0].punkte[1], [60.0, 0.0]));
    assert_eq!(s.pfade[0].strich.map(|s| s.1), Some(2.0));
    assert_eq!(s.bilder, vec![[0.0, 0.0, 1.0, 1.0]]);
}

#[test]
fn eingebettetes_bild_und_unsinn() {
    let s = seite_aus(
        b"BI /W 2 /H 1 /BPC 8 /CS /G ID \x00EI\x01 EI 0 0 m 1 1 l S ) ] >> }} 1 2 3 4 5 6 7 cm m l c re Tj TJ Do Tf",
        "",
        &[],
    );
    assert_eq!(s.bilder.len(), 1);
    assert_eq!(s.pfade.len(), 1);
}

#[test]
fn beispiel_lageplan() {
    let pdf = Pdf::lesen(include_bytes!("../../tests/daten/lageplan-lindenhof-7.pdf")).unwrap();
    let s = seite(&pdf, &pdf.seiten()[0]);
    // Grenze rot 0,9 pt, geschlossen, vier Ecken
    let g = &s.pfade[0];
    assert_eq!(g.strich, Some(([204, 0, 0], 0.9)));
    assert!(g.zuege[0].zu);
    assert_eq!(g.zuege[0].punkte.len(), 4);
    assert!(nah(g.zuege[0].punkte[0], [237.73, 374.18]));
    let kopf = s
        .texte
        .iter()
        .find(|t| t.text.starts_with("Maßstab"))
        .unwrap();
    assert!(nah(kopf.lage, [60.0, 774.0]) && kopf.groesse == 8.0);
    assert!(kopf.text.contains("1:500 · ETRS89"));
    let nummern: Vec<&str> = s
        .texte
        .iter()
        .filter(|t| t.groesse == 3.6)
        .map(|t| t.text.as_str())
        .collect();
    assert_eq!(nummern, ["1", "2", "3", "4"]);
}

/// Bomben (Review W37): je eine kleine Datei, die ohne Grenze Gigabytes
/// oder Stunden kostete. Sie öffnen, kosten höchstens das Budget und
/// brechen nie ab.
mod bomben {
    use super::*;
    use std::time::Instant;

    /// Gespeicherter Deflate-Block (BTYPE 00) um `d`.
    fn gespeichert(d: &[u8]) -> Vec<u8> {
        let n = d.len() as u16;
        let mut v = vec![1];
        v.extend_from_slice(&n.to_le_bytes());
        v.extend_from_slice(&(!n).to_le_bytes());
        v.extend_from_slice(d);
        v
    }

    fn katalog_und_seite(contents: &str, res: &str) -> Vec<Vec<u8>> {
        vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            format!("<< /Type /Page /Parent 2 0 R /Contents {contents} /Resources << {res} >> >>")
                .into_bytes(),
        ]
    }

    /// B1 und B2: /Contents nennt tausendmal denselben Strom (je 1 MB
    /// entpackt), und eine Filterkette entpackt zweimal hintereinander.
    #[test]
    fn inhalt_und_filterkette() {
        let bombe = crate::inflate::bombe(4064);
        let refs = vec!["4 0 R"; 1000].join(" ");
        let mut o = katalog_und_seite(&format!("[{refs}]"), "");
        o.push(strom("/Filter /FlateDecode", &bombe));
        let roh = pdf_aus(&o);
        let pdf = Pdf::lesen_mit(&roh, 8 << 20).unwrap();
        let b = pdf.seiten().remove(0);
        let t = Instant::now();
        let inhalt = pdf.inhalt(&b);
        assert!(pdf.zu_gross());
        assert!(inhalt.len() <= 8 << 20, "{}", inhalt.len());
        assert!(t.elapsed().as_secs_f64() < 10.0);
        // zweimal Flate: außen gespeichert, innen die Bombe
        let mut o = katalog_und_seite("4 0 R", "");
        o.push(strom(
            "/Filter [/FlateDecode /FlateDecode]",
            &gespeichert(&bombe),
        ));
        let roh = pdf_aus(&o);
        let pdf = Pdf::lesen_mit(&roh, 1 << 20).unwrap();
        let b = pdf.seiten().remove(0);
        assert!(pdf.inhalt(&b).is_empty());
        assert!(pdf.zu_gross());
        let pdf = Pdf::lesen_mit(&roh, 2 << 20).unwrap();
        assert_eq!(pdf.inhalt(&b).len(), 1 + 258 * 4064 + 1);
        assert!(!pdf.zu_gross());
    }

    /// B3: ein Objektstrom mit 1000 Nummern, alle auf demselben 100-kB-Feld;
    /// nur das erste gilt, die übrigen reichen in es hinein.
    #[test]
    fn objektstrom_mit_gleichen_stellen() {
        let feld = format!("[{}]", "0 ".repeat(50_000));
        let paare: String = (0..1000).map(|i| format!("{} 0 ", 10 + i)).collect();
        let mut d = paare.into_bytes();
        let erst = d.len();
        d.extend_from_slice(feld.as_bytes());
        let mut o = katalog_und_seite("[]", "");
        o.push(strom(&format!("/Type /ObjStm /N 1000 /First {erst}"), &d));
        let mut roh = pdf_aus(&o);
        // kaputte Querverweise: Skizzeo sucht und findet den Objektstrom
        let p = roh.windows(9).rposition(|w| w == b"startxref").unwrap();
        roh.truncate(p);
        roh.extend_from_slice(b"startxref\n9\n%%EOF\n");
        let t = Instant::now();
        let pdf = Pdf::lesen(&roh).unwrap();
        assert!(t.elapsed().as_secs_f64() < 10.0);
        assert_eq!(
            pdf.objekt(10).and_then(|o| o.feld().map(|f| f.len())),
            Some(50_000)
        );
        assert!(pdf.objekt(11).is_none());
    }

    /// B4: zwölf Stufen Formulare, jedes ruft das nächste zehnmal auf
    /// (10^12 Läufe ohne Grenze in der Breite).
    #[test]
    fn formulare_in_der_breite() {
        let mut o = katalog_und_seite("4 0 R", "/XObject << /X 5 0 R >>");
        o.push(strom("", b"/X Do"));
        for k in 0..12 {
            let aufruf = "/X Do ".repeat(10);
            o.push(strom(
                &format!(
                    "/Type /XObject /Subtype /Form /Resources << /XObject << /X {} 0 R >> >>",
                    6 + k
                ),
                format!("0 0 m 1 1 l S {aufruf}").as_bytes(),
            ));
        }
        o.push(strom("/Type /XObject /Subtype /Form", b"0 0 m 1 1 l S"));
        let roh = pdf_aus(&o);
        let pdf = Pdf::lesen(&roh).unwrap();
        let b = pdf.seiten().remove(0);
        let t = Instant::now();
        let s = seite(&pdf, &b);
        assert!(t.elapsed().as_secs_f64() < 20.0);
        assert!(s.pfade.len() <= 20_001, "{}", s.pfade.len());
    }

    /// B5 und B8: tausend bfrange-Tripel über alle Codes, riesige CIDs und
    /// Breiten am Rand von i64 laufen weder lange noch über.
    #[test]
    fn cmap_und_breiten() {
        let mut cm = String::from("1000 beginbfrange ");
        for _ in 0..1000 {
            cm.push_str("<0000> <FFFF> <0041> ");
        }
        cm.push_str("endbfrange 1 begincidrange <0000> <0010> 9223372036854775807 endcidrange");
        let res = "/Font << /F1 5 0 R >>";
        let mut o = katalog_und_seite("4 0 R", res);
        o.push(strom("", b"BT /F1 12 Tf 10 10 Td <0001> Tj ET"));
        o.push(b"<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding 6 0 R /ToUnicode 6 0 R /DescendantFonts [7 0 R] >>".to_vec());
        o.push(strom("", cm.as_bytes()));
        o.push(b"<< /Type /Font /Subtype /CIDFontType2 /W [9223372036854775807 [500 500] 9223372036854775000 9223372036854775807 300] >>".to_vec());
        let roh = pdf_aus(&o);
        let pdf = Pdf::lesen(&roh).unwrap();
        let b = pdf.seiten().remove(0);
        let t = Instant::now();
        let s = seite(&pdf, &b);
        assert!(t.elapsed().as_secs_f64() < 20.0);
        assert_eq!(s.texte.len(), 1);
    }

    /// B6 und B8: viele Ströme ohne endstream und ohne Querverweise, dazu
    /// eine Tabelle mit riesiger Anfangsnummer.
    #[test]
    fn ohne_endstream_und_ueberlauf() {
        let mut roh = b"%PDF-1.4\n".to_vec();
        for i in 1..5000 {
            roh.extend_from_slice(format!("{i} 0 obj << /Length 999999 >> stream\n").as_bytes());
            roh.extend_from_slice(&[b'x'; 200]);
            roh.push(b'\n');
        }
        let t = Instant::now();
        assert!(Pdf::lesen(&roh).is_err());
        assert!(t.elapsed().as_secs_f64() < 10.0);
        let mut o = katalog_und_seite("[]", "");
        o.push(b"<< >>".to_vec());
        let roh = pdf_aus(&o);
        let x = String::from_utf8_lossy(&roh).replace("xref\n0 5", "xref\n9223372036854775807 5");
        let _ = Pdf::lesen(x.as_bytes());
    }
}

//! Prüfungen des PDF-Lesers am Beispiel-Lageplan und seinen Varianten aus
//! `gelaende/beispiel/erzeuge_lageplan_varianten.py`.

use super::schutz::KENNWORT;
use super::*;

const ORIGINAL: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7.pdf");
const RC4_40: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-rc4-40.pdf");
const RC4_128: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-rc4-128.pdf");
const AES_128: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-aes-128.pdf");
const AES_256: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-aes-256.pdf");
const AES_256_R5: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-aes-256-r5.pdf");
const KENNWORT_PDF: &[u8] = include_bytes!("../../tests/daten/lageplan-lindenhof-7-kennwort.pdf");

fn eine_seite(roh: &[u8]) -> (Pdf, Blatt, Vec<u8>) {
    let pdf = Pdf::lesen(roh).unwrap();
    let s = pdf.seiten();
    assert_eq!(s.len(), 1);
    let b = s.into_iter().next().unwrap();
    let inhalt = pdf.inhalt(&b);
    (pdf, b, inhalt)
}

#[test]
fn original_lesen() {
    let (pdf, b, inhalt) = eine_seite(ORIGINAL);
    assert_eq!(b.mediabox, [0.0, 0.0, 595.28, 841.89]);
    assert_eq!(b.drehung, 0);
    assert!(pdf.schutz().is_none());
    assert!(pdf.hinweise.is_empty());
    assert_eq!(pdf.erzeuger(), "");
    let f = pdf.verz(&b.ressourcen, "Font").unwrap();
    let f1 = pdf.aufloesen(f.get("F1").unwrap());
    assert_eq!(f1.verz().unwrap().name("BaseFont"), Some("Helvetica"));
    assert!(inhalt.starts_with(b"q 0.8 0 0 RG 0.9 w 237.73 374.18 m"));
    assert!(inhalt.windows(9).any(|w| w == b"Ma\xDFstab 1"));
}

#[test]
fn varianten_ergeben_denselben_inhalt() {
    let (_, b0, soll) = eine_seite(ORIGINAL);
    for (roh, v, r) in [
        (RC4_40, 1, 2),
        (RC4_128, 2, 3),
        (AES_128, 4, 4),
        (AES_256, 5, 6),
        (AES_256_R5, 5, 5),
    ] {
        let (pdf, b, inhalt) = eine_seite(roh);
        assert_eq!(pdf.schutz(), Some((v, r)));
        assert!(pdf.hinweise.is_empty(), "{:?}", pdf.hinweise);
        assert_eq!(inhalt, soll, "V {v} R {r}");
        assert_eq!(b.mediabox, b0.mediabox);
        assert_eq!(pdf.erzeuger(), "Skizzeo Beispiel");
        let info = pdf.verz(&pdf.trailer, "Info").unwrap();
        assert_eq!(
            info.get("Title").and_then(Objekt::text),
            Some(&b"Lageplan Am Lindenhof 7"[..])
        );
    }
    // der Nachtrag über /Prev ersetzt die Seite
    let (_, b, _) = eine_seite(AES_256);
    assert_eq!(b.verz.ganz("Rotate"), Some(0));
    assert_eq!(b0.verz.get("Rotate"), None);
}

#[test]
fn kennwort_wird_ein_satz() {
    assert_eq!(Pdf::lesen(KENNWORT_PDF).err().as_deref(), Some(KENNWORT));
    assert_eq!(KENNWORT, "Die Datei ist mit Kennwort geschützt.");
    assert_eq!(
        Pdf::lesen(b"kein pdf").err().as_deref(),
        Some("Das ist keine PDF-Datei.")
    );
}

#[test]
fn kaputte_querverweise_werden_gesucht() {
    // alle Lagen in der Tabelle um 3 verschoben, startxref falsch
    let mut roh = ORIGINAL.to_vec();
    let x = roh.windows(6).rposition(|w| w == b"\nxref\n").unwrap();
    let t = String::from_utf8_lossy(&roh[x..]).replace("0000000015", "0000000018");
    roh.truncate(x);
    roh.extend_from_slice(t.as_bytes());
    let (pdf, _, inhalt) = eine_seite(&roh);
    assert_eq!(pdf.hinweise.len(), 1);
    assert_eq!(inhalt, eine_seite(ORIGINAL).2);
    // ganz ohne Querverweise und Trailer: Katalog über /Type /Catalog
    let roh = &ORIGINAL[..ORIGINAL.windows(6).rposition(|w| w == b"\nxref\n").unwrap()];
    let (pdf, _, inhalt) = eine_seite(roh);
    assert_eq!(pdf.hinweise.len(), 1);
    assert_eq!(inhalt, eine_seite(ORIGINAL).2);
    // ohne „endstream“ an der Stelle von /Length: Länge falsch angegeben
    let roh = String::from_utf8_lossy(ORIGINAL).replace("/Length 14060", "/Length 99");
    let roh: Vec<u8> = roh.chars().map(|c| c as u32 as u8).collect();
    let _ = Pdf::lesen(&roh);
}

#[test]
fn querverweisstrom_von_hand() {
    // Objektstrom und Querverweisstrom ohne Filter, Seitenbaum mit Erbe
    let os_rumpf = b"<< /Type /Catalog /Pages 2 0 R >>\n<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 100 200] /Resources << /Font << /F1 9 0 R >> >> /Rotate 90 >>\n";
    let zweites = os_rumpf.iter().position(|&b| b == b'\n').unwrap() + 1;
    let os_kopf = format!("1 0 2 {zweites} ");
    let mut roh = b"%PDF-1.5\n".to_vec();
    let p3 = roh.len();
    roh.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n");
    let p4 = roh.len();
    roh.extend_from_slice(b"4 0 obj\n<< /Length 5 >>\nstream\n0 0 m\nendstream\nendobj\n");
    let p5 = roh.len();
    let os = [os_kopf.as_bytes(), os_rumpf].concat();
    roh.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /ObjStm /N 2 /First {} /Length {} >>\nstream\n",
            os_kopf.len(),
            os.len()
        )
        .as_bytes(),
    );
    roh.extend_from_slice(&os);
    roh.extend_from_slice(b"\nendstream\nendobj\n");
    let p6 = roh.len();
    let mut z = Vec::new();
    let mut zeile = |a: u8, b: usize, c: u8| {
        z.push(a);
        z.extend_from_slice(&(b as u16).to_be_bytes());
        z.push(c);
    };
    zeile(0, 0, 255);
    zeile(2, 5, 0);
    zeile(2, 5, 1);
    zeile(1, p3, 0);
    zeile(1, p4, 0);
    zeile(1, p5, 0);
    zeile(1, p6, 0);
    roh.extend_from_slice(
        format!(
            "6 0 obj\n<< /Type /XRef /Size 7 /W [1 2 1] /Root 1 0 R /Length {} >>\nstream\n",
            z.len()
        )
        .as_bytes(),
    );
    roh.extend_from_slice(&z);
    roh.extend_from_slice(format!("\nendstream\nendobj\nstartxref\n{p6}\n%%EOF\n").as_bytes());
    let pdf = Pdf::lesen(&roh).unwrap();
    assert!(pdf.hinweise.is_empty(), "{:?}", pdf.hinweise);
    let s = pdf.seiten();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].mediabox, [0.0, 0.0, 100.0, 200.0]);
    assert_eq!(s[0].drehung, 90);
    assert!(s[0].ressourcen.get("Font").is_some());
    assert_eq!(pdf.inhalt(&s[0]), b"0 0 m\n".to_vec());
}

#[test]
fn unsinn_bricht_nicht_ab() {
    let mut x: u32 = 99;
    // AES-256 R6 rechnet je Öffnen 128 Runden Kennwort-Hash; hier nicht
    for roh in [ORIGINAL, RC4_40, AES_128] {
        for n in (0..roh.len()).step_by(997) {
            if let Ok(p) = Pdf::lesen(&roh[..n]) {
                for s in p.seiten() {
                    let _ = p.inhalt(&s);
                }
            }
        }
        for _ in 0..40 {
            let mut f = roh.to_vec();
            for _ in 0..8 {
                x = x.wrapping_mul(1103515245).wrapping_add(12345);
                let i = (x >> 8) as usize % f.len();
                f[i] = (x >> 3) as u8;
            }
            if let Ok(p) = Pdf::lesen(&f) {
                for s in p.seiten() {
                    let _ = p.inhalt(&s);
                }
            }
        }
    }
    // Längen, die aufeinander zeigen
    let roh = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R /L 3 0 R >>\nendobj\n3 0 obj\n<< /Length 4 0 R >>\nstream\nab\nendstream\nendobj\n4 0 obj\n<< /Length 3 0 R >>\nstream\ncd\nendstream\nendobj\n";
    let p = Pdf::lesen(roh).unwrap();
    let Some(Objekt::Strom(s)) = p.objekt(3) else {
        panic!("Strom 3");
    };
    assert_eq!(s.daten, b"ab".to_vec());
    // Seitenbaum mit Kreis
    let roh = b"%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [2 0 R 3 0 R] >>\nendobj\n3 0 obj\n<< /Type /Pages /Kids [2 0 R] >>\nendobj\n";
    let p = Pdf::lesen(roh).unwrap();
    assert!(p.seiten().is_empty());
}

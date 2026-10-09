//! Eigene PDF-Ausgabe (PDF 1.4) für Blätter aus Text und Linien: Seiten in
//! Punkt, die Schriften der App als Teilmenge eingebettet (TrueType als
//! `CIDFontType2`, Glyphen über `Identity-H`, `ToUnicode` zum Suchen und
//! Kopieren), Ströme mit eigenem Deflate. Dieselben Seiten zeichnet die
//! Vorschau auf die Leinwand ([`zeichnen`]), so zeigt sie genau das PDF.

use crate::deflate::zlib;
use crate::font::Font;
use crate::{Canvas, Rgba};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A4 hoch in Punkt.
pub const A4: (f32, f32) = (595.276, 841.89);

/// Millimeter in Punkt.
pub fn mm(v: f32) -> f32 {
    v * 72.0 / 25.4
}

/// Ein Zeichenbefehl; Punkt, Ursprung oben links, y nach unten. `grau`:
/// 0 schwarz bis 255 weiß.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Text mit Grundlinie bei (x, y).
    Text {
        x: f32,
        y: f32,
        pt: f32,
        fett: bool,
        grau: u8,
        text: String,
    },
    /// Gerade Linie der Stärke `breite`.
    Linie {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        breite: f32,
        grau: u8,
    },
}

/// Eine Seite.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seite {
    pub ops: Vec<Op>,
}

/// Zahl für den Inhaltsstrom: höchstens drei Nachkommastellen, ohne
/// überflüssige Nullen.
fn n(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

fn grau(g: u8) -> String {
    n(f32::from(g) / 255.0)
}

/// Glyphen eines Textes als Hex-Zeichenkette für `Tj`.
fn glyphen(f: &Font, text: &str) -> String {
    let mut s = String::from("<");
    for c in text.chars() {
        let _ = write!(s, "{:04X}", f.glyph(c));
    }
    s.push('>');
    s
}

/// Inhaltsstrom einer Seite der Höhe `h` (Punkt), Schriften `/F1` und
/// `/F2` (fett).
fn inhalt(seite: &Seite, h: f32, schriften: [&Font; 2]) -> String {
    let mut s = String::new();
    for op in &seite.ops {
        match op {
            Op::Text {
                x,
                y,
                pt,
                fett,
                grau: g,
                text,
            } => {
                let (name, f) = if *fett {
                    ("F2", schriften[1])
                } else {
                    ("F1", schriften[0])
                };
                let _ = writeln!(
                    s,
                    "BT /{name} {} Tf {} g 1 0 0 1 {} {} Tm {} Tj ET",
                    n(*pt),
                    grau(*g),
                    n(*x),
                    n(h - y),
                    glyphen(f, text)
                );
            }
            Op::Linie {
                x0,
                y0,
                x1,
                y1,
                breite,
                grau: g,
            } => {
                let _ = writeln!(
                    s,
                    "{} G {} w {} {} m {} {} l S",
                    grau(*g),
                    n(*breite),
                    n(*x0),
                    n(h - y0),
                    n(*x1),
                    n(h - y1)
                );
            }
        }
    }
    s
}

/// Text als PDF-Zeichenkette in UTF-16 mit Kennung (für `/Title`).
fn utf16(text: &str) -> String {
    let mut s = String::from("<FEFF");
    for u in text.encode_utf16() {
        let _ = write!(s, "{u:04X}");
    }
    s.push('>');
    s
}

/// `ToUnicode`-Tabelle: Glyphe → Zeichen.
fn to_unicode(benutzt: &BTreeMap<u16, char>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let v: Vec<(&u16, &char)> = benutzt.iter().collect();
    for block in v.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", block.len());
        for (g, c) in block {
            let mut u = String::new();
            for x in c.encode_utf16(&mut [0; 2]) {
                let _ = write!(u, "{x:04X}");
            }
            let _ = writeln!(s, "<{g:04X}> <{u}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CIDResource defineresource pop\nend\nend\n");
    s
}

struct Schreiber {
    out: Vec<u8>,
    lage: Vec<usize>,
}

impl Schreiber {
    fn obj(&mut self, id: usize, body: &str) {
        self.lage[id - 1] = self.out.len();
        self.out
            .extend(format!("{id} 0 obj\n{body}\nendobj\n").bytes());
    }

    fn strom(&mut self, id: usize, extra: &str, data: &[u8]) {
        let z = zlib(data);
        self.lage[id - 1] = self.out.len();
        self.out.extend(
            format!(
                "{id} 0 obj\n<< /Length {} /Filter /FlateDecode{extra} >>\nstream\n",
                z.len()
            )
            .bytes(),
        );
        self.out.extend(z);
        self.out.extend(b"\nendstream\nendobj\n");
    }
}

/// Kennung einer eingebetteten Teilmenge: sechs Großbuchstaben vor dem
/// Namen („ABCDEF+Skizzeo-Regular“, PDF 1.7, 9.6.4), aus dem Inhalt
/// berechnet, damit dasselbe Blatt dieselbe Datei gibt.
fn kennung(datei: &[u8]) -> String {
    let mut h = datei.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    (0..6)
        .map(|_| {
            let c = char::from(b'A' + (h % 26) as u8);
            h /= 26;
            c
        })
        .collect()
}

/// Das PDF der Seiten `seiten` im Format `groesse` (Punkt) mit den
/// Schriften `regular` und `fett`; `titel` steht in den Eigenschaften.
/// Von den Schriften geht nur die Teilmenge der benutzten Glyphen als
/// `FontFile2` hinein ([`Font::teilmenge`]); das geht, weil [`Font::parse`]
/// nur einfache TrueType-Dateien annimmt (keine `.ttc`, kein CFF).
pub fn schreiben(
    seiten: &[Seite],
    groesse: (f32, f32),
    regular: &Font,
    fett: &Font,
    titel: &str,
) -> Vec<u8> {
    let (w, h) = groesse;
    let schriften = [regular, fett];
    // Benutzte Glyphen je Schrift, für /W und ToUnicode
    let mut benutzt = [BTreeMap::new(), BTreeMap::new()];
    for s in seiten {
        for op in &s.ops {
            if let Op::Text { fett, text, .. } = op {
                let k = usize::from(*fett);
                for c in text.chars() {
                    benutzt[k].entry(schriften[k].glyph(c)).or_insert(c);
                }
            }
        }
    }
    // 1 Katalog, 2 Seiten, 3 Eigenschaften, je Schrift 5 Objekte, dann je
    // Seite Seite und Inhalt
    let schrift_id = |k: usize| 4 + 5 * k;
    let seite_id = |i: usize| 14 + 2 * i;
    let anzahl = 13 + 2 * seiten.len();
    let mut p = Schreiber {
        out: b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec(),
        lage: vec![0; anzahl],
    };
    p.obj(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let kinder: Vec<String> = (0..seiten.len())
        .map(|i| format!("{} 0 R", seite_id(i)))
        .collect();
    p.obj(
        2,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kinder.join(" "),
            seiten.len()
        ),
    );
    p.obj(
        3,
        &format!(
            "<< /Title {} /Producer (Skizzeo) /Creator (Skizzeo) >>",
            utf16(titel)
        ),
    );
    for (k, f) in schriften.iter().enumerate() {
        let id = schrift_id(k);
        let datei = f.teilmenge(&benutzt[k].keys().copied().collect());
        let name = format!(
            "{}+Skizzeo-{}",
            kennung(&datei),
            if k == 0 { "Regular" } else { "Bold" }
        );
        let mut breiten = String::new();
        for g in benutzt[k].keys() {
            let _ = write!(breiten, "{g} [{}] ", n(f.advance_milli(*g)));
        }
        p.obj(
            id,
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H \
                 /DescendantFonts [{} 0 R] /ToUnicode {} 0 R >>",
                id + 1,
                id + 4
            ),
        );
        p.obj(
            id + 1,
            &format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{name} \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor {} 0 R /DW 1000 /W [{}] /CIDToGIDMap /Identity >>",
                id + 2,
                breiten.trim_end()
            ),
        );
        let (auf, ab, bbox) = f.metrik_milli();
        p.obj(
            id + 2,
            &format!(
                "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [{} {} {} {}] \
                 /ItalicAngle 0 /Ascent {} /Descent {} /CapHeight {} /StemV {} /FontFile2 {} 0 R >>",
                n(bbox[0]),
                n(bbox[1]),
                n(bbox[2]),
                n(bbox[3]),
                n(auf),
                n(ab),
                n(auf * 0.7),
                if k == 0 { 80 } else { 140 },
                id + 3
            ),
        );
        p.strom(id + 3, &format!(" /Length1 {}", datei.len()), &datei);
        p.strom(id + 4, "", to_unicode(&benutzt[k]).as_bytes());
    }
    for (i, s) in seiten.iter().enumerate() {
        let id = seite_id(i);
        p.obj(
            id,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
                 /Resources << /Font << /F1 {} 0 R /F2 {} 0 R >> >> /Contents {} 0 R >>",
                n(w),
                n(h),
                schrift_id(0),
                schrift_id(1),
                id + 1
            ),
        );
        p.strom(id + 1, "", inhalt(s, h, schriften).as_bytes());
    }
    let xref = p.out.len();
    let mut t = format!("xref\n0 {}\n0000000000 65535 f \n", anzahl + 1);
    for l in &p.lage {
        let _ = writeln!(t, "{l:010} 00000 n ");
    }
    let _ = write!(
        t,
        "trailer\n<< /Size {} /Root 1 0 R /Info 3 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        anzahl + 1
    );
    p.out.extend(t.bytes());
    p.out
}

/// Zeichnet eine Seite auf die Leinwand: `(x, y)` ist die linke obere Ecke
/// des Blatts in Pixeln, `k` Pixel je Punkt. Text in der Schrift des PDF.
pub fn zeichnen(
    c: &mut Canvas,
    seite: &Seite,
    (x, y): (f32, f32),
    k: f32,
    regular: &Font,
    fett: &Font,
) {
    let farbe = |g: u8| Rgba(g, g, g, 255);
    for op in &seite.ops {
        match op {
            Op::Text {
                x: tx,
                y: ty,
                pt,
                fett: f,
                grau: g,
                text,
            } => {
                let font = if *f { fett } else { regular };
                font.draw(c, text, pt * k, x + tx * k, y + ty * k, farbe(*g));
            }
            Op::Linie {
                x0,
                y0,
                x1,
                y1,
                breite,
                grau: g,
            } => {
                let b = (breite * k).max(1.0);
                let (ax, ay) = (x + x0 * k, y + y0 * k);
                let (bx, by) = (x + x1 * k, y + y1 * k);
                // Nur waagrechte und senkrechte Linien
                if (ay - by).abs() < 0.01 {
                    c.fill_rect(ax.min(bx), ay - b * 0.5, (bx - ax).abs(), b, farbe(*g));
                } else {
                    c.fill_rect(ax - b * 0.5, ay.min(by), b, (by - ay).abs(), farbe(*g));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schrift(name: &str) -> Option<Font> {
        let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
        std::fs::read(lib.join(name)).ok().and_then(Font::parse)
    }

    #[test]
    fn zahlen_kurz() {
        assert_eq!(n(12.0), "12");
        assert_eq!(n(12.5), "12.5");
        assert_eq!(n(0.12345), "0.123");
        assert_eq!(n(-0.0001), "0");
        assert_eq!(utf16("Ä"), "<FEFF00C4>");
    }

    /// Aufbau: Kopf, jedes Objekt dort, wo die Querverweistabelle es
    /// nennt, Schriften eingebettet, Umlaute und m²/m³ in `ToUnicode`.
    #[test]
    fn aufbau_und_querverweise() {
        let (Some(r), Some(b)) = (
            schrift("LiberationSans-Regular.ttf"),
            schrift("LiberationSans-Bold.ttf"),
        ) else {
            return;
        };
        let seite = Seite {
            ops: vec![
                Op::Text {
                    x: 56.7,
                    y: 60.0,
                    pt: 9.0,
                    fett: false,
                    grau: 0,
                    text: "Mauerwerk Planstein 17,5 cm, 172,224 m² · Größe".into(),
                },
                Op::Text {
                    x: 56.7,
                    y: 80.0,
                    pt: 10.0,
                    fett: true,
                    grau: 0,
                    text: "01 Betonarbeiten 23,906 m³".into(),
                },
                Op::Linie {
                    x0: 56.7,
                    y0: 84.0,
                    x1: 552.8,
                    y1: 84.0,
                    breite: 0.5,
                    grau: 128,
                },
            ],
        };
        let pdf = schreiben(&[seite.clone(), seite.clone()], A4, &r, &b, "LV Rohbau");
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        let start: usize = text
            .rsplit("startxref\n")
            .next()
            .and_then(|r| r.lines().next())
            .and_then(|z| z.parse().ok())
            .unwrap();
        assert!(pdf[start..].starts_with(b"xref\n0 18\n"));
        let tabelle = std::str::from_utf8(&pdf[start..]).unwrap();
        for (i, z) in tabelle.lines().skip(3).take(17).enumerate() {
            let lage: usize = z[..10].parse().unwrap();
            let kopf = format!("{} 0 obj\n", i + 1);
            assert!(pdf[lage..].starts_with(kopf.as_bytes()), "Objekt {}", i + 1);
        }
        assert_eq!(text.matches("/FontFile2").count(), 2);
        assert!(text.contains("/Count 2"));
        // Inhalt: Text als Glyphen, y von unten
        let i = inhalt(&seite, A4.1, [&r, &b]);
        let g = |c: char| format!("{:04X}", r.glyph(c));
        assert!(i.contains(&format!(
            "BT /F1 9 Tf 0 g 1 0 0 1 56.7 781.89 Tm <{}",
            g('M')
        )));
        assert!(
            i.contains("0.502 G 0.5 w 56.7 757.89 m 552.8 757.89 l S"),
            "{i}"
        );
        // ToUnicode: ö, ², ³ und ß kommen zurück
        let mut benutzt = BTreeMap::new();
        for c in "Größe m² m³".chars() {
            benutzt.insert(r.glyph(c), c);
        }
        let tu = to_unicode(&benutzt);
        for (c, u) in [('ö', "00F6"), ('²', "00B2"), ('³', "00B3"), ('ß', "00DF")] {
            assert_ne!(r.glyph(c), 0, "{c} fehlt in der Schrift");
            assert!(tu.contains(&format!("<{:04X}> <{u}>", r.glyph(c))), "{c}");
        }
    }

    /// Hinweis P: die Schriften gehen nur als Teilmenge hinein (Kennung vor
    /// dem Namen, ein Bruchteil der Datei), und der Rundlauf über Poppler
    /// gibt den Text zurück und zeichnet ihn. Ohne Poppler prüft der Test
    /// nur die Größe.
    #[test]
    fn teilmenge_im_rundlauf() {
        let (Some(r), Some(b)) = (
            schrift("LiberationSans-Regular.ttf"),
            schrift("LiberationSans-Bold.ttf"),
        ) else {
            return;
        };
        let zeilen = [
            (false, "Mauerwerk Planstein 17,5 cm, 172,224 m² · Größe"),
            (true, "01 Betonarbeiten 23,906 m³ Übertrag"),
            (false, "Äußere Wände, Öffnungen ÄÖÜ äöü ß € 1.234,56"),
        ];
        let seite = Seite {
            ops: zeilen
                .iter()
                .enumerate()
                .map(|(i, (fett, text))| Op::Text {
                    x: 56.7,
                    y: 60.0 + 20.0 * i as f32,
                    pt: 10.0,
                    fett: *fett,
                    grau: 0,
                    text: (*text).into(),
                })
                .collect(),
        };
        let pdf = schreiben(&[seite], A4, &r, &b, "Rundlauf");
        let ganz = r.data().len() + b.data().len();
        assert!(pdf.len() * 10 < ganz, "{} Bytes", pdf.len());
        let text = String::from_utf8_lossy(&pdf);
        for name in ["+Skizzeo-Regular", "+Skizzeo-Bold"] {
            let k = text.find(name).expect(name);
            let kennung = &text[k - 7..k];
            assert!(kennung.starts_with('/'), "{name}: {kennung:?}");
            assert!(
                kennung[1..].bytes().all(|c| c.is_ascii_uppercase()),
                "{name}: {kennung:?}"
            );
        }

        let dir = std::env::temp_dir().join(format!("sk-pdf-rundlauf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let datei = dir.join("rundlauf.pdf");
        std::fs::write(&datei, &pdf).unwrap();
        let lauf = |prog: &str, args: &[&std::ffi::OsStr]| {
            std::process::Command::new(prog).args(args).output().ok()
        };
        let Some(aus) = lauf("pdftotext", &[datei.as_os_str(), "-".as_ref()]) else {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        };
        assert!(aus.status.success());
        assert!(
            aus.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&aus.stderr)
        );
        let gelesen = String::from_utf8(aus.stdout).unwrap();
        for (_, z) in zeilen {
            assert!(gelesen.contains(z), "{z:?} fehlt in {gelesen:?}");
        }
        // pdffonts: beide eingebettet und als Teilmenge erkannt
        if let Some(aus) = lauf("pdffonts", &[datei.as_os_str()]) {
            let liste = String::from_utf8_lossy(&aus.stdout);
            let zeilen: Vec<&str> = liste.lines().filter(|z| z.contains("Skizzeo")).collect();
            assert_eq!(zeilen.len(), 2, "{liste}");
            for z in zeilen {
                assert!(z.contains("CID TrueType"), "{z}");
                assert!(z.contains(" yes yes yes "), "{z}");
            }
        }
        // pdftoppm zeichnet die Glyphen: ohne Fehler und mit Tinte
        let bild = dir.join("seite");
        if let Some(aus) = lauf(
            "pdftoppm",
            &[
                "-r".as_ref(),
                "72".as_ref(),
                "-gray".as_ref(),
                datei.as_os_str(),
                bild.as_os_str(),
            ],
        ) {
            assert!(
                aus.stderr.is_empty(),
                "{}",
                String::from_utf8_lossy(&aus.stderr)
            );
            let pgm = std::fs::read(dir.join("seite-1.pgm")).unwrap();
            // P5-Kopf: Kennung, Breite Höhe, Maximum
            let kopf = pgm
                .split(|c| *c == b'\n')
                .take(3)
                .map(|z| z.len() + 1)
                .sum::<usize>();
            let dunkel = pgm[kopf..].iter().filter(|p| **p < 128).count();
            assert!(dunkel > 1000, "nur {dunkel} dunkle Pixel");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

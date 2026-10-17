//! Prozedurale Muster als Bild für den 3D-Export (Jörn 17.10.: „Texturen
//! mit übergeben, sofern es bei prozeduralen Texturen möglich ist“): Das
//! Muster einer Oberfläche wird über eine Kachel abgetastet
//! ([`proctex::sample`], dieselbe Formel wie der Shader) und als PNG
//! mitgegeben; im Zielprogramm wiederholt sie sich auf der Fläche.
//!
//! Die Kachel ist ein Vielfaches des Rasters (Steine, Schichten,
//! Schaltafeln, Bretter, Platten), damit Fugen über den Rand laufen. Putz
//! und Sichtbeton haben dazu Rauschen ohne Raster; ihre Kachel wird an den
//! Rändern überblendet, damit keine Naht zu sehen ist. Steine, Bretter
//! und Platten wiederholen ihre Farben je Kachel.

use sk_model::proctex::{self, Bond, Pattern};

/// Größte Kantenlänge einer Kachel in Bildpunkten; große Kacheln (wilder
/// Verband, über 4 m) bekommen das Doppelte, damit Fugen sichtbar bleiben.
const PX: f64 = 1024.0;
const PX_GROSS: f64 = 2048.0;
/// Feinste Auflösung (mm je Bildpunkt).
const MM_MIN: f64 = 2.0;
/// Zielgröße einer Kachel am Bau (mm).
const ZIEL: (f64, f64) = (2400.0, 2000.0);

/// Zeigt die Ansicht das Muster auch auf ebenen Flächen? Mauerwerk,
/// Sichtbeton und Holz zeigen dort ihre Mischfarbe (Shader
/// `pattern_rgb`), Putz, Platten und Naturstein ihr Muster.
pub(crate) fn auch_eben(p: &Pattern) -> bool {
    matches!(
        p,
        Pattern::Plaster { .. } | Pattern::Tiles { .. } | Pattern::Stone { .. }
    )
}

/// Die Kachel zum Muster auf der Grundfarbe `base`; `None` ohne Muster
/// (fremde Fassung).
pub(crate) fn kachel(p: &Pattern, base: [u8; 3]) -> Option<sk_export::Textur> {
    let (w, h, ueberblenden) = groesse(p)?;
    let px = if w.max(h) > 4000.0 { PX_GROSS } else { PX };
    let mm = (w.max(h) / px).max(MM_MIN);
    let (pw, ph) = ((w / mm).round().max(1.0), (h / mm).round().max(1.0));
    let (nw, nh) = (pw as usize, ph as usize);
    let mut rgba = vec![0u8; nw * nh * 4];
    let zeilen = |y0: usize, teil: &mut [u8]| {
        for (dy, zeile) in teil.chunks_mut(nw * 4).enumerate() {
            // Zeile 0 ist die obere Kante der Kachel
            let v = h - (y0 + dy) as f64 * h / ph - 0.5 * h / ph;
            for x in 0..nw {
                let u = (x as f64 + 0.5) * w / pw;
                let c = if ueberblenden {
                    blend(p, base, u, v, w, h)
                } else {
                    proctex::sample(p, base, u, v)
                };
                zeile[x * 4..x * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    };
    // Abtasten in Streifen auf allen Kernen
    let kerne = std::thread::available_parallelism().map_or(1, |n| n.get());
    let streifen = nh.div_ceil(kerne).max(1);
    std::thread::scope(|sc| {
        for (i, teil) in rgba.chunks_mut(streifen * nw * 4).enumerate() {
            let zeilen = &zeilen;
            sc.spawn(move || zeilen(i * streifen, teil));
        }
    });
    Some(sk_export::Textur {
        png: sk_paint::encode_png(nw as u32, nh as u32, &rgba),
        breite: w,
        hoehe: h,
    })
}

/// Breite und Höhe der Kachel (mm) und ob sie überblendet wird.
fn groesse(p: &Pattern) -> Option<(f64, f64, bool)> {
    let vielfach = |pitch: f64, ziel: f64, schritt: f64| {
        let n = (ziel / (pitch * schritt)).round().max(1.0) * schritt;
        pitch * n
    };
    Some(match p {
        Pattern::Masonry {
            len,
            h,
            joint,
            bond,
            ..
        } => {
            let (a, course) = (f64::from(*len + *joint), f64::from(*h + *joint));
            match bond {
                // Die Verbandstabelle wiederholt sich nach 128 Vierteln und
                // 128 Schichten
                Bond::Wild => (
                    proctex::WILD_SIZE as f64 * a / 4.0,
                    proctex::WILD_SIZE as f64 * course,
                    false,
                ),
                // 12 Schichten passen auf halben, drittel und Kreuzverband
                _ => (
                    vielfach(a, ZIEL.0, 1.0),
                    vielfach(course, ZIEL.1, 12.0),
                    false,
                ),
            }
        }
        Pattern::Plaster { .. } => (1000.0, 1000.0, true),
        Pattern::Concrete { w, h, .. } => (
            vielfach(f64::from(*w).max(10.0), ZIEL.0, 1.0),
            vielfach(f64::from(*h).max(10.0), ZIEL.1, 1.0),
            true,
        ),
        Pattern::Timber {
            vertical,
            board,
            joint,
            ..
        } => {
            let quer = vielfach(f64::from(*board + *joint).max(1.0), ZIEL.0, 1.0);
            if *vertical {
                (quer, ZIEL.1, false)
            } else {
                (ZIEL.1, quer, false)
            }
        }
        Pattern::Tiles {
            len,
            wid,
            joint,
            half,
            ..
        } => (
            vielfach(f64::from(*len + *joint).max(1.0), ZIEL.0, 1.0),
            vielfach(
                f64::from(*wid + *joint).max(1.0),
                ZIEL.1,
                if *half { 2.0 } else { 1.0 },
            ),
            false,
        ),
        Pattern::Stone { .. } => (ZIEL.1, ZIEL.1, false),
        Pattern::Foreign(_) => return None,
    })
}

/// Muster an (u, v), über die Kachel `w` × `h` überblendet: An jedem Rand
/// steht dasselbe wie am gegenüberliegenden, das Raster bleibt, weil die
/// Kachel ein Vielfaches davon ist.
fn blend(p: &Pattern, base: [u8; 3], u: f64, v: f64, w: f64, h: f64) -> [u8; 3] {
    let (su, sv) = (u / w, v / h);
    let mut c = [0.0f64; 3];
    for (du, wu) in [(0.0, 1.0 - su), (w, su)] {
        for (dv, wv) in [(0.0, 1.0 - sv), (h, sv)] {
            let s = proctex::sample(p, base, u - du, v - dv);
            for k in 0..3 {
                c[k] += f64::from(s[k]) * wu * wv;
            }
        }
    }
    c.map(|x| x.round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_groesse(png: &[u8]) -> (u32, u32) {
        let be = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
        (be(16), be(20))
    }

    #[test]
    fn mauerwerk_kachel_im_raster() {
        let ganz = |x: f64| (x - x.round()).abs() < 1e-9;
        for bond in [Bond::Half, Bond::Wild] {
            let mut p = proctex::masonry_default();
            let Pattern::Masonry {
                len,
                h,
                joint,
                bond: b,
                ..
            } = &mut p
            else {
                unreachable!()
            };
            *b = bond;
            let a = f64::from(*len + *joint);
            let course = f64::from(*h + *joint);
            let t = kachel(&p, [200, 100, 80]).unwrap();
            let (pw, ph) = png_groesse(&t.png);
            assert_eq!(&t.png[1..4], b"PNG");
            if bond == Bond::Wild {
                // Die ganze Verbandstabelle
                assert!(ganz(t.breite / (32.0 * a)) && ganz(t.hoehe / (128.0 * course)));
                assert!(pw.max(ph) <= 2048, "{pw} × {ph}");
            } else {
                assert!(ganz(t.breite / a) && ganz(t.hoehe / (12.0 * course)));
                assert!(pw.max(ph) <= 1024 && pw > 100, "{pw} × {ph}");
            }
        }
    }

    #[test]
    fn putz_ohne_naht() {
        let p = proctex::plaster_default();
        let (w, h) = (1000.0, 1000.0);
        // Gegenüberliegende Ränder tragen dieselbe Farbe
        for k in 0..20 {
            let v = k as f64 * 50.0;
            assert_eq!(
                blend(&p, [230, 225, 215], 0.0, v, w, h),
                blend(&p, [230, 225, 215], w, v, w, h)
            );
            assert_eq!(
                blend(&p, [230, 225, 215], v, 0.0, w, h),
                blend(&p, [230, 225, 215], v, h, w, h)
            );
        }
        assert!(auch_eben(&p));
        assert!(!auch_eben(&proctex::masonry_default()));
    }

    #[test]
    fn fremde_fassung_ohne_bild() {
        assert!(kachel(&Pattern::Foreign("gen=neu".into()), [0; 3]).is_none());
    }
}

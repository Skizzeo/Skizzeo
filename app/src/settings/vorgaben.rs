//! Firmenvorgaben dieses Arbeitsplatzes (Sonnenstand S8, Analyse §6
//! Frage 3): Schatten der Ansichten und Standardort, in `einstellungen.txt`
//! als `[ansichtsschatten] on=1 fill=area light=front-left` und
//! `[standardort] lat=53.0589 lon=8.591`. Geschrieben wird nur, was vom
//! Werk abweicht; ab Werk bleibt die Datei bytegleich.

use sk_math::sonne::Lage;
use sk_model::szo::{Line, Record};
use sk_model::ViewShade;

/// Die Vorgaben; [`Vorgaben::WERK`] ab Werk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vorgaben {
    /// Schatten jeder Ansicht ohne eigene Wahl (lebend, Darstellung).
    pub schatten: ViewShade,
    /// Bauort neuer Projekte.
    pub ort: Lage,
}

impl Default for Vorgaben {
    fn default() -> Self {
        Vorgaben::WERK
    }
}

impl Vorgaben {
    /// Werk: an, graue Fläche, vorne links; Ganderkesee.
    pub const WERK: Vorgaben = Vorgaben {
        schatten: ViewShade::WERK,
        ort: Lage::GANDERKESEE,
    };

    /// Die Zeilen der Datei, nur die vom Werk abweichenden.
    pub fn schreiben(&self) -> String {
        let mut out = String::new();
        let s = self.schatten;
        if s != ViewShade::WERK {
            Line::new("ansichtsschatten")
                .flag("on", s.on)
                .word("fill", s.fill_text())
                .word("light", s.light_text())
                .finish(&mut out);
        }
        if self.ort != Lage::GANDERKESEE {
            Line::new("standardort")
                .num("lat", self.ort.breite)
                .num("lon", self.ort.laenge)
                .finish(&mut out);
        }
        out
    }

    /// Liest die Vorgaben aus der Einstellungsdatei; Unlesbares gilt als
    /// Werk und gibt einen Hinweis, eine zweite Zeile zählt nicht.
    pub fn lesen(text: &str) -> (Vorgaben, Vec<String>) {
        let mut v = Vorgaben::WERK;
        let mut hints = Vec::new();
        let (mut schatten, mut ort) = (false, false);
        for (i, l) in text.lines().enumerate() {
            let Ok(Some(r)) = Record::parse(i + 1, l) else {
                continue;
            };
            let mut skip = |was: &str| {
                hints.push(format!(
                    "Zeile {}: [{}] {was}, es gilt Werk",
                    r.line, r.section
                ));
            };
            match r.section.as_str() {
                "ansichtsschatten" if schatten => skip("doppelt"),
                "ansichtsschatten" => {
                    schatten = true;
                    let on = r.opt("on").and_then(|t| match t {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    });
                    let hatch = r.opt("fill").and_then(ViewShade::parse_fill);
                    let light = r.opt("light").and_then(ViewShade::parse_light);
                    match (on, hatch, light) {
                        (Some(on), Some(hatch), Some(light)) => {
                            v.schatten = ViewShade { on, hatch, light }
                        }
                        _ => skip("unlesbar"),
                    }
                }
                "standardort" if ort => skip("doppelt"),
                "standardort" => {
                    ort = true;
                    let grad = |k: &str, max: f64| {
                        r.opt(k)?
                            .parse::<f64>()
                            .ok()
                            .filter(|x| x.is_finite() && x.abs() <= max)
                    };
                    match (grad("lat", 90.0), grad("lon", 180.0)) {
                        (Some(breite), Some(laenge)) => v.ort = Lage { breite, laenge },
                        _ => skip("unlesbar"),
                    }
                }
                _ => {}
            }
        }
        (v, hints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::ShadeLight;

    /// Ab Werk keine Zeile; Abweichungen im Rundlauf; Unlesbares und
    /// Doppeltes gilt als Werk mit Hinweis.
    #[test]
    fn vorgaben_in_der_datei() {
        assert_eq!(Vorgaben::WERK.schreiben(), "");
        assert_eq!(Vorgaben::lesen(""), (Vorgaben::WERK, Vec::new()));
        let v = Vorgaben {
            schatten: ViewShade {
                on: true,
                hatch: true,
                light: ShadeLight::Top,
            },
            ort: Lage {
                breite: 48.137,
                laenge: 11.575,
            },
        };
        let t = v.schreiben();
        assert_eq!(
            t,
            "[ansichtsschatten] on=1 fill=hatch light=top\n[standardort] lat=48.137 lon=11.575\n"
        );
        assert_eq!(Vorgaben::lesen(&t), (v, Vec::new()));
        // Nur der Ort weicht ab
        let nur_ort = Vorgaben {
            schatten: ViewShade::WERK,
            ..v
        };
        assert_eq!(nur_ort.schreiben(), "[standardort] lat=48.137 lon=11.575\n");
        for roh in [
            "[ansichtsschatten] on=ja fill=area light=sun",
            "[ansichtsschatten] on=1 fill=grau light=sun",
            "[ansichtsschatten] on=1 fill=area",
            "[standardort] lat=91 lon=8",
            "[standardort] lat=53",
        ] {
            let (w, h) = Vorgaben::lesen(roh);
            assert_eq!(w, Vorgaben::WERK, "{roh}");
            assert_eq!(h.len(), 1, "{roh}: {h:?}");
            assert!(h[0].contains("es gilt Werk"), "{h:?}");
        }
        let (w, h) = Vorgaben::lesen(&format!("{t}[ansichtsschatten] on=0 fill=area light=sun\n"));
        assert_eq!(w, v);
        assert_eq!(h.len(), 1, "{h:?}");
    }
}

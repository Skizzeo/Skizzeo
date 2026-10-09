//! Firmenvorgaben dieses Arbeitsplatzes (Sonnenstand S8, Analyse §6
//! Frage 3): Schatten der Ansichten und Standardort, in `einstellungen.txt`
//! als `[ansichtsschatten] on=1 fill=area light=front-left` und
//! `[standardort] lat=53.0589 lon=8.591`, dazu die H-Linie der
//! Schattenschraffur (S13) als `[hlinie] mm=0.05`. Geschrieben wird nur,
//! was vom Werk abweicht; ab Werk bleibt die Datei bytegleich.

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
    /// Strich der Schattenschraffur in den Ansichten (mm), eine der
    /// [`H_LINIEN`] (Jörn 09.10. 14:10, S13).
    pub h_linie: f32,
}

/// Wählbare Dicken der H-Linie (mm).
pub const H_LINIEN: [f32; 2] = [0.05, 0.08];

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
        h_linie: 0.08,
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
        if self.h_linie != Vorgaben::WERK.h_linie {
            Line::new("hlinie")
                .num("mm", (self.h_linie as f64 * 100.0).round() / 100.0)
                .finish(&mut out);
        }
        out
    }

    /// Liest die Vorgaben aus der Einstellungsdatei; es gilt die erste
    /// lesbare Zeile je Abschnitt, eine zweite lesbare zählt nicht.
    /// Unlesbares (etwa ein Licht einer späteren Fassung) gibt einen
    /// Hinweis und bleibt beim Speichern stehen (Review 3cd).
    pub fn lesen(text: &str) -> (Vorgaben, Vec<String>) {
        let mut v = Vorgaben::WERK;
        let mut hints = Vec::new();
        // Zeile der geltenden Vorgabe je Abschnitt
        let (mut schatten, mut ort, mut hl) = (None, None, None);
        let mut unlesbar = Vec::new();
        for (i, l) in text.lines().enumerate() {
            let Ok(Some(r)) = Record::parse(i + 1, l) else {
                continue;
            };
            let doppelt = |gilt: usize| {
                format!(
                    "Zeile {}: [{}] doppelt, es gilt Zeile {gilt}",
                    r.line, r.section
                )
            };
            match r.section.as_str() {
                "ansichtsschatten" => {
                    let on = r.opt("on").and_then(|t| match t {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    });
                    let hatch = r.opt("fill").and_then(ViewShade::parse_fill);
                    let light = r.opt("light").and_then(ViewShade::parse_light);
                    match (on, hatch, light) {
                        (Some(_), Some(_), Some(_)) if schatten.is_some() => {
                            hints.extend(schatten.map(doppelt))
                        }
                        (Some(on), Some(hatch), Some(light)) => {
                            v.schatten = ViewShade { on, hatch, light };
                            schatten = Some(r.line);
                        }
                        _ => unlesbar.push((r.line, "ansichtsschatten")),
                    }
                }
                "standardort" => {
                    let grad = |k: &str, max: f64| {
                        r.opt(k)?
                            .parse::<f64>()
                            .ok()
                            .filter(|x| x.is_finite() && x.abs() <= max)
                    };
                    match (grad("lat", 90.0), grad("lon", 180.0)) {
                        (Some(_), Some(_)) if ort.is_some() => hints.extend(ort.map(doppelt)),
                        (Some(breite), Some(laenge)) => {
                            v.ort = Lage { breite, laenge };
                            ort = Some(r.line);
                        }
                        _ => unlesbar.push((r.line, "standardort")),
                    }
                }
                "hlinie" => {
                    let mm = r.opt("mm").and_then(|t| t.parse::<f64>().ok());
                    let mm = H_LINIEN
                        .into_iter()
                        .find(|&h| mm.is_some_and(|m| (m - h as f64).abs() < 1e-6));
                    match mm {
                        Some(_) if hl.is_some() => hints.extend(hl.map(doppelt)),
                        Some(mm) => {
                            v.h_linie = mm;
                            hl = Some(r.line);
                        }
                        None => unlesbar.push((r.line, "hlinie")),
                    }
                }
                _ => {}
            }
        }
        for (zeile, abschnitt) in unlesbar {
            let gilt = match abschnitt {
                "standardort" => ort,
                "hlinie" => hl,
                _ => schatten,
            };
            hints.push(match gilt {
                Some(g) => format!("Zeile {zeile}: [{abschnitt}] unlesbar, es gilt Zeile {g}"),
                None => format!("Zeile {zeile}: [{abschnitt}] unlesbar, es gilt Werk"),
            });
        }
        (v, hints)
    }

    /// Ob ein Satz `[ansichtsschatten]`, `[standardort]` oder `[hlinie]`
    /// gelesen wird;
    /// ein unlesbarer bleibt als fremde Zeile in der Datei (Review 3cd).
    pub fn satz_lesbar(zeile: &str) -> bool {
        Vorgaben::lesen(zeile).1.is_empty()
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
            h_linie: 0.05,
        };
        let t = v.schreiben();
        assert_eq!(
            t,
            "[ansichtsschatten] on=1 fill=hatch light=top\n[standardort] lat=48.137 lon=11.575\n\
             [hlinie] mm=0.05\n"
        );
        assert_eq!(Vorgaben::lesen(&t), (v, Vec::new()));
        // Nur der Ort weicht ab
        let nur_ort = Vorgaben {
            schatten: ViewShade::WERK,
            h_linie: 0.08,
            ..v
        };
        assert_eq!(nur_ort.schreiben(), "[standardort] lat=48.137 lon=11.575\n");
        for roh in [
            "[ansichtsschatten] on=ja fill=area light=sun",
            "[ansichtsschatten] on=1 fill=grau light=sun",
            "[ansichtsschatten] on=1 fill=area",
            "[standardort] lat=91 lon=8",
            "[standardort] lat=53",
            "[hlinie] mm=0.13",
        ] {
            let (w, h) = Vorgaben::lesen(roh);
            assert_eq!(w, Vorgaben::WERK, "{roh}");
            assert_eq!(h.len(), 1, "{roh}: {h:?}");
            assert!(h[0].contains("es gilt Werk"), "{h:?}");
        }
        let (w, h) = Vorgaben::lesen(&format!("{t}[ansichtsschatten] on=0 fill=area light=sun\n"));
        assert_eq!(w, v);
        assert_eq!(h, ["Zeile 4: [ansichtsschatten] doppelt, es gilt Zeile 1"]);
        // Unlesbar vor der lesbaren: die lesbare gilt, gleich wo sie steht
        let (w, h) = Vorgaben::lesen(&format!("[standardort] lat=x lon=8\n{t}"));
        assert_eq!(w, v);
        assert_eq!(h, ["Zeile 1: [standardort] unlesbar, es gilt Zeile 3"]);
        assert!(Vorgaben::satz_lesbar("[hlinie] mm=0.08"));
        assert!(!Vorgaben::satz_lesbar("[hlinie] mm=0.1"));
        assert!(Vorgaben::satz_lesbar("[standardort] lat=53 lon=8"));
        assert!(!Vorgaben::satz_lesbar(
            "[ansichtsschatten] on=1 fill=area light=west"
        ));
    }
}

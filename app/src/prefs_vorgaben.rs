//! Reiter „Vorgaben“ des Einstellungsfensters (Sonnenstand S8): die
//! Firmenvorgaben dieses Arbeitsplatzes, Schatten der Ansichten und
//! Standardort ([`crate::settings::vorgaben`]). Die Wahl gilt ab
//! „Übernehmen“ bzw. „OK“; „Abbrechen“ verwirft sie.

use super::{label, FieldId, Prefs, Target, TextKind, UiText, Win, PAD, VALUE_X};
use crate::ansicht_schatten::{gewaehlt, text, waehlen, Teil};
use crate::settings::vorgaben::{Vorgaben, H_LINIEN};
use sk_math::sonne::Lage;
use sk_model::ShadeLight;
use sk_paint::Canvas;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Überschrift unter dem Reiternamen.
pub const UEBER: &str = "Firmenvorgaben für diesen Arbeitsplatz";
/// Leise Zeilen.
pub const GILT: &str = "Gilt für jede Ansicht ohne eigene Wahl (Zahnrad in der Ansicht).";
pub const OHNE_NORD: &str = "Projekte ohne Nordrichtung zeigen dann keinen Schatten.";
pub const HEUTE: &str = "Projekte ohne gespeicherten Sonnenstand rechnen mit dem heutigen Tag.";
pub const NEUE: &str = "Neue Projekte bekommen diesen Bauort. Ab Werk Ganderkesee.";
pub const H_LINIE: &str = "Strich der Schattenschraffur in den Ansichten. Ab Werk 0,08 mm.";

/// Knopftext einer H-Linie.
pub fn h_text(mm: f32) -> String {
    format!("{mm:.2} mm").replace('.', ",")
}

/// Knöpfe der Schattenwahl in drei Zeilen.
const ZEILEN: [(&str, &[Teil]); 3] = [
    ("Schatten", &[Teil::An(true), Teil::An(false)]),
    (
        "Darstellung",
        &[Teil::Schraffur(false), Teil::Schraffur(true)],
    ),
    (
        "Licht",
        &[
            Teil::Licht(ShadeLight::FrontLeft),
            Teil::Licht(ShadeLight::FrontRight),
            Teil::Licht(ShadeLight::Top),
            Teil::Licht(ShadeLight::Sun),
        ],
    ),
];

/// Lage im Reiter.
pub(super) struct Layout {
    pub items: Vec<(Rect, Target)>,
    pub texts: Vec<UiText>,
}

/// Neue Vorgaben nach einem Klick auf `t`.
pub fn klick(v: Vorgaben, t: Teil) -> Vorgaben {
    Vorgaben {
        schatten: waehlen(t, v.schatten),
        ..v
    }
}

/// Feldtext eines Grads, mit Komma.
pub fn grad_text(v: f64) -> String {
    v.to_string().replace('.', ",")
}

/// Liest Breite (`laenge = false`) oder Länge in den Ort `ort`; leer oder
/// außerhalb des Bereichs ist ein Fehler.
pub fn grad_setzen(ort: Lage, laenge: bool, text: &str) -> Result<Lage, String> {
    let (name, max) = if laenge {
        ("Länge", 180.0)
    } else {
        ("Breite", 90.0)
    };
    let t = text.trim();
    let t = t.strip_suffix('°').unwrap_or(t).trim_end();
    let v = t
        .replace(',', ".")
        .replace('−', "-")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= max)
        .ok_or_else(|| format!("{name}: erlaubt −{max} bis {max}°"))?;
    Ok(if laenge {
        Lage { laenge: v, ..ort }
    } else {
        Lage { breite: v, ..ort }
    })
}

impl Prefs {
    /// Die Vorgaben, wie der Reiter sie zeigt (ohne geladene: Werk).
    pub(super) fn vorgaben_jetzt(&self) -> Vorgaben {
        self.vorgaben.unwrap_or(Vorgaben::WERK)
    }

    pub(super) fn vorgaben_layout(&self, t: &Theme, w: &Win) -> Layout {
        let c = self.content(t, w);
        let s = w.scale;
        let (lx, vx) = (c.x, c.x + VALUE_X * s);
        let gap = 8.0 * s;
        let bw = ((c.w - VALUE_X * s - 3.0 * gap) / 4.0)
            .min(118.0 * s)
            .floor();
        let fh = t.size.field_height * s;
        let mut items = Vec::new();
        let mut texts = Vec::new();
        let mut y = 0.0;
        texts.push(UiText::dim(lx, c.y + y + 14.0 * s, UEBER));
        y += 30.0 * s;
        texts.push(UiText::heading(
            lx,
            c.y + y + 18.0 * s,
            "Schatten in den Ansichten",
        ));
        y += 34.0 * s;
        for (name, teile) in ZEILEN {
            texts.push(UiText::label(lx, c.y + y + 18.0 * s, name));
            for (i, teil) in teile.iter().enumerate() {
                let x = vx + i as f32 * (bw + gap);
                items.push((Rect::new(x, c.y + y, bw, fh), Target::Vorgabe(*teil)));
            }
            y += fh + 8.0 * s;
        }
        texts.push(UiText::label(lx, c.y + y + 18.0 * s, "H-Linie"));
        for (i, _) in H_LINIEN.iter().enumerate() {
            let x = vx + i as f32 * (bw + gap);
            items.push((Rect::new(x, c.y + y, bw, fh), Target::HLinie(i)));
        }
        y += fh + 8.0 * s;
        texts.push(UiText::dim(lx, c.y + y + 14.0 * s, GILT));
        y += 22.0 * s;
        texts.push(UiText::dim(lx, c.y + y + 14.0 * s, H_LINIE));
        y += 22.0 * s;
        if self.vorgaben_jetzt().schatten.light == ShadeLight::Sun {
            texts.push(UiText::dim(lx, c.y + y + 14.0 * s, OHNE_NORD));
            y += 22.0 * s;
            texts.push(UiText::dim(lx, c.y + y + 14.0 * s, HEUTE));
            y += 22.0 * s;
        }
        y += 16.0 * s;
        texts.push(UiText::heading(lx, c.y + y + 18.0 * s, "Standardort"));
        y += 34.0 * s;
        let fw = (120.0 * s).min(c.w - VALUE_X * s - PAD * s);
        for (name, f) in [
            ("Breite", FieldId::OrtBreite),
            ("Länge", FieldId::OrtLaenge),
        ] {
            texts.push(UiText::label(lx, c.y + y + 18.0 * s, name));
            items.push((Rect::new(vx, c.y + y, fw, fh), Target::Field(f)));
            y += fh + 8.0 * s;
        }
        texts.push(UiText::dim(lx, c.y + y + 14.0 * s, NEUE));
        Layout { items, texts }
    }

    /// Reiter malen (Fensterkoordinaten über `at` ins Bild).
    pub(super) fn paint_vorgaben(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let l = self.vorgaben_layout(t, w);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        for tx in &l.texts {
            let p = at(Rect::new(tx.x, tx.y, 0.0, 0.0));
            match tx.kind {
                TextKind::Heading => {
                    label(c, bold, &tx.text, t.size.font_title * s, p.x, p.y, u.text)
                }
                TextKind::Label => {
                    label(c, regular, &tx.text, t.size.font * s, p.x, p.y, u.text_dim)
                }
                _ => label(
                    c,
                    regular,
                    &tx.text,
                    t.size.font_small * s,
                    p.x,
                    p.y,
                    u.field_unit,
                ),
            }
        }
        let v = self.vorgaben_jetzt();
        for (r, tg) in &l.items {
            let rr = at(*r);
            let hover = self.hov(*tg, rr);
            match *tg {
                Target::Vorgabe(teil) => {
                    let st = ButtonState {
                        hover,
                        active: gewaehlt(teil, v.schatten),
                        ..Default::default()
                    };
                    widgets::button(c, fonts, rr, text(teil), st, s, t);
                }
                Target::HLinie(i) => {
                    let st = ButtonState {
                        hover,
                        active: v.h_linie == H_LINIEN[i],
                        ..Default::default()
                    };
                    widgets::button(c, fonts, rr, &h_text(H_LINIEN[i]), st, s, t);
                }
                Target::Field(f) => {
                    let val = self.vorgaben_feld(f);
                    let st = self.field_state(f, &val, "°", rr);
                    widgets::field(c, fonts, rr, &st, s, t);
                }
                _ => {}
            }
        }
    }

    /// Wert eines Ortsfelds außerhalb der Eingabe.
    pub(super) fn vorgaben_feld(&self, f: FieldId) -> String {
        let o = self.vorgaben_jetzt().ort;
        match f {
            FieldId::OrtBreite => grad_text(o.breite),
            FieldId::OrtLaenge => grad_text(o.laenge),
            _ => String::new(),
        }
    }

    /// Eingabe in ein Ortsfeld: gültig wirkt sie sofort im Reiter.
    pub(super) fn vorgaben_wert(&mut self, f: FieldId, text: &str) -> Result<(), String> {
        let mut v = self.vorgaben_jetzt();
        v.ort = grad_setzen(v.ort, f == FieldId::OrtLaenge, text)?;
        self.vorgaben = Some(v);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Knöpfe wählen wie im Zahnrad-Feld; Grad mit Komma, Bereich geprüft.
    #[test]
    fn vorgaben_knoepfe_und_grad() {
        let v = klick(Vorgaben::WERK, Teil::Licht(ShadeLight::Top));
        assert_eq!(v.schatten.light, ShadeLight::Top);
        assert_eq!(v.ort, Lage::GANDERKESEE);
        assert!(gewaehlt(Teil::Licht(ShadeLight::Top), v.schatten));
        assert!(gewaehlt(Teil::An(true), v.schatten));
        assert!(!gewaehlt(Teil::Schraffur(true), v.schatten));
        assert_eq!(grad_text(53.0589), "53,0589");
        let o = grad_setzen(Lage::GANDERKESEE, false, "48,137").unwrap();
        assert_eq!(o.breite, 48.137);
        assert_eq!(o.laenge, Lage::GANDERKESEE.laenge);
        let o = grad_setzen(o, true, " 11.575° ").unwrap();
        assert_eq!(o.laenge, 11.575);
        assert!(grad_setzen(o, false, "91").is_err());
        assert!(grad_setzen(o, true, "").is_err());
        assert!(grad_setzen(o, true, "-180").is_ok());
        assert_eq!(h_text(H_LINIEN[0]), "0,05 mm");
        assert_eq!(h_text(Vorgaben::WERK.h_linie), "0,08 mm");
    }

    /// „OK“ schreibt die Vorgaben in die Einstellungen, „Abbrechen“ nicht;
    /// ohne geladene Vorgaben bleibt die Einstellung, wie sie ist.
    #[test]
    fn ok_und_abbrechen() {
        use crate::scene::Scene;
        use crate::settings::Settings;
        let neu = |a: &[&str]| Settings::new(a.iter().map(|x| x.to_string()), None);
        let th = Theme::dark();
        let mut s = Scene::with_model(sk_model::Model::new());
        let anders = Vorgaben {
            schatten: sk_model::ViewShade {
                on: false,
                ..sk_model::ViewShade::WERK
            },
            ..Vorgaben::WERK
        };
        let mut st = neu(&["skizzeo"]);
        let mut p = Prefs::open(&mut s, &th).with_vorgaben(Vorgaben::WERK);
        p.show_tab(super::super::Tab::Vorgaben);
        p.vorgaben = Some(klick(Vorgaben::WERK, Teil::An(false)));
        let mut th2 = th.clone();
        p.cancel(&mut s, &mut th2);
        assert_eq!(st.vorgaben, Vorgaben::WERK);
        let mut p = Prefs::open(&mut s, &th).with_vorgaben(Vorgaben::WERK);
        p.vorgaben = Some(klick(Vorgaben::WERK, Teil::An(false)));
        assert!(p.vorgaben_wert(FieldId::OrtBreite, "abc").is_err());
        p.ok(&mut s, &th, &mut st);
        assert_eq!(st.vorgaben, anders);
        // Ohne geladene Vorgaben (Tests, ältere Aufrufer): bleibt
        let mut p = Prefs::open(&mut s, &th);
        p.ok(&mut s, &th, &mut st);
        assert_eq!(st.vorgaben, anders);
        // Zurücksetzen heißt Werk
        let mut p = Prefs::open(&mut s, &th).with_vorgaben(anders);
        let mut th3 = th.clone();
        p.reset_tab(&mut s, &mut th3, super::super::Tab::Vorgaben);
        p.ok(&mut s, &th, &mut st);
        assert_eq!(st.vorgaben, Vorgaben::WERK);
    }

    /// Ist-Bild des Reiters (nur mit SKIZZEO_ISTBILDER, Schrift aus
    /// SKIZZEO_SCHRIFT).
    #[test]
    fn istbild_reiter_vorgaben() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let mut fonts = Fonts::system();
        let schrift = || {
            std::env::var_os("SKIZZEO_SCHRIFT")
                .and_then(|p| std::fs::read(p).ok())
                .and_then(sk_paint::font::Font::parse)
        };
        if let (Some(a), Some(b)) = (schrift(), schrift()) {
            fonts.regular = Some(a);
            fonts.bold = Some(b);
        }
        let th = Theme::dark();
        let mut s = crate::scene::Scene::with_model(sk_model::Model::new());
        let v = klick(Vorgaben::WERK, Teil::Licht(ShadeLight::Sun));
        let mut p = Prefs::open(&mut s, &th).with_vorgaben(v);
        p.show_tab(super::super::Tab::Vorgaben);
        let w = Win {
            w: 1440,
            h: 900,
            top: 32,
            scale: 1.0,
        };
        let (c, _, _) = p.paint(&th, &fonts, &w, &s);
        std::fs::write(ziel.join("ist-s8-reiter-vorgaben.png"), c.to_png()).unwrap();
    }
}

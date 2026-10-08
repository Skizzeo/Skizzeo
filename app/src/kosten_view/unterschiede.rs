//! Abgleichzeile im Reiter Kosten und „Unterschiede ansehen“ (KA-3a5,
//! Regel 92): welche Stammsätze von der Firma abweichen, welche eigenen
//! Werte dieses Hauses bleiben und wie viele gleich sind.

use super::*;

/// Lage der Abgleichzeile (px).
pub(super) struct AbgleichLage {
    /// x des Texts und Grundlinie.
    pub x: f32,
    pub base: f32,
    /// Text, gekürzt.
    pub text: String,
    pub text_r: Rect,
    pub ansehen: Rect,
    pub uebernehmen: Rect,
    pub lassen: Rect,
}

const ANSEHEN: &str = "Unterschiede ansehen";
const UEBERNEHMEN: &str = "übernehmen";
const LASSEN: &str = "so lassen";

/// Blatt unter der Zeile (dip).
const PAD: f32 = 14.0;
const ZEILE: f32 = 18.0;
const KOPF: f32 = 24.0;
const BREITE: f32 = 520.0;

/// Eine Zeile im Blatt „Unterschiede“.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Zeile {
    Titel(String, String),
    Kopf(&'static str, &'static str),
    Eintrag(String),
    Fuss(String),
}

impl Zeile {
    fn hoehe(&self) -> f32 {
        match self {
            Zeile::Titel(..) => KOPF,
            Zeile::Kopf(..) => KOPF,
            Zeile::Eintrag(_) | Zeile::Fuss(_) => ZEILE,
        }
    }
}

impl KostenView {
    /// Abgleichzeile: „● Für neue Häuser gilt … · Unterschiede ansehen ·
    /// übernehmen · so lassen“.
    pub(super) fn abgleich_lage(&self, t: &Theme, fonts: &Fonts) -> Option<AbgleichLage> {
        let (a, _) = self.abgleich.as_ref()?;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let px = 11.0 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let breite = |f: Option<&sk_paint::font::Font>, text: &str| {
            f.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            })
        };
        let base = self.top_px() + ABGLEICH_Y * s;
        let x = x0 + 12.0 * s;
        let sep = breite(regular, " · ");
        let wa = breite(bold, ANSEHEN);
        let (wu, wl) = (breite(bold, UEBERNEHMEN), breite(bold, LASSEN));
        let platz = (x0 + cw - x - 3.0 * sep - wa - wu - wl).max(0.0);
        let text = sk_ui::widgets::ellipsize(regular, &a.zeile(), px, platz);
        let wt = breite(regular, &text);
        let (y, h) = (base - 14.0 * s, ABGLEICH_H * s);
        let an = x + wt + sep;
        let u = an + wa + sep;
        let l = u + wu + sep;
        Some(AbgleichLage {
            x,
            base,
            text,
            text_r: (x, y, wt, h),
            ansehen: (an, y, wa, h),
            uebernehmen: (u, y, wu, h),
            lassen: (l, y, wl, h),
        })
    }

    pub(super) fn paint_abgleich(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some(lage) = self.abgleich_lage(t, fonts) else {
            return;
        };
        let (s, u) = (self.scale, &t.ui);
        let px = 11.0 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (x0, _) = self.content_x(t);
        let base = lage.base;
        let cap = regular.map_or(8.0 * s, |f| f.cap_height(px));
        let mut p = Path::new();
        let d = 3.0 * s;
        p.rounded_rect(x0, base - cap * 0.5 - d, 2.0 * d, 2.0 * d, d);
        c.fill(&p, u.accent);
        if let Some(f) = regular {
            f.draw(c, &lage.text, px, lage.x, base, u.sheet_text);
            let sep = f.width(" · ", px);
            for r in [lage.ansehen, lage.uebernehmen, lage.lassen] {
                f.draw(c, " · ", px, r.0 - sep, base, u.sheet_text_dim);
            }
        }
        if let Some(f) = bold {
            let unter = |h| self.hot == Some(h);
            let ansehen = unter(Hot::Ansehen) || self.unterschiede;
            f.draw(
                c,
                ANSEHEN,
                px,
                lage.ansehen.0,
                base,
                crate::cards::verweis(u, ansehen),
            );
            f.draw(
                c,
                UEBERNEHMEN,
                px,
                lage.uebernehmen.0,
                base,
                crate::cards::verweis(u, unter(Hot::Uebernehmen)),
            );
            let col = if unter(Hot::Lassen) {
                u.text
            } else {
                u.sheet_text_dim
            };
            f.draw(c, LASSEN, px, lage.lassen.0, base, col);
        }
    }

    /// Inhalt des Blatts: abweichend (geht mit „übernehmen“), eigene Werte
    /// dieses Hauses (bleiben), Zahl der gleichen Einträge.
    pub(super) fn unterschiede_zeilen(&self) -> Vec<Zeile> {
        let Some((a, _)) = self.abgleich.as_ref() else {
            return Vec::new();
        };
        let mut z = vec![Zeile::Titel(
            "Unterschiede zum Firmenkatalog".into(),
            format!("Stand {}", a.stand),
        )];
        z.push(Zeile::Kopf(
            "Abweichend",
            "„übernehmen“ setzt den Wert für neue Häuser",
        ));
        z.extend(a.texte.iter().cloned().map(Zeile::Eintrag));
        if !a.eigene.is_empty() {
            z.push(Zeile::Kopf(
                "Eigener Wert dieses Hauses",
                "bleibt auch beim Übernehmen",
            ));
            z.extend(a.eigene.iter().cloned().map(Zeile::Eintrag));
        }
        z.push(Zeile::Fuss(match a.gleich {
            1 => "1 Eintrag gleich".into(),
            n => format!("{} Einträge gleich", tausender(&n.to_string())),
        }));
        z
    }

    /// Blatt unter „Unterschiede ansehen“ (px) und die Zeilen, die
    /// hineinpassen; was nicht passt, zählt „… und n weitere“.
    pub(super) fn unterschiede_lage(&self, t: &Theme, fonts: &Fonts) -> Option<(Rect, Vec<Zeile>)> {
        if !self.unterschiede {
            return None;
        }
        let lage = self.abgleich_lage(t, fonts)?;
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let w = (BREITE * s).min(cw);
        let x = (lage.ansehen.0 - 24.0 * s).clamp(x0, (x0 + cw - w).max(x0));
        let y = lage.ansehen.1 + lage.ansehen.3 + 8.0 * s;
        let platz = self.h as f32 - BOTTOM_PAD * s - y - 2.0 * PAD * s;
        let mut alle = self.unterschiede_zeilen();
        let fuss = alle.pop();
        let mut zeilen = Vec::new();
        let mut h = 0.0;
        for (i, z) in alle.iter().enumerate() {
            // Platz für diese Zeile, „… und n weitere“ und den Fuß
            if (h + z.hoehe() + 2.0 * ZEILE) * s > platz {
                let rest = alle[i..]
                    .iter()
                    .filter(|z| matches!(z, Zeile::Eintrag(_)))
                    .count();
                if rest > 0 {
                    zeilen.push(Zeile::Eintrag(format!("… und {rest} weitere")));
                    h += ZEILE;
                }
                break;
            }
            h += z.hoehe();
            zeilen.push(z.clone());
        }
        if let Some(f) = fuss {
            h += f.hoehe() + 6.0;
            zeilen.push(f);
        }
        Some(((x, y, w, (h + 2.0 * PAD - 6.0) * s), zeilen))
    }

    /// Ist das Blatt „Unterschiede“ offen (Esc schließt es)?
    pub fn unterschiede_offen(&self) -> bool {
        self.unterschiede
    }

    /// Liegt (x, y) auf dem offenen Blatt?
    pub(super) fn auf_unterschieden(&self, t: &Theme, fonts: &Fonts, x: f32, y: f32) -> bool {
        self.unterschiede_lage(t, fonts)
            .is_some_and(|(r, _)| inside(r, x, y))
    }

    pub(super) fn paint_unterschiede(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some(((x, y, w, h), zeilen)) = self.unterschiede_lage(t, fonts) else {
            return;
        };
        let Some(lage) = self.abgleich_lage(t, fonts) else {
            return;
        };
        let (s, u) = (self.scale, &t.ui);
        crate::preis_blatt::flaeche(c, (x, y, w, h), lage.ansehen, true, s, t);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (Some(f), Some(fb)) = (regular, bold) else {
            return;
        };
        let (xl, xr) = (x + PAD * s, x + w - PAD * s);
        let mut top = y + PAD * s - 6.0 * s;
        let px = 11.0 * s;
        let pxk = 10.5 * s;
        let kuerzen = |f: &sk_paint::font::Font, text: &str, px: f32, breite: f32| {
            sk_ui::widgets::ellipsize(Some(f), text, px, breite)
        };
        for z in &zeilen {
            let hz = z.hoehe() * s;
            let base = (top + hz - 6.0 * s).round();
            match z {
                Zeile::Titel(titel, stand) => {
                    let pxt = 12.0 * s;
                    fb.draw(c, titel, pxt, xl, base, u.sheet_text);
                    let ws = f.width(stand, pxk);
                    f.draw(c, stand, pxk, xr - ws, base, u.sheet_text_dim);
                }
                Zeile::Kopf(kopf, was) => {
                    fb.draw(c, kopf, pxk, xl, base, u.sheet_text);
                    let wk = fb.width(kopf, pxk);
                    let rest = format!(" · {was}");
                    let rest = kuerzen(f, &rest, pxk, xr - xl - wk);
                    f.draw(c, &rest, pxk, xl + wk, base, u.sheet_text_dim);
                }
                Zeile::Eintrag(text) => {
                    let text = kuerzen(f, text, px, xr - xl - 8.0 * s);
                    f.draw(c, &text, px, xl + 8.0 * s, base, u.sheet_text);
                }
                Zeile::Fuss(text) => {
                    top += 6.0 * s;
                    let base = base + 6.0 * s;
                    c.fill_rect(xl, top - 2.0 * s, xr - xl, s.max(1.0), u.sheet_rule);
                    f.draw(c, text, pxk, xl, base, u.sheet_text_dim);
                }
            }
            top += hz;
        }
    }
}

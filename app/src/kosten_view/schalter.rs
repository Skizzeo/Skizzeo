//! Schalterzeile „Preise … Gliedern …“ (Notiz Kopf §3): „Gliedern“ steht
//! bei `x0 + GLIEDERN_X` wie im Mengenblatt. Reicht die Breite dafür nicht,
//! rutscht er linksbündig unter „Preise“, und der Kopf wird um diese Zeile
//! höher (Befund Q).

use super::*;

/// Abstand der zweiten Schalterzeile von der ersten (dip).
const ZEILE2: f32 = SWITCH_H + 6.0;

impl KostenView {
    /// Segmentschalter: Beschriftung (x, Grundlinie) und Segmente. Merkt
    /// sich, ob „Gliedern“ unter „Preise“ steht ([`KostenView::zeile2`]).
    #[allow(clippy::type_complexity)]
    pub(super) fn schalter(
        &self,
        t: &Theme,
        fonts: &Fonts,
    ) -> ((f32, Vec<(Modus, Rect)>), (f32, Vec<(Gliederung, Rect)>)) {
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let px = 10.5 * s;
        let y = self.top_px() + (SWITCH_TOP + self.ab()) * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let w = |text: &str| {
            bold.map_or(text.chars().count() as f32 * px * 0.6, |f| {
                f.width(text, px)
            }) + 20.0 * s
        };
        let label_w = |text: &str| {
            regular.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            }) + 8.0 * s
        };
        let inset = 2.0 * s;
        let mut x = x0 + label_w("Preise") + inset;
        let mut preise = Vec::new();
        for m in Modus::ALLE {
            let sw = w(m.label());
            preise.push((m, (x, y + inset, sw, SWITCH_H * s - 2.0 * inset)));
            x += sw;
        }
        let gx = (x0 + GLIEDERN_X * s).max(x + 24.0 * s);
        let breite = label_w("Gliedern")
            + inset
            + Gliederung::ALLE.iter().map(|g| w(g.label())).sum::<f32>();
        let unten = gx + breite > x0 + cw + 0.5;
        self.gliedern_unten.set(unten);
        let (gx, y) = if unten { (x0, y + ZEILE2 * s) } else { (gx, y) };
        let mut x = gx + label_w("Gliedern") + inset;
        let mut gl = Vec::new();
        for g in Gliederung::ALLE {
            let sw = w(g.label());
            gl.push((g, (x, y + inset, sw, SWITCH_H * s - 2.0 * inset)));
            x += sw;
        }
        ((x0, preise), (gx, gl))
    }

    /// Höhe der zweiten Schalterzeile (dip), 0, solange „Gliedern“ neben
    /// „Preise“ passt. Gilt ab dem Bild, das [`KostenView::schalter`]
    /// zuletzt ausgelegt hat.
    pub(super) fn zeile2(&self) -> f32 {
        if self.gliedern_unten.get() {
            ZEILE2
        } else {
            0.0
        }
    }
}

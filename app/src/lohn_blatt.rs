//! Feld Verrechnungslohn (KA-2c, paket-ka2 §4, Einstellungen §3 KA-2
//! Punkt 5, Bedienbarkeit 2.2, 2.5, 4.1, 4.7) in zwei Formen:
//!
//! - **Hinweiskarte** beim ersten Öffnen der Kosten: dunkel unten rechts,
//!   Titel „Kosten mit Referenzpreisen 10/2026“, das Feld, darunter leise
//!   „gilt für dieses und alle neuen Häuser“, unten rechts „Übernehmen“
//!   (wie Enter: Projekt und Firma) und leise „Nur dieses Haus“.
//! - **Blatt** am Stundenlohn der Kachel Lohnanteil: im Stil des
//!   Preisblatts mit „Gilt für“ und der Folgezeile.
//!
//! Das Blatt liest nur die Zahl; geschrieben wird über die Kostenansicht
//! (`FirmenwertSetzen { wage }`, `Scene::fuer_firma` bzw. `kosten_folge`).

use crate::preis_blatt::{self, flaeche, inside, Gilt, Rect, SPITZE};
use sk_cost::Dez;
use sk_paint::{Canvas, Path};
use sk_platform::{Key, Modifiers};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts};

const PAD: f32 = 16.0;
const FELD_W: f32 = 104.0;
const FELD_H: f32 = 26.0;
const SEG_H: f32 = 26.0;
const KARTE_W: f32 = 330.0;
const BLATT_W: f32 = 360.0;

/// Form des Felds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Karte,
    Blatt,
}

/// Ergebnis an die Kostenansicht.
#[derive(Clone, Debug, PartialEq)]
pub enum Aus {
    Repaint,
    Schliessen,
    /// Lohn schreiben: für dieses Haus oder auch für neue Häuser.
    Schreiben {
        wert: Dez,
        gilt: Gilt,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ziel {
    Schliessen,
    Feld,
    Segment(Gilt),
    Uebernehmen,
    NurHaus,
    Innen,
}

pub struct LohnBlatt {
    pub form: Form,
    titel: String,
    /// Lohn dieses Hauses und der Firma (für die Folgezeile).
    jetzt: Dez,
    firma: Dez,
    edit: TextEdit,
    wert: Option<Dez>,
    pub gilt: Gilt,
    hot: Option<Ziel>,
    /// Stundenlohn in der Kachel (px), nur für das Blatt.
    anker: Rect,
    pub scale: f32,
    pub fenster: (f32, f32),
}

/// „Kosten mit Referenzpreisen 10/2026“ aus der Preisquelle
/// („Referenzpreise 10/2026“, „Preise Firmenkatalog vom …“).
pub fn kartentitel(preisquelle: &str) -> String {
    let q = preisquelle.trim();
    for (a, b) in [("Referenzpreise", "Referenzpreisen"), ("Preise", "Preisen")] {
        if let Some(rest) = q.strip_prefix(a) {
            return format!("Kosten mit {b}{rest}");
        }
    }
    format!("Kosten mit {q}")
}

impl LohnBlatt {
    pub fn neu(form: Form, titel: String, jetzt: Dez, firma: Dez) -> LohnBlatt {
        let mut edit = TextEdit::new(&preis_blatt::zahl(jetzt, 2));
        edit.select_all();
        LohnBlatt {
            form,
            titel,
            jetzt,
            firma,
            edit,
            wert: Some(jetzt),
            // Die Karte schreibt mit Enter Projekt und Firma (Bedienbarkeit
            // 4.1); das Blatt beginnt wie das Preisblatt bei diesem Haus
            gilt: match form {
                Form::Karte => Gilt::NeueHaeuser,
                Form::Blatt => Gilt::NurHaus,
            },
            hot: None,
            anker: (0.0, 0.0, 0.0, 0.0),
            scale: 1.0,
            fenster: (0.0, 0.0),
        }
    }

    pub fn set_anker(&mut self, r: Rect) {
        self.anker = r;
    }

    fn hoehe(&self) -> f32 {
        let fehler = if self.wert.is_none() { 18.0 } else { 0.0 };
        match self.form {
            Form::Karte => PAD + 24.0 + FELD_H + 22.0 + fehler + 34.0 + PAD,
            Form::Blatt => {
                let folge = if self.gilt == Gilt::NeueHaeuser {
                    32.0
                } else {
                    16.0
                };
                PAD + 24.0 + FELD_H + 14.0 + fehler + SEG_H + 12.0 + folge + PAD
            }
        }
    }

    fn rect(&self) -> (Rect, bool) {
        let s = self.scale;
        let (fw, fh) = self.fenster;
        let h = self.hoehe() * s;
        match self.form {
            Form::Karte => {
                let w = KARTE_W * s;
                ((fw - w - 24.0 * s, fh - h - 24.0 * s, w, h), false)
            }
            Form::Blatt => {
                let w = BLATT_W * s;
                let (ax0, ay0, ax1, ay1) = self.anker;
                let rand = 8.0 * s;
                let x = ((ax0 + ax1) * 0.5 - w * 0.5).min(fw - w - rand).max(rand);
                // Die Kachel steht unten: das Blatt darüber, sonst darunter
                let oben = ay0 - SPITZE * s - h >= rand;
                let y = if oben {
                    ay0 - SPITZE * s - h
                } else {
                    ay1 + SPITZE * s
                };
                ((x, y, w, h), !oben)
            }
        }
    }

    fn feld_rect(&self) -> Rect {
        let s = self.scale;
        let ((x, y, w, _), _) = self.rect();
        (
            x + w - (PAD + FELD_W) * s,
            y + (PAD + 24.0) * s,
            FELD_W * s,
            FELD_H * s,
        )
    }

    fn schliessen_rect(&self) -> Rect {
        let s = self.scale;
        let ((x, y, w, _), _) = self.rect();
        (
            x + w - (PAD + 16.0) * s,
            y + (PAD - 4.0) * s,
            20.0 * s,
            20.0 * s,
        )
    }

    fn fehler_h(&self) -> f32 {
        if self.wert.is_none() {
            18.0
        } else {
            0.0
        }
    }

    /// Segmente „Nur dieses Haus | Auch für neue Häuser“ (nur das Blatt).
    fn segmente(&self, fonts: &Fonts) -> Vec<(Gilt, Rect)> {
        if self.form != Form::Blatt {
            return Vec::new();
        }
        let s = self.scale;
        let ((x, y, _, _), _) = self.rect();
        let px = 10.5 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let label_w = regular.map_or(50.0 * s, |f| f.width("Gilt für", px)) + 12.0 * s;
        let sy = y + (PAD + 24.0 + FELD_H + 14.0 + self.fehler_h()) * s;
        let mut sx = x + PAD * s + label_w;
        let mut out = Vec::new();
        for g in Gilt::ALLE {
            let w = bold.map_or(110.0 * s, |f| f.width(g.label(), px)) + 20.0 * s;
            out.push((g, (sx, sy + 2.0 * s, w, (SEG_H - 4.0) * s)));
            sx += w;
        }
        out
    }

    /// „Nur dieses Haus“ und „Übernehmen“ (nur die Karte).
    fn knoepfe(&self, fonts: &Fonts) -> Option<(Rect, Rect)> {
        if self.form != Form::Karte {
            return None;
        }
        let s = self.scale;
        let ((x, y, w, h), _) = self.rect();
        let px = 11.0 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let wu = bold.map_or(80.0 * s, |f| f.width("Übernehmen", px)) + 24.0 * s;
        let wn = regular.map_or(100.0 * s, |f| f.width("Nur dieses Haus", px)) + 16.0 * s;
        let bh = 26.0 * s;
        let by = y + h - (PAD * s) - bh;
        let ux = x + w - PAD * s - wu;
        Some(((ux - 8.0 * s - wn, by, wn, bh), (ux, by, wu, bh)))
    }

    fn hit(&self, fonts: &Fonts, x: f32, y: f32) -> Option<Ziel> {
        if !inside(self.rect().0, x, y) {
            return None;
        }
        if inside(self.schliessen_rect(), x, y) {
            return Some(Ziel::Schliessen);
        }
        if inside(self.feld_rect(), x, y) {
            return Some(Ziel::Feld);
        }
        if let Some((g, _)) = self
            .segmente(fonts)
            .into_iter()
            .find(|(_, r)| inside(*r, x, y))
        {
            return Some(Ziel::Segment(g));
        }
        if let Some((n, u)) = self.knoepfe(fonts) {
            if inside(u, x, y) {
                return Some(Ziel::Uebernehmen);
            }
            if inside(n, x, y) {
                return Some(Ziel::NurHaus);
            }
        }
        Some(Ziel::Innen)
    }

    pub fn enthaelt(&self, x: f32, y: f32) -> bool {
        inside(self.rect().0, x, y)
    }

    // --- Ereignisse ----------------------------------------------------------

    /// Schreiben mit `gilt`; ein ungültiges Feld hält offen, ein
    /// unveränderter Wert schließt ohne Spur.
    fn schreiben(&self, gilt: Gilt) -> Option<Aus> {
        let wert = self.wert?;
        Some(
            if wert == self.jetzt && (gilt == Gilt::NurHaus || wert == self.firma) {
                Aus::Schliessen
            } else {
                Aus::Schreiben { wert, gilt }
            },
        )
    }

    pub fn mouse_move(&mut self, fonts: &Fonts, x: f32, y: f32) -> bool {
        let hot = self.hit(fonts, x, y);
        let look = |h: Option<Ziel>| h.filter(|z| *z != Ziel::Innen);
        let changed = look(hot) != look(self.hot);
        self.hot = hot;
        changed
    }

    /// Klick (px). Daneben: das Blatt schreibt wie Enter, die Karte bleibt
    /// stehen (sie geht nur mit ×, Esc oder einem der Knöpfe).
    pub fn mouse_down(&mut self, fonts: &Fonts, x: f32, y: f32) -> Option<Aus> {
        match self.hit(fonts, x, y) {
            None => match self.form {
                Form::Blatt => self.schreiben(self.gilt).or(Some(Aus::Repaint)),
                Form::Karte => None,
            },
            Some(Ziel::Schliessen) => Some(Aus::Schliessen),
            Some(Ziel::Feld) => {
                let s = self.scale;
                let (fx, _, fw, _) = self.feld_rect();
                let px = 11.0 * s;
                let f = fonts.regular.as_ref();
                let unit_w = f.map_or(0.0, |f| f.width("€/h", 10.0 * s));
                let text_w = f.map_or(0.0, |f| f.width(&self.edit.text, px));
                let tx = fx + fw - 8.0 * s - unit_w - 4.0 * s - text_w;
                let c = widgets::caret_at(f, &self.edit.text, px, tx, x);
                self.edit.place(c, false);
                Some(Aus::Repaint)
            }
            Some(Ziel::Segment(g)) => (g != self.gilt).then(|| {
                self.gilt = g;
                Aus::Repaint
            }),
            Some(Ziel::Uebernehmen) => self.schreiben(Gilt::NeueHaeuser),
            Some(Ziel::NurHaus) => self.schreiben(Gilt::NurHaus),
            Some(Ziel::Innen) => None,
        }
    }

    fn geaendert(&mut self) -> Option<Aus> {
        self.wert = preis_blatt::lesen(&self.edit.text);
        Some(Aus::Repaint)
    }

    pub fn key(&mut self, key: Key, mods: Modifiers) -> Option<Aus> {
        let sh = mods.shift;
        let e = &mut self.edit;
        let changed = match key {
            Key::Escape => return Some(Aus::Schliessen),
            Key::Enter => return self.schreiben(self.gilt).or(Some(Aus::Repaint)),
            Key::Backspace => {
                e.backspace();
                true
            }
            Key::Delete => {
                e.delete();
                true
            }
            Key::Left => {
                e.left(sh);
                false
            }
            Key::Right => {
                e.right(sh);
                false
            }
            Key::Home => {
                e.home(sh);
                false
            }
            Key::End => {
                e.end(sh);
                false
            }
            Key::Char('A') if mods.ctrl => {
                e.select_all();
                false
            }
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                e.insert(paste.lines().next().unwrap_or("").trim());
                true
            }
            Key::Char('Z') if mods.ctrl => e.undo(),
            _ => return None,
        };
        if changed {
            self.geaendert()
        } else {
            Some(Aus::Repaint)
        }
    }

    pub fn text(&mut self, ch: char) -> Option<Aus> {
        if !(ch.is_ascii_digit() || matches!(ch, ',' | '.')) {
            return None;
        }
        self.edit.insert(ch.encode_utf8(&mut [0; 4]));
        self.geaendert()
    }

    pub fn tip_at(&self, fonts: &Fonts, x: f32, y: f32) -> Option<String> {
        match self.hit(fonts, x, y)? {
            Ziel::Segment(Gilt::NeueHaeuser) | Ziel::Uebernehmen => {
                Some("Auch für neue Häuser: speichert im Firmenkatalog".into())
            }
            _ => None,
        }
    }

    /// Bezeichnung des Schritts („Lohn 65,00 €/h für dieses und neue
    /// Häuser“, „Lohn 65,00 €/h nur für dieses Haus“).
    pub fn bezeichnung(wert: Dez, gilt: Gilt) -> String {
        let w = preis_blatt::zahl(wert, 2);
        match gilt {
            Gilt::NeueHaeuser => format!("Lohn {w} €/h{}", preis_blatt::FUER_NEUE),
            Gilt::NurHaus => format!("Lohn {w} €/h nur für dieses Haus"),
        }
    }

    // --- Zeichnen ------------------------------------------------------------

    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let ((x, y, w, h), unten) = self.rect();
        let karte = self.form == Form::Karte;
        // Farben: dunkle Karte wie die Hinweiskarten, sonst das helle Blatt
        let (text, dim, feld_bg) = if karte {
            widgets::panel_filled(c, widgets::Rect::new(x, y, w, h), s, t, u.menu_bg);
            (u.text, u.text_dim, u.field)
        } else {
            flaeche(c, (x, y, w, h), self.anker, unten, s, t);
            (u.sheet_text, u.sheet_text_dim, u.sheet_card)
        };
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (Some(f), Some(fb)) = (regular, bold) else {
            return;
        };
        let x0 = x + PAD * s;
        let x1 = x + w - PAD * s;
        // Titel und ×
        let px = 12.0 * s;
        let tb = (y + PAD * s + fb.cap_height(px)).round();
        let titel = widgets::ellipsize(Some(fb), &self.titel, px, x1 - x0 - 28.0 * s);
        fb.draw(c, &titel, px, x0, tb, text);
        let (cx, cy, cw, ch) = self.schliessen_rect();
        let col = if self.hot == Some(Ziel::Schliessen) {
            text
        } else {
            dim
        };
        let (mx, my, d) = (cx + cw * 0.5, cy + ch * 0.5, 4.0 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
        c.fill(&p, col);
        // Feld mit Beschriftung
        let (fx, fy, fw, fh) = self.feld_rect();
        let px_e = 11.0 * s;
        let base = (fy + (fh + f.cap_height(px_e)) * 0.5).round();
        f.draw(c, "Verrechnungslohn", px_e, x0, base, dim);
        let rcol = if self.wert.is_none() {
            u.field_invalid
        } else {
            u.accent
        };
        let mut p = Path::new();
        p.rounded_rect(fx, fy, fw, fh, 4.0 * s);
        c.fill(&p, rcol);
        let b = 1.5 * s;
        let mut p = Path::new();
        p.rounded_rect(fx + b, fy + b, fw - 2.0 * b, fh - 2.0 * b, 4.0 * s - b);
        c.fill(&p, feld_bg);
        let px_u = 10.0 * s;
        let ux = fx + fw - 8.0 * s - f.width("€/h", px_u);
        f.draw(c, "€/h", px_u, ux, base, dim);
        let tx = ux - 4.0 * s - f.width(&self.edit.text, px_e);
        let (a, bsel) = self.edit.selection();
        if a < bsel {
            let sx = tx + f.width(&self.edit.text[..a], px_e);
            let sw = f.width(&self.edit.text[a..bsel], px_e);
            c.fill_rect(sx, fy + 5.0 * s, sw, fh - 10.0 * s, u.text_select);
        }
        f.draw(c, &self.edit.text, px_e, tx, base, text);
        let kx = (tx + f.width(&self.edit.text[..self.edit.caret], px_e)).round();
        c.fill_rect(kx, fy + 6.0 * s, s.max(1.0), fh - 12.0 * s, text);
        let px_s = 10.5 * s;
        let mut zy = fy + fh;
        if self.wert.is_none() {
            zy += 16.0 * s;
            f.draw(
                c,
                "Bitte eine Zahl ab 0 eingeben, etwa 65,00.",
                px_s,
                x0,
                zy.round(),
                u.field_invalid,
            );
        }
        match self.form {
            Form::Karte => {
                let t2 = "gilt für dieses und alle neuen Häuser";
                f.draw(
                    c,
                    t2,
                    px_s,
                    fx + fw - f.width(t2, px_s),
                    (zy + 16.0 * s).round(),
                    dim,
                );
                if let Some(((nx, ny, nw, nh), (bx, by, bw, bh))) = self.knoepfe(fonts) {
                    if self.hot == Some(Ziel::NurHaus) {
                        let mut p = Path::new();
                        p.rounded_rect(nx, ny, nw, nh, t.size.corner_radius * s);
                        c.fill(&p, u.hover);
                    }
                    let nb = (ny + (nh + f.cap_height(px_e)) * 0.5).round();
                    f.draw(c, "Nur dieses Haus", px_e, nx + 8.0 * s, nb, dim);
                    let mut p = Path::new();
                    p.rounded_rect(bx, by, bw, bh, t.size.corner_radius * s);
                    c.fill(
                        &p,
                        if self.hot == Some(Ziel::Uebernehmen) {
                            u.accent_hover
                        } else {
                            u.accent
                        },
                    );
                    let ub = (by + (bh + fb.cap_height(px_e)) * 0.5).round();
                    fb.draw(c, "Übernehmen", px_e, bx + 12.0 * s, ub, u.on_accent);
                }
            }
            Form::Blatt => {
                let segs = self.segmente(fonts);
                if let (Some(&(_, first)), Some(&(_, last))) = (segs.first(), segs.last()) {
                    let inset = 2.0 * s;
                    let (sx, sy, sh) = (first.0 - inset, first.1 - inset, first.3 + 2.0 * inset);
                    let sw = last.0 + last.2 + inset - sx;
                    let mut p = Path::new();
                    p.rounded_rect(sx, sy, sw, sh, 6.0 * s);
                    c.fill(&p, u.sheet_tile);
                    let sb = (sy + (sh + f.cap_height(px_s)) * 0.5).round();
                    f.draw(c, "Gilt für", px_s, x0, sb, dim);
                    for &(g, (rx, ry, rw, rh)) in &segs {
                        let (font, col) = if g == self.gilt {
                            let mut p = Path::new();
                            p.rounded_rect(rx, ry, rw, rh, 5.0 * s);
                            c.fill(&p, u.sheet_rule);
                            let mut p = Path::new();
                            let b = s.max(1.0);
                            p.rounded_rect(rx + b, ry + b, rw - 2.0 * b, rh - 2.0 * b, 5.0 * s - b);
                            c.fill(&p, u.sheet_card);
                            (fb, text)
                        } else if self.hot == Some(Ziel::Segment(g)) {
                            (f, text)
                        } else {
                            (f, dim)
                        };
                        let tw = font.width(g.label(), px_s);
                        let gb = (ry + (rh + font.cap_height(px_s)) * 0.5).round();
                        font.draw(c, g.label(), px_s, rx + (rw - tw) * 0.5, gb, col);
                    }
                    let fy2 = (sy + sh + 12.0 * s + f.cap_height(px_s)).round();
                    match self.gilt {
                        Gilt::NurHaus => {
                            let t2 = format!(
                                "Neue Häuser rechnen weiter mit {} €/h.",
                                preis_blatt::zahl(self.firma, 2)
                            );
                            f.draw(c, &t2, px_s, x0, fy2, dim);
                        }
                        Gilt::NeueHaeuser => {
                            let neu = self.wert.unwrap_or(self.jetzt);
                            let t2 = format!(
                                "Neue Häuser rechnen dann mit {} €/h.",
                                preis_blatt::zahl(neu, 2)
                            );
                            fb.draw(c, &t2, px_s, x0, fy2, u.accent);
                            f.draw(
                                c,
                                "Strg+Z nimmt es nur für dieses Haus zurück.",
                                px_s,
                                x0,
                                fy2 + 16.0 * s,
                                dim,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leer() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    #[test]
    fn titel_der_karte() {
        assert_eq!(
            kartentitel("Referenzpreise 10/2026"),
            "Kosten mit Referenzpreisen 10/2026"
        );
        assert_eq!(
            kartentitel("Preise Firmenkatalog vom 08.10.2026"),
            "Kosten mit Preisen Firmenkatalog vom 08.10.2026"
        );
    }

    /// Karte: Tippen, Enter schreibt für dieses und neue Häuser, „Nur dieses
    /// Haus“ nur das Projekt; ungültig hält offen; Esc schließt.
    #[test]
    fn karte_enter_und_knoepfe() {
        let mut k = LohnBlatt::neu(Form::Karte, "Kosten".into(), Dez::ganz(60), Dez::ganz(60));
        k.fenster = (1200.0, 900.0);
        let mods = Modifiers::default();
        for ch in "65".chars() {
            k.text(ch);
        }
        assert_eq!(
            k.key(Key::Enter, mods),
            Some(Aus::Schreiben {
                wert: Dez::ganz(65),
                gilt: Gilt::NeueHaeuser
            })
        );
        let fonts = leer();
        let (n, u) = k.knoepfe(&fonts).unwrap();
        assert_eq!(
            k.mouse_down(&fonts, n.0 + 2.0, n.1 + 2.0),
            Some(Aus::Schreiben {
                wert: Dez::ganz(65),
                gilt: Gilt::NurHaus
            })
        );
        assert!(matches!(
            k.mouse_down(&fonts, u.0 + 2.0, u.1 + 2.0),
            Some(Aus::Schreiben { .. })
        ));
        assert_eq!(k.mouse_down(&fonts, 5.0, 5.0), None, "Karte bleibt");
        k.text(',');
        k.text(',');
        assert_eq!(k.key(Key::Enter, mods), Some(Aus::Repaint), "ungültig");
        assert_eq!(k.key(Key::Escape, mods), Some(Aus::Schliessen));
        assert_eq!(
            LohnBlatt::bezeichnung(Dez::ganz(65), Gilt::NeueHaeuser),
            "Lohn 65,00 €/h für dieses und neue Häuser"
        );
    }

    /// Blatt: beginnt bei „Nur dieses Haus“, Klick daneben schreibt, ein
    /// unveränderter Wert schließt ohne Spur.
    #[test]
    fn blatt_daneben_und_unveraendert() {
        let mut b = LohnBlatt::neu(Form::Blatt, "Lohn".into(), Dez::ganz(60), Dez::ganz(60));
        b.fenster = (1200.0, 900.0);
        b.set_anker((500.0, 800.0, 560.0, 816.0));
        let fonts = leer();
        assert_eq!(b.mouse_down(&fonts, 5.0, 5.0), Some(Aus::Schliessen));
        b.key(Key::Backspace, Modifiers::default());
        for ch in "62,5".chars() {
            b.text(ch);
        }
        assert_eq!(
            b.mouse_down(&fonts, 5.0, 5.0),
            Some(Aus::Schreiben {
                wert: Dez::lesen("62.5", 4).unwrap(),
                gilt: Gilt::NurHaus
            })
        );
    }
}

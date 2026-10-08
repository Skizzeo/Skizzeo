//! Kopfzeile des Blatts AVA (KA-4c, paket-ka4 §4): „Kopf und
//! Vorbemerkungen“ zum Aufklappen mit den Feldern Bauvorhaben, Bauherr und
//! Aufsteller, „Bauherr fehlt“ bzw. „Aufsteller fehlt“ und „Mehr“ ›
//! „Geschosse als Untertitel“.

use super::*;
use crate::kosten_view::Schreiben;
use sk_platform::{Key, Modifiers};
use sk_ui::widgets;

/// Höhe des aufgeklappten Kopfs (dip).
const KOPF_H: f32 = 164.0;
const ZEILE: f32 = 28.0;
const FELD_X: f32 = 110.0;
const FELD_W: f32 = 300.0;
const FELD_H: f32 = 22.0;
/// Innenabstand des Texts im Feld.
const FELD_PAD: f32 = 8.0;
const RECHTS_X: f32 = 460.0;
/// Längster Text in einem Kopffeld.
const MAX_ZEICHEN: usize = 200;

pub const KOPF: &str = "Kopf und Vorbemerkungen";
pub const MEHR: &str = "Mehr";
pub const UNTERTITEL: &str = "Geschosse als Untertitel";

/// Feld im Kopf; schreibt `Project.site/client/author`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feld {
    Bauvorhaben,
    Bauherr,
    Aufsteller,
}

impl Feld {
    pub const ALLE: [Feld; 3] = [Feld::Bauvorhaben, Feld::Bauherr, Feld::Aufsteller];

    pub fn label(self) -> &'static str {
        match self {
            Feld::Bauvorhaben => "Bauvorhaben",
            Feld::Bauherr => "Bauherr",
            Feld::Aufsteller => "Aufsteller",
        }
    }

    /// Bezeichnung des Rückgängig-Schritts.
    fn schritt(self) -> &'static str {
        match self {
            Feld::Bauvorhaben => "Bauvorhaben gesetzt",
            Feld::Bauherr => "Bauherr gesetzt",
            Feld::Aufsteller => "Aufsteller gesetzt",
        }
    }

    fn wert(self, p: &sk_model::Project) -> &str {
        match self {
            Feld::Bauvorhaben => &p.site,
            Feld::Bauherr => &p.client,
            Feld::Aufsteller => &p.author,
        }
    }

    fn setzen(self, p: &mut sk_model::Project, v: String) {
        match self {
            Feld::Bauvorhaben => p.site = v,
            Feld::Bauherr => p.client = v,
            Feld::Aufsteller => p.author = v,
        }
    }

    fn naechstes(self) -> Feld {
        match self {
            Feld::Bauvorhaben => Feld::Bauherr,
            Feld::Bauherr => Feld::Aufsteller,
            Feld::Aufsteller => Feld::Bauvorhaben,
        }
    }
}

/// Lage der Kopfzeile: Klappknopf, „Bauherr fehlt“ (wenn er fehlt) und
/// „Mehr“.
struct Zeile {
    kopf: Rect,
    fehlt: Option<Rect>,
    mehr: Rect,
}

impl AvaView {
    /// Zusätzliche Höhe des aufgeklappten Kopfs (dip).
    pub(super) fn kopf_h(&self) -> f32 {
        if self.kopf_offen {
            KOPF_H
        } else {
            0.0
        }
    }

    /// Bauvorhaben wie im Kopf: `Project.site`, sonst aus dem Dateinamen.
    pub(super) fn bauvorhaben(&self, lv: &Lv) -> String {
        match lv
            .kopf
            .bauvorhaben
            .as_deref()
            .filter(|b| !b.trim().is_empty())
        {
            Some(b) => b.to_string(),
            None => sk_cost::lv::bauvorhaben_aus_datei(&self.datei),
        }
    }

    pub(super) fn bauherr_fehlt(&self) -> bool {
        self.projekt
            .as_ref()
            .is_some_and(|p| p.client.trim().is_empty())
    }

    /// Das Kopffeld hinter dem Verweis „… fehlt“: zuerst der Bauherr, dann
    /// der Aufsteller (weder Verfasser noch Firmenkatalog, Kosten A1).
    pub(super) fn fehlt(&self) -> Option<Feld> {
        if self.bauherr_fehlt() {
            return Some(Feld::Bauherr);
        }
        let lv = self.lv.as_deref()?;
        (self.projekt.is_some() && lv.kopf.aufsteller.is_none()).then_some(Feld::Aufsteller)
    }

    /// „Bauherr fehlt“ bzw. „Aufsteller fehlt“.
    pub(super) fn fehlt_text(&self) -> Option<String> {
        self.fehlt().map(|f| format!("{} fehlt", f.label()))
    }

    /// Zeile „Preise“ im Kopf, wie LV-Art, Währung und Preisquelle in der
    /// CSV: „mit Preisen · EUR netto zzgl. MwSt. · Referenzpreise 10/2026,
    /// unverbindlich“.
    pub(super) fn preise_text(&self) -> String {
        let Some(lv) = self.lv.as_deref() else {
            return String::new();
        };
        let k = &lv.kopf;
        let mut t = format!("{} · {}, {}", k.art, k.waehrung, k.netto);
        if self.preise && !self.preisquelle.is_empty() {
            t.push_str(" · ");
            t.push_str(&self.preisquelle);
            if self.preisquelle.starts_with("Referenzpreise") {
                t.push_str(", unverbindlich");
            }
        }
        t
    }

    /// Ein Feld im Kopf nimmt Tasten und Zeichen.
    pub fn feld_offen(&self) -> bool {
        self.feld.is_some()
    }

    fn zeile(&self, t: &Theme, regular: &Font, bold: &Font) -> Zeile {
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let y = self.top_px() + SWITCH_TOP * s;
        let h = SWITCH_H * s;
        let px = 10.5 * s;
        let kw = 14.0 * s + bold.width(KOPF, px) + 4.0 * s;
        let kopf = (x0, y, kw, h);
        let fehlt = self.fehlt_text().map(|f| {
            let fw = bold.width(&f, 10.0 * s) + 8.0 * s;
            (x0 + kw + 20.0 * s, y, fw, h)
        });
        let (lx, _) = self.schalter_mit(t, regular, bold);
        let mw = bold.width(MEHR, px) + 14.0 * s;
        let mehr = (lx - 28.0 * s - mw, y, mw, h);
        Zeile { kopf, fehlt, mehr }
    }

    /// Karte unter „Mehr“ mit der Zeile „Geschosse als Untertitel“.
    fn mehr_karte(&self, z: &Zeile, regular: &Font) -> Rect {
        let s = self.scale;
        let w = 44.0 * s + regular.width(UNTERTITEL, 10.5 * s) + 16.0 * s;
        let (mx, my, mw, mh) = z.mehr;
        (mx + mw - w, my + mh + 6.0 * s, w, 36.0 * s)
    }

    /// Felder des aufgeklappten Kopfs (px).
    fn felder(&self, t: &Theme) -> Vec<(Feld, Rect)> {
        if !self.kopf_offen {
            return Vec::new();
        }
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let y0 = self.top_px() + (SWITCH_TOP + SWITCH_H + 12.0) * s;
        Feld::ALLE
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let y = y0 + i as f32 * ZEILE * s;
                (*f, (x0 + FELD_X * s, y, FELD_W * s, FELD_H * s))
            })
            .collect()
    }

    /// `Some(hot)`: der Kopf hat getroffen (auch `Some(None)` über der
    /// offenen Karte „Mehr“, die alles darunter verdeckt).
    pub(super) fn kopf_hit(&self, t: &Theme, fonts: &Fonts, x: f32, y: f32) -> Option<Option<Hot>> {
        let regular = fonts.regular.as_ref()?;
        let bold = fonts.bold.as_ref().unwrap_or(regular);
        let z = self.zeile(t, regular, bold);
        if self.mehr_offen {
            let k = self.mehr_karte(&z, regular);
            if inside(k, x, y) {
                return Some(Some(Hot::Untertitel));
            }
        }
        if inside(z.kopf, x, y) {
            return Some(Some(Hot::Kopf));
        }
        if z.fehlt.is_some_and(|r| inside(r, x, y)) {
            return Some(Some(Hot::BauherrFehlt));
        }
        if inside(z.mehr, x, y) {
            return Some(Some(Hot::Mehr));
        }
        self.felder(t)
            .into_iter()
            .find(|(_, r)| inside(*r, x, y))
            .map(|(f, _)| Some(Hot::Feld(f)))
    }

    /// Klick auf den Kopf; `None`, wenn `hot` nicht zum Kopf gehört.
    pub(super) fn kopf_klick(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        hot: Option<Hot>,
        x: f64,
    ) -> Option<ListOut> {
        match hot? {
            Hot::Kopf => {
                self.kopf_offen = !self.kopf_offen;
                if !self.kopf_offen {
                    self.feld = None;
                }
                self.clamp();
                Some(ListOut::Repaint)
            }
            Hot::BauherrFehlt => {
                self.kopf_offen = true;
                self.feld_oeffnen(self.fehlt().unwrap_or(Feld::Bauherr));
                self.clamp();
                Some(ListOut::Repaint)
            }
            Hot::Mehr => {
                self.mehr_offen = !self.mehr_offen;
                Some(ListOut::Repaint)
            }
            Hot::Untertitel => {
                self.mehr_offen = false;
                Some(ListOut::Kosten(Schreiben::Gliederung(!self.untertitel)))
            }
            Hot::Feld(f) => {
                if self.feld.as_ref().is_none_or(|(g, _)| *g != f) {
                    self.feld_oeffnen(f);
                } else if let Some(r) = fonts.regular.as_ref() {
                    // Schreibmarke an die Klickstelle
                    let (_, rect) = self.felder(t).into_iter().find(|(g, _)| *g == f)?;
                    let tx = rect.0 + FELD_PAD * self.scale;
                    let px = 10.5 * self.scale;
                    let (_, e) = self.feld.as_mut()?;
                    let i = widgets::caret_at(Some(r), &e.text, px, tx, x as f32);
                    e.place(i, false);
                }
                Some(ListOut::Repaint)
            }
            _ => None,
        }
    }

    pub(super) fn feld_oeffnen(&mut self, f: Feld) {
        let wert = self.projekt.as_ref().map_or("", |p| f.wert(p));
        self.feld = Some((f, TextEdit::new(wert)));
    }

    /// Schließt das Feld; mit `schreiben` und geändertem Text der Schritt.
    pub(super) fn feld_schliessen(&mut self, schreiben: bool) -> Option<ListOut> {
        let (f, e) = self.feld.take()?;
        if !schreiben {
            return Some(ListOut::Repaint);
        }
        let mut p = self.projekt.clone()?;
        let neu: String = e.text.trim().chars().take(MAX_ZEICHEN).collect();
        if neu == f.wert(&p) {
            return Some(ListOut::Repaint);
        }
        f.setzen(&mut p, neu);
        // Gleich zeigen, auch bevor das Modell zurückkommt
        self.projekt = Some(p.clone());
        Some(ListOut::Kosten(Schreiben::Projekt {
            projekt: p,
            label: f.schritt(),
        }))
    }

    /// Tasten im Kopffeld: Enter schreibt, Esc verwirft, Tab schreibt und
    /// geht zum nächsten Feld.
    pub fn key(&mut self, key: Key, mods: Modifiers) -> Option<ListOut> {
        let (f, _) = self.feld.as_ref()?;
        let f = *f;
        let sh = mods.shift;
        match key {
            Key::Enter => return self.feld_schliessen(true),
            Key::Escape => return self.feld_schliessen(false),
            Key::Tab => {
                let out = self.feld_schliessen(true);
                self.feld_oeffnen(f.naechstes());
                return out;
            }
            _ => {}
        }
        let (_, e) = self.feld.as_mut()?;
        match key {
            Key::Backspace => e.backspace(),
            Key::Delete => e.delete(),
            Key::Left => e.left(sh),
            Key::Right => e.right(sh),
            Key::Home => e.home(sh),
            Key::End => e.end(sh),
            Key::Char('A') if mods.ctrl => e.select_all(),
            Key::Char('V') if mods.ctrl => {
                let paste = sk_platform::clipboard_text().unwrap_or_default();
                e.insert(paste.lines().next().unwrap_or("").trim());
            }
            Key::Char('Z') if mods.ctrl => {
                e.undo();
            }
            _ => return None,
        }
        Some(ListOut::Repaint)
    }

    pub fn text(&mut self, ch: char) -> Option<ListOut> {
        let (_, e) = self.feld.as_mut()?;
        if ch.is_control() || e.text.chars().count() >= MAX_ZEICHEN {
            return None;
        }
        e.insert(ch.encode_utf8(&mut [0; 4]));
        Some(ListOut::Repaint)
    }

    pub(super) fn paint_kopf(&self, c: &mut Canvas, t: &Theme, regular: &Font, bold: &Font) {
        let s = self.scale;
        let u = &t.ui;
        let px = 10.5 * s;
        let z = self.zeile(t, regular, bold);
        let mitte = |r: Rect, f: &Font, px: f32| r.1 + (r.3 + f.cap_height(px)) * 0.5;
        // Klappknopf
        widgets::disclosure(
            c,
            z.kopf.0 + 4.0 * s,
            z.kopf.1 + z.kopf.3 * 0.5,
            self.kopf_offen,
            u.sheet_text_dim,
            s,
        );
        bold.draw(
            c,
            KOPF,
            px,
            z.kopf.0 + 14.0 * s,
            mitte(z.kopf, bold, px),
            u.sheet_text,
        );
        if self.hot == Some(Hot::Kopf) {
            let w = bold.width(KOPF, px);
            let y = mitte(z.kopf, bold, px) + 2.0 * s;
            c.fill_rect(z.kopf.0 + 14.0 * s, y, w, s.max(1.0), u.sheet_text_dim);
        }
        if let (Some(r), Some(f)) = (z.fehlt, self.fehlt_text()) {
            bold.draw(c, &f, 10.0 * s, r.0, mitte(r, bold, 10.0 * s), u.accent);
            if self.hot == Some(Hot::BauherrFehlt) {
                let w = bold.width(&f, 10.0 * s);
                let y = mitte(r, bold, 10.0 * s) + 2.0 * s;
                c.fill_rect(r.0, y, w, s.max(1.0), u.accent);
            }
        }
        // „Mehr ▸“ bzw. offen „Mehr ▾“
        let (mx, _, _, _) = z.mehr;
        bold.draw(c, MEHR, px, mx, mitte(z.mehr, bold, px), u.accent);
        let w = bold.width(MEHR, px);
        widgets::disclosure(
            c,
            mx + w + 7.0 * s,
            z.mehr.1 + z.mehr.3 * 0.5,
            self.mehr_offen,
            u.accent,
            s,
        );
        if !self.kopf_offen {
            return;
        }
        let lv = self.lv.as_deref();
        let (x0, cw) = self.content_x(t);
        let lpx = 10.5 * s;
        for (f, r) in self.felder(t) {
            let base = mitte(r, regular, lpx);
            regular.draw(c, f.label(), lpx, x0, base, u.sheet_text_dim);
            let edit = self.feld.as_ref().filter(|(g, _)| *g == f).map(|(_, e)| e);
            // Feld im Blatt: weiß mit Rand, im Eingabemodus Rand im Akzent
            let rand = if edit.is_some() {
                u.accent
            } else if self.hot == Some(Hot::Feld(f)) {
                u.sheet_text_dim
            } else {
                u.sheet_rule
            };
            let mut p = Path::new();
            p.rounded_rect(r.0, r.1, r.2, r.3, 4.0 * s);
            c.fill(&p, rand);
            let b = if edit.is_some() { 1.5 * s } else { s.max(1.0) };
            let mut p = Path::new();
            p.rounded_rect(r.0 + b, r.1 + b, r.2 - 2.0 * b, r.3 - 2.0 * b, 4.0 * s - b);
            c.fill(&p, u.sheet_card);
            let tx = r.0 + FELD_PAD * s;
            let breit = r.2 - 2.0 * FELD_PAD * s;
            match edit {
                Some(e) => {
                    let (a, bis) = e.selection();
                    if a != bis {
                        let sx = tx + regular.width(&e.text[..a], lpx);
                        let sw = regular.width(&e.text[a..bis], lpx);
                        c.fill_rect(sx, r.1 + 4.0 * s, sw, r.3 - 8.0 * s, u.sheet_select);
                    }
                    regular.draw(c, &e.text, lpx, tx, base, u.sheet_text);
                    let kx = (tx + regular.width(&e.text[..e.caret], lpx)).round();
                    c.fill_rect(kx, r.1 + 4.0 * s, s.max(1.0), r.3 - 8.0 * s, u.sheet_text);
                }
                None => {
                    let wert = self.projekt.as_ref().map_or("", |p| f.wert(p));
                    // Vorbelegung blass im leeren Feld
                    let (text, col) = if wert.is_empty() {
                        match (f, lv) {
                            (Feld::Bauvorhaben, Some(lv)) => (self.bauvorhaben(lv), u.sheet_hint),
                            (Feld::Aufsteller, Some(lv)) => match lv.kopf.aufsteller.clone() {
                                Some(a) => (a, u.sheet_hint),
                                None => ("fehlt".to_string(), u.accent),
                            },
                            (Feld::Bauherr, _) => ("fehlt".to_string(), u.accent),
                            _ => (String::new(), u.sheet_hint),
                        }
                    } else {
                        (wert.to_string(), u.sheet_text)
                    };
                    let text = widgets::ellipsize(Some(regular), &text, lpx, breit);
                    regular.draw(c, &text, lpx, tx, base, col);
                }
            }
        }
        // Rechts: Los, Umfang, Stand und Preise wie in der CSV (Kosten B4);
        // darunter die Vorbemerkungen
        let y0 = self.top_px() + (SWITCH_TOP + SWITCH_H + 12.0) * s;
        let rx = x0 + RECHTS_X * s;
        let (umfang, stand) = &self.kopf_umfang;
        let los = lv.map_or(String::new(), |l| format!("Los {}", l.kopf.los));
        let preise = self.preise_text();
        for (i, (k, v)) in [
            ("Los", los.as_str()),
            ("Umfang", umfang),
            ("Stand", stand),
            ("Preise", preise.as_str()),
        ]
        .iter()
        .enumerate()
        {
            let r = (rx, y0 + i as f32 * ZEILE * s, 0.0, FELD_H * s);
            let base = mitte(r, regular, lpx);
            regular.draw(c, k, lpx, rx, base, u.sheet_text_dim);
            let v = widgets::ellipsize(Some(regular), v, lpx, x0 + cw - rx - 80.0 * s);
            regular.draw(c, &v, lpx, rx + 80.0 * s, base, u.sheet_text);
        }
        let y = y0 + 4.0 * ZEILE * s + 4.0 * s;
        let r = (x0, y, 0.0, FELD_H * s);
        let base = mitte(r, regular, lpx);
        regular.draw(c, "Vorbemerkungen", lpx, x0, base, u.sheet_text_dim);
        let text = lv
            .and_then(|l| l.kopf.vorbemerkungen.clone())
            .unwrap_or_else(|| "keine (am Los nicht hinterlegt)".into());
        let breit = cw - FELD_X * s;
        let zeilen = widgets::wrap(Some(regular), &text.replace('\n', " "), lpx, breit);
        for (i, l) in zeilen.iter().take(2).enumerate() {
            let l = if i == 1 && zeilen.len() > 2 {
                widgets::ellipsize(Some(regular), &format!("{l} …"), lpx, breit)
            } else {
                l.clone()
            };
            regular.draw(
                c,
                &l,
                lpx,
                x0 + FELD_X * s,
                base + i as f32 * 16.0 * s,
                u.sheet_text,
            );
        }
    }

    /// Offene Karte „Mehr“ über allem.
    pub(super) fn paint_mehr(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        if !self.mehr_offen {
            return;
        }
        let Some(regular) = fonts.regular.as_ref() else {
            return;
        };
        let bold = fonts.bold.as_ref().unwrap_or(regular);
        let s = self.scale;
        let u = &t.ui;
        let z = self.zeile(t, regular, bold);
        let (x, y, w, h) = self.mehr_karte(&z, regular);
        let mut p = Path::new();
        p.rounded_rect(x - s, y - s, w + 2.0 * s, h + 2.0 * s, 7.0 * s);
        c.fill(&p, u.sheet_rule);
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, 6.0 * s);
        let fl = if self.hot == Some(Hot::Untertitel) {
            u.sheet_tile
        } else {
            u.sheet_card
        };
        c.fill(&p, fl);
        let k = 14.0 * s;
        widgets::checkbox(
            c,
            widgets::Rect::new(x + 14.0 * s, y + (h - k) * 0.5, k, k),
            self.untertitel,
            self.hot == Some(Hot::Untertitel),
            s,
            t,
        );
        let px = 10.5 * s;
        regular.draw(
            c,
            UNTERTITEL,
            px,
            x + 38.0 * s,
            y + (h + regular.cap_height(px)) * 0.5,
            u.sheet_text,
        );
    }
}

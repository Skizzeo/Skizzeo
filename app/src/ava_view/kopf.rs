//! Kopfzeile des Blatts AVA (KA-4c, paket-ka4 §4; Paket PD-3): „Kopf und
//! Vorbemerkungen“ zum Aufklappen. Der Kopf zeigt die Projektdaten nur an;
//! ein Klick auf eine Zeile, „Projektdaten …“ oder „Bauherr fehlt“ öffnet
//! die Maske „Projektdaten“ im passenden Feld. Dazu „Mehr“ › „Geschosse als
//! Untertitel“.

use super::*;
use sk_ui::widgets;

/// Höhe des aufgeklappten Kopfs (dip): zwei Spalten, im schmalen Blatt
/// untereinander.
const ZEILE: f32 = 28.0;
const FELD_X: f32 = 110.0;
const FELD_W: f32 = 300.0;
const FELD_H: f32 = 22.0;
const RECHTS_X: f32 = 460.0;
/// Ab dieser Inhaltsbreite (dip) stehen Los, Umfang, Stand und Preise
/// rechts neben den Projektdaten.
const ZWEI_SPALTEN: f32 = RECHTS_X + 260.0;
/// Vorbemerkungen: zwei Zeilen und Luft.
const VOR_H: f32 = 52.0;

pub const KOPF: &str = "Kopf und Vorbemerkungen";
pub const MEHR: &str = "Mehr";
pub const UNTERTITEL: &str = "Geschosse als Untertitel";
pub const PROJEKTDATEN: &str = "Projektdaten …";

/// Zeile im Kopf; ein Klick öffnet die Maske in [`Feld::maske_feld`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feld {
    Bauvorhaben,
    /// Projektart · Bauort.
    Projekt,
    Projektnummer,
    Bauherr,
    Aufsteller,
}

impl Feld {
    pub const ALLE: [Feld; 5] = [
        Feld::Bauvorhaben,
        Feld::Projekt,
        Feld::Projektnummer,
        Feld::Bauherr,
        Feld::Aufsteller,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Feld::Bauvorhaben => "Bauvorhaben",
            Feld::Projekt => "Projekt",
            Feld::Projektnummer => "Projekt-Nr.",
            Feld::Bauherr => "Bauherr",
            Feld::Aufsteller => "Aufsteller",
        }
    }

    /// Feld der Maske „Projektdaten“ (kind, site, place, projno, client,
    /// clientaddr, author, authoraddr).
    pub fn maske_feld(self) -> usize {
        match self {
            Feld::Bauvorhaben => 1,
            Feld::Projekt => 0,
            Feld::Projektnummer => 3,
            Feld::Bauherr => 4,
            Feld::Aufsteller => 6,
        }
    }
}

/// Mehrzeiliges in einer Zeile mit „, “.
fn einzeilig(t: &str) -> String {
    t.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Lage der Kopfzeile: Klappknopf, „Bauherr fehlt“ (wenn er fehlt),
/// „Projektdaten …“ und „Mehr“.
struct Zeile {
    kopf: Rect,
    fehlt: Option<Rect>,
    projektdaten: Option<Rect>,
    mehr: Rect,
}

impl AvaView {
    /// Inhaltsbreite (dip) mit dem Rand aus `tick`.
    fn kopf_breite(&self) -> f32 {
        self.w as f32 / self.scale - 2.0 * self.rand
    }

    /// Los, Umfang, Stand und Preise rechts neben den Projektdaten?
    fn kopf_zwei_spalten(&self) -> bool {
        self.kopf_breite() >= ZWEI_SPALTEN
    }

    /// Zusätzliche Höhe des aufgeklappten Kopfs (dip).
    pub(super) fn kopf_h(&self) -> f32 {
        if !self.kopf_offen {
            return 0.0;
        }
        let links = Feld::ALLE.len() as f32;
        let zeilen = if self.kopf_zwei_spalten() {
            links
        } else {
            links + 4.0
        };
        12.0 + zeilen * ZEILE + VOR_H
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

    /// Die Zeile hinter dem Verweis „… fehlt“: zuerst der Bauherr, dann
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

    /// Text einer Zeile im Kopf und ob er fehlt bzw. nur vorbelegt ist:
    /// `(text, Ton)` mit Ton 0 = Wert, 1 = blass, 2 = „fehlt“.
    fn kopf_wert(&self, f: Feld) -> (String, u8) {
        let Some(p) = self.projekt.as_ref() else {
            return (String::new(), 1);
        };
        let lv = self.lv.as_deref();
        let mit = |name: &str, anschrift: &str| {
            let a = einzeilig(anschrift);
            if a.is_empty() {
                name.to_string()
            } else {
                format!("{name}, {a}")
            }
        };
        match f {
            Feld::Bauvorhaben if !p.site.trim().is_empty() => (einzeilig(&p.site), 0),
            Feld::Bauvorhaben => (lv.map_or(String::new(), |l| self.bauvorhaben(l)), 1),
            Feld::Projekt => {
                let teile: Vec<String> = [einzeilig(&p.kind), einzeilig(&p.place)]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect();
                if teile.is_empty() {
                    ("nicht angegeben".into(), 1)
                } else {
                    (teile.join(" · "), 0)
                }
            }
            Feld::Projektnummer if p.number.trim().is_empty() => ("nicht angegeben".into(), 1),
            Feld::Projektnummer => (einzeilig(&p.number), 0),
            Feld::Bauherr if p.client.trim().is_empty() => ("fehlt".into(), 2),
            Feld::Bauherr => (mit(p.client.trim(), &p.client_addr), 0),
            Feld::Aufsteller if !p.author.trim().is_empty() => {
                (mit(p.author.trim(), &p.author_addr), 0)
            }
            // Ohne Verfasser der Name eines echten Firmenkatalogs, blass
            Feld::Aufsteller => match lv.and_then(|l| l.kopf.aufsteller.clone()) {
                Some(a) => (a, 1),
                None => ("fehlt".into(), 2),
            },
        }
    }

    fn zeile(&self, t: &Theme, regular: &Font, bold: &Font) -> Zeile {
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let y = self.top_px() + (SWITCH_TOP + self.kopfzeile_dy()) * s;
        let h = SWITCH_H * s;
        let px = 10.5 * s;
        let kw = 14.0 * s + bold.width(KOPF, px) + 4.0 * s;
        let kopf = (x0, y, kw, h);
        let mut x = x0 + kw + 20.0 * s;
        let fehlt = self.fehlt_text().map(|f| {
            let fw = bold.width(&f, 10.0 * s) + 8.0 * s;
            let r = (x, y, fw, h);
            x += fw + 12.0 * s;
            r
        });
        let pw = bold.width(PROJEKTDATEN, 10.0 * s) + 8.0 * s;
        // „Mehr“ vor dem Schalter, im schmalen Blatt am rechten Rand
        let mw = bold.width(MEHR, px) + 14.0 * s;
        let mehr_x = if self.kopfzeile_dy() > 0.0 {
            self.tabelle_x(t).1 - mw
        } else {
            self.schalter_mit(t, regular, bold).0 - 28.0 * s - mw
        };
        let mehr = (mehr_x, y, mw, h);
        // Reicht der Platz nicht, entfällt „Projektdaten …“ (der Knopf im
        // Hauptfenster und die Zeilen des Kopfs führen ebenso hin)
        let projektdaten = (x + pw + 12.0 * s <= mehr_x).then_some((x, y, pw, h));
        Zeile {
            kopf,
            fehlt,
            projektdaten,
            mehr,
        }
    }

    /// Karte unter „Mehr“ mit der Zeile „Geschosse als Untertitel“.
    fn mehr_karte(&self, z: &Zeile, regular: &Font) -> Rect {
        let s = self.scale;
        let w = 44.0 * s + regular.width(UNTERTITEL, 10.5 * s) + 16.0 * s;
        let (mx, my, mw, mh) = z.mehr;
        (mx + mw - w, my + mh + 6.0 * s, w, 36.0 * s)
    }

    /// Oberkante der ersten Kopfzeile (px).
    fn kopf_y0(&self) -> f32 {
        self.top_px() + (SWITCH_TOP + self.kopfzeile_dy() + SWITCH_H + 12.0) * self.scale
    }

    /// Zeilen des aufgeklappten Kopfs (px), über Bezeichnung und Wert.
    fn felder(&self, t: &Theme) -> Vec<(Feld, Rect)> {
        if !self.kopf_offen {
            return Vec::new();
        }
        let s = self.scale;
        let (x0, cw) = self.content_x(t);
        let w = if self.kopf_zwei_spalten() {
            (FELD_X + FELD_W) * s
        } else {
            cw
        };
        let y0 = self.kopf_y0();
        Feld::ALLE
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let y = y0 + i as f32 * ZEILE * s;
                (*f, (x0 - 6.0 * s, y, w + 6.0 * s, FELD_H * s))
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
        if z.projektdaten.is_some_and(|r| inside(r, x, y)) {
            return Some(Some(Hot::Projektdaten));
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
    pub(super) fn kopf_klick(&mut self, hot: Option<Hot>) -> Option<ListOut> {
        match hot? {
            Hot::Kopf => {
                self.kopf_offen = !self.kopf_offen;
                self.clamp();
                Some(ListOut::Repaint)
            }
            // Die Maske „Projektdaten“ im passenden Feld (Paket PD-3)
            Hot::BauherrFehlt => Some(ListOut::Projektdaten(
                self.fehlt().unwrap_or(Feld::Bauherr).maske_feld(),
            )),
            Hot::Projektdaten => Some(ListOut::Projektdaten(0)),
            Hot::Feld(f) => Some(ListOut::Projektdaten(f.maske_feld())),
            Hot::Mehr => {
                self.mehr_offen = !self.mehr_offen;
                Some(ListOut::Repaint)
            }
            Hot::Untertitel => {
                self.mehr_offen = false;
                Some(ListOut::Kosten(crate::kosten_view::Schreiben::Gliederung(
                    !self.untertitel,
                )))
            }
            _ => None,
        }
    }

    /// Ein Verweis in der Kopfzeile: fett im lesbaren Akzent, unter der
    /// Maus unterstrichen.
    fn verweis(&self, c: &mut Canvas, t: &Theme, bold: &Font, text: &str, r: Rect, hot: bool) {
        let s = self.scale;
        let col = crate::cards::verweis(&t.ui, hot);
        let px = 10.0 * s;
        let base = r.1 + (r.3 + bold.cap_height(px)) * 0.5;
        bold.draw(c, text, px, r.0, base, col);
        if hot {
            let w = bold.width(text, px);
            c.fill_rect(r.0, base + 2.0 * s, w, s.max(1.0), col);
        }
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
        // Verweise auf dem hellen Blatt im lesbaren Akzent (reiner Akzent
        // hat dort 1,7 : 1, Einstellungen 09.10.)
        if let (Some(r), Some(f)) = (z.fehlt, self.fehlt_text()) {
            self.verweis(c, t, bold, &f, r, self.hot == Some(Hot::BauherrFehlt));
        }
        if let Some(r) = z.projektdaten {
            let hot = self.hot == Some(Hot::Projektdaten);
            self.verweis(c, t, bold, PROJEKTDATEN, r, hot);
        }
        // „Mehr ▸“ bzw. offen „Mehr ▾“
        let (mx, _, _, _) = z.mehr;
        let col = crate::cards::verweis(u, self.hot == Some(Hot::Mehr));
        bold.draw(c, MEHR, px, mx, mitte(z.mehr, bold, px), col);
        let w = bold.width(MEHR, px);
        widgets::disclosure(
            c,
            mx + w + 7.0 * s,
            z.mehr.1 + z.mehr.3 * 0.5,
            self.mehr_offen,
            col,
            s,
        );
        if !self.kopf_offen {
            return;
        }
        let lv = self.lv.as_deref();
        let (x0, cw) = self.content_x(t);
        let lpx = 10.5 * s;
        // Links die Projektdaten, nur zum Lesen; ein Klick öffnet die Maske
        for (f, r) in self.felder(t) {
            let base = mitte(r, regular, lpx);
            if self.hot == Some(Hot::Feld(f)) {
                let mut p = Path::new();
                p.rounded_rect(r.0, r.1, r.2, r.3, 4.0 * s);
                c.fill(&p, u.sheet_hover);
            }
            regular.draw(c, f.label(), lpx, x0, base, u.sheet_text_dim);
            let (text, ton) = self.kopf_wert(f);
            let col = match ton {
                0 => u.sheet_text,
                1 => u.sheet_hint,
                _ => crate::cards::verweis(u, false),
            };
            let breit = r.0 + r.2 - (x0 + FELD_X * s) - 6.0 * s;
            let text = widgets::ellipsize(Some(regular), &text, lpx, breit);
            regular.draw(c, &text, lpx, x0 + FELD_X * s, base, col);
        }
        // Rechts (im schmalen Blatt darunter): Los, Umfang, Stand und
        // Preise wie in der CSV (Kosten B4); darunter die Vorbemerkungen
        let y0 = self.kopf_y0();
        let (rx, ry) = if self.kopf_zwei_spalten() {
            (x0 + RECHTS_X * s, y0)
        } else {
            (x0, y0 + Feld::ALLE.len() as f32 * ZEILE * s)
        };
        let wert_x = if self.kopf_zwei_spalten() {
            80.0 * s
        } else {
            FELD_X * s
        };
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
            let r = (rx, ry + i as f32 * ZEILE * s, 0.0, FELD_H * s);
            let base = mitte(r, regular, lpx);
            regular.draw(c, k, lpx, rx, base, u.sheet_text_dim);
            let v = widgets::ellipsize(Some(regular), v, lpx, x0 + cw - rx - wert_x);
            regular.draw(c, &v, lpx, rx + wert_x, base, u.sheet_text);
        }
        let zeilen_davor = if self.kopf_zwei_spalten() {
            Feld::ALLE.len()
        } else {
            Feld::ALLE.len() + 4
        };
        let y = y0 + zeilen_davor as f32 * ZEILE * s + 4.0 * s;
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

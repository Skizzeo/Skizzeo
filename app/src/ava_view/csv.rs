//! „LV {Los} als Tabelle speichern“ (KA-4d, ka-4-fach §3.6): Knopf oben
//! rechts und die CSV des gewählten Loses, Werte wie in der Anzeige.

use super::*;
use crate::kosten_view::csv_text;

const BUTTON_H: f32 = crate::cards::KNOPF_H;
const BUTTON_PAD: f32 = crate::cards::KNOPF_PAD;
const BUTTON_Y: f32 = 14.0;
/// Knopf „Druckvorschau“ und sein Abstand zum Speichern-Knopf (dip).
const VORSCHAU: &str = "Druckvorschau";
const VORSCHAU_GAP: f32 = 8.0;

/// Einheit als GAEB-Wort (m2, m3, m, t, St).
fn gaeb(e: sk_cost::katalog::Einheit) -> &'static str {
    match e {
        sk_cost::katalog::Einheit::St => "St",
        e => e.wort(),
    }
}

fn feld(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Betrag ohne Tausenderpunkt („3977,60“), leer ohne Preis.
fn zahl(c: Option<Cent>) -> String {
    c.map_or_else(String::new, |c| euro(c).replace('.', ""))
}

impl AvaView {
    /// Beschriftung des Knopfs: „LV Rohbau als Tabelle speichern“.
    pub fn knopf_text(&self) -> String {
        let Some(lv) = self.lv.as_deref() else {
            return "Als Tabelle speichern".into();
        };
        let los = &lv.kopf.los;
        match self.ansicht {
            // Die Fassung im Knopf (Bedienbarkeit 26.2)
            Ansicht::Blatt if self.preise => format!("LV {los} mit Preisen als PDF speichern"),
            Ansicht::Blatt => format!("Anfrage LV {los} als PDF speichern"),
            _ => format!("LV {los} als Tabelle speichern"),
        }
    }

    /// Name des gewählten Loses („Rohbau“) für den Dateinamen.
    pub fn los_name(&self) -> Option<&str> {
        self.lv.as_deref().map(|lv| lv.kopf.los.as_str())
    }

    pub(super) fn knopf_rect(&self, t: &Theme, fonts: &Fonts) -> Rect {
        self.knopf_lage(t, fonts).0
    }

    /// Lage und Beschriftung des Speichern-Knopfs. In der Kartenzeile, wenn
    /// Platz ist (auch für „Druckvorschau“ links daneben), sonst in der
    /// Titelzeile; dort wird die Beschriftung kürzer, bis sie (mit
    /// „Druckvorschau“) neben „Leistungsverzeichnis“ passt.
    pub(super) fn knopf_lage(&self, t: &Theme, fonts: &Fonts) -> (Rect, String) {
        let s = self.scale;
        let (x0, w) = self.content_x(t);
        let f = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let breite =
            |text: &str| f.map_or(200.0 * s, |f| f.width(text, 11.0 * s)) + 2.0 * BUTTON_PAD * s;
        let voll = self.knopf_text();
        let bw = breite(&voll);
        let x = x0 + w - bw;
        let links = match self.vorschau_breite(fonts.bold.as_ref()) {
            Some(vw) if !self.baum_als_leiste() => x - VORSCHAU_GAP * s - vw,
            _ => x,
        };
        let breit = (self.w as f32 - 2.0 * x0).max(0.0);
        if let Some(y) = crate::cards::Karten::knopf_y(x0, s, breit, links, self.top_px()) {
            return ((x, y, bw, BUTTON_H * s), voll);
        }
        let y = self.top_px() + BUTTON_Y * s;
        let titel = fonts
            .bold
            .as_ref()
            .map_or(190.0 * s, |f| f.width("Leistungsverzeichnis", 19.0 * s));
        // „Druckvorschau“ steht in der Titelzeile links neben dem Knopf
        let frei = x0 + titel + 16.0 * s + (x - links);
        let mut texte = vec![voll];
        if self.ansicht != Ansicht::Blatt {
            texte.push("Als Tabelle speichern".into());
        } else {
            // Die Fassung bleibt bis zur kürzesten Stufe (Bedienbarkeit 27.1)
            let kurz = if self.preise {
                ["Mit Preisen als PDF speichern", "PDF mit Preisen"]
            } else {
                ["Anfrage als PDF speichern", "PDF ohne Preise"]
            };
            texte.extend(kurz.map(String::from));
        }
        let i = texte
            .iter()
            .position(|t| x0 + w - breite(t) >= frei)
            .unwrap_or(texte.len() - 1);
        let text = texte.swap_remove(i);
        let bw = breite(&text);
        ((x0 + w - bw, y, bw, BUTTON_H * s), text)
    }

    /// Breite des Knopfs „Druckvorschau“ (px); nur in der Ansicht LV.
    pub(super) fn vorschau_breite(&self, f: Option<&Font>) -> Option<f32> {
        if self.ansicht != Ansicht::Lv || self.lv.is_none() {
            return None;
        }
        let s = self.scale;
        let tw = f.map_or(80.0 * s, |f| f.width(VORSCHAU, 11.0 * s));
        Some(tw + 2.0 * BUTTON_PAD * s)
    }

    /// Knopf „Druckvorschau“ (Bedienbarkeit 26.1): links neben „als
    /// Tabelle speichern“; im schmalen Fenster rechts in der Zeile der
    /// Leiste.
    pub(super) fn vorschau_rect(&self, t: &Theme, fonts: &Fonts) -> Option<Rect> {
        let vw = self.vorschau_breite(fonts.bold.as_ref())?;
        let s = self.scale;
        if self.baum_als_leiste() {
            let (x0, cw) = self.content_x(t);
            return Some((x0 + cw - vw, self.body_top(), vw, 26.0 * s));
        }
        let (kx, ky, _, kh) = self.knopf_rect(t, fonts);
        Some((kx - VORSCHAU_GAP * s - vw, ky, vw, kh))
    }

    pub(super) fn paint_vorschau_knopf(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let Some((x, y, w, h)) = self.vorschau_rect(t, fonts) else {
            return;
        };
        let s = self.scale;
        let u = &t.ui;
        let bg = if self.hot == Some(Hot::Vorschau) {
            u.sheet_hover
        } else {
            u.sheet_tile
        };
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, t.size.corner_radius * s);
        c.fill(&p, bg);
        if let Some(f) = fonts.bold.as_ref() {
            let px = 11.0 * s;
            let base = y + (h + f.cap_height(px)) * 0.5;
            f.draw(c, VORSCHAU, px, x + BUTTON_PAD * s, base, u.sheet_text);
        }
    }

    pub fn mouse_up(&mut self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<ListOut> {
        if !std::mem::take(&mut self.knopf_down) {
            return None;
        }
        if self.hit(t, fonts, x, y) != Some(Hot::Knopf) {
            Some(ListOut::Repaint)
        } else if self.ansicht == Ansicht::Blatt {
            Some(ListOut::SavePdf)
        } else {
            Some(ListOut::SaveCsv)
        }
    }

    pub(super) fn paint_knopf(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let ((bx, by, bw, bh), text) = self.knopf_lage(t, fonts);
        let bg = if self.knopf_down {
            u.pressed
        } else if self.hot == Some(Hot::Knopf) {
            u.hover
        } else {
            u.bg
        };
        let mut p = Path::new();
        p.rounded_rect(bx, by, bw, bh, t.size.corner_radius * s);
        c.fill(&p, bg);
        if let Some(f) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
            let px = 11.0 * s;
            f.draw(
                c,
                &text,
                px,
                bx + BUTTON_PAD * s,
                by + (bh + f.cap_height(px)) * 0.5,
                u.text,
            );
        }
    }

    /// CSV des gewählten Loses: UTF-8 mit BOM, „;“, Dezimalkomma. Kopf
    /// (ka-4-fach §3.2), Vorbemerkungen als eine Zeile, dann OZ | Kurztext |
    /// Menge | ME | EP | GP mit Titel- und Untertitelzeilen und ihren
    /// Summen, am Ende die Zusammenstellung. „Für Anfrage (leer)“ lässt EP,
    /// GP und Summen leer.
    pub fn csv(&self) -> Vec<u8> {
        let mut out = String::from("\u{feff}");
        let mut zeile = |cols: &[&str]| {
            let v: Vec<String> = cols.iter().map(|c| feld(c)).collect();
            out.push_str(&v.join(";"));
            out.push_str("\r\n");
        };
        let Some(lv) = self.lv.as_deref() else {
            return out.into_bytes();
        };
        let k = &lv.kopf;
        let p = self.projekt.as_ref();
        zeile(&["Leistungsverzeichnis", &format!("LV {}", k.los)]);
        // Mehrzeilige Projektdaten (Bauort, Anschriften) in einer Zelle mit
        // „, “ (Paket PD-3)
        let einzeilig = |v: &Option<String>| {
            v.as_deref()
                .unwrap_or("")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        };
        zeile(&["Bauvorhaben", &self.bauvorhaben(lv)]);
        zeile(&["Projektart", &einzeilig(&k.projektart)]);
        zeile(&["Bauort", &einzeilig(&k.bauort)]);
        zeile(&["Projekt-Nr.", &einzeilig(&k.projektnummer)]);
        zeile(&["Bauherr", p.map_or("", |p| p.client.as_str())]);
        zeile(&["Anschrift Bauherr", &einzeilig(&k.bauherr_anschrift)]);
        zeile(&["Aufsteller", k.aufsteller.as_deref().unwrap_or("")]);
        zeile(&["Anschrift Aufsteller", &einzeilig(&k.aufsteller_anschrift)]);
        zeile(&["Los", &k.los]);
        zeile(&["Umfang", &self.kopf_umfang.0]);
        zeile(&["Datum", &self.kopf_umfang.1]);
        zeile(&["LV-Art", k.art]);
        zeile(&["Währung", k.waehrung]);
        zeile(&[k.netto]);
        // ka-4-fach §3.2 Nachtrag 14:40: Werkspreise „unverbindlich“
        if self.preisquelle.starts_with("Referenzpreise") {
            zeile(&[
                "Preisquelle",
                &format!("{}, unverbindlich", self.preisquelle),
            ]);
        } else if !self.preisquelle.is_empty() {
            zeile(&["Preisquelle", &self.preisquelle]);
        }
        let vor = k
            .vorbemerkungen
            .as_deref()
            .unwrap_or("")
            .replace(['\r', '\n'], " ");
        zeile(&["Vorbemerkungen", vor.trim()]);
        zeile(&[]);
        zeile(&["OZ", "Kurztext", "Menge", "ME", "EP", "GP"]);
        for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
            zeile(&[&csv_text(&t.nr), &t.name]);
            let mut uu = None;
            let summe_uu = |uu: Option<u32>, zeile: &mut dyn FnMut(&[&str])| {
                if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                    let text = format!("Summe {} {}", u.oz, u.name);
                    zeile(&["", &text, "", "", "", &zahl(u.summe)]);
                }
            };
            for p in &t.positionen {
                if p.untertitel.is_some() && p.untertitel != uu {
                    summe_uu(uu, &mut zeile);
                    uu = p.untertitel;
                    if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                        zeile(&[&csv_text(&u.oz), &u.name]);
                    }
                }
                zeile(&[
                    &csv_text(&p.oz),
                    &p.kurztext,
                    &menge_zahl(p.menge).replace('.', ""),
                    gaeb(p.einheit),
                    &zahl(p.ep),
                    &zahl(p.gp),
                ]);
            }
            summe_uu(uu, &mut zeile);
            // Kosten A2: eine Summe ohne alle Preise nicht still zu niedrig
            let text = if t.unvollstaendig && self.preise {
                format!("Summe {} {} (unvollständig)", t.nr, t.name)
            } else {
                format!("Summe {} {}", t.nr, t.name)
            };
            zeile(&["", &text, "", "", "", &zahl(t.summe)]);
        }
        zeile(&[]);
        let z = &lv.zusammenstellung;
        zeile(&["Zusammenstellung"]);
        for (nr, name, summe) in &z.zeilen {
            zeile(&[&csv_text(nr), name, "", "", "", &zahl(*summe)]);
        }
        let satz = z.mwst_satz.text().replace('.', ",");
        zeile(&["", "Summe netto", "", "", "", &zahl(z.netto)]);
        if let Some(g) = z.geschaetzt {
            let text = "nicht ausgeschrieben (geschätzt), nicht in der Summe";
            zeile(&["", text, "", "", "", &zahl(Some(g))]);
        }
        zeile(&["", &format!("MwSt. {satz} %"), "", "", "", &zahl(z.mwst)]);
        zeile(&["", "Summe brutto", "", "", "", &zahl(z.brutto)]);
        if z.unvollstaendig {
            zeile(&["", UNVOLLSTAENDIG]);
        }
        out.into_bytes()
    }
}

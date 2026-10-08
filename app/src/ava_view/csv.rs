//! „LV {Los} als Tabelle speichern“ (KA-4d, ka-4-fach §3.6): Knopf oben
//! rechts und die CSV des gewählten Loses, Werte wie in der Anzeige.

use super::*;

const BUTTON_H: f32 = 26.0;
const BUTTON_PAD: f32 = 12.0;
const BUTTON_Y: f32 = 14.0;

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
        match self.lv.as_deref() {
            Some(lv) => format!("LV {} als Tabelle speichern", lv.kopf.los),
            None => "Als Tabelle speichern".into(),
        }
    }

    /// Name des gewählten Loses („Rohbau“) für den Dateinamen.
    pub fn los_name(&self) -> Option<&str> {
        self.lv.as_deref().map(|lv| lv.kopf.los.as_str())
    }

    pub(super) fn knopf_rect(&self, t: &Theme, fonts: &Fonts) -> Rect {
        let s = self.scale;
        let (x0, w) = self.content_x(t);
        let text = self.knopf_text();
        let tw = fonts
            .bold
            .as_ref()
            .or(fonts.regular.as_ref())
            .map_or(200.0 * s, |f| f.width(&text, 11.0 * s));
        let bw = tw + 2.0 * BUTTON_PAD * s;
        (x0 + w - bw, self.top_px() + BUTTON_Y * s, bw, BUTTON_H * s)
    }

    pub fn mouse_up(&mut self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<ListOut> {
        if !std::mem::take(&mut self.knopf_down) {
            return None;
        }
        if self.hit(t, fonts, x, y) == Some(Hot::Knopf) {
            Some(ListOut::SaveCsv)
        } else {
            Some(ListOut::Repaint)
        }
    }

    pub(super) fn paint_knopf(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let (bx, by, bw, bh) = self.knopf_rect(t, fonts);
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
                &self.knopf_text(),
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
        zeile(&["Bauvorhaben", &self.bauvorhaben(lv)]);
        zeile(&["Bauherr", p.map_or("", |p| p.client.as_str())]);
        zeile(&["Aufsteller", k.aufsteller.as_deref().unwrap_or("")]);
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
            zeile(&[&t.nr, &t.name]);
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
                        zeile(&[&u.oz, &u.name]);
                    }
                }
                zeile(&[
                    &p.oz,
                    &p.kurztext,
                    &menge_zahl(p.menge).replace('.', ""),
                    gaeb(p.einheit),
                    &zahl(p.ep),
                    &zahl(p.gp),
                ]);
            }
            summe_uu(uu, &mut zeile);
            let text = format!("Summe {} {}", t.nr, t.name);
            zeile(&["", &text, "", "", "", &zahl(t.summe)]);
        }
        zeile(&[]);
        let z = &lv.zusammenstellung;
        zeile(&["Zusammenstellung"]);
        for (nr, name, summe) in &z.zeilen {
            zeile(&[nr, name, "", "", "", &zahl(*summe)]);
        }
        let satz = z.mwst_satz.text().replace('.', ",");
        zeile(&["", "Summe netto", "", "", "", &zahl(z.netto)]);
        if let Some(g) = z.geschaetzt {
            let text = "nicht ausgeschrieben (geschätzt), nicht in der Summe";
            zeile(&["", text, "", "", "", &zahl(Some(g))]);
        }
        zeile(&["", &format!("MwSt. {satz} %"), "", "", "", &zahl(z.mwst)]);
        zeile(&["", "Summe brutto", "", "", "", &zahl(z.brutto)]);
        out.into_bytes()
    }
}

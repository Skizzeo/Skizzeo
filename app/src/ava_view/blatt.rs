//! Druckvorschau des LV-Blatts (kosten/lv-blatt-a4.md §9, paket-projektdaten
//! §5): Seite für Seite wie der Ausdruck, darüber Blättern und die Häkchen
//! „Titelblatt“ und „Inhaltsverzeichnis“; der Knopf oben schreibt genau
//! diese Seiten als PDF.

use super::*;
use crate::lv_blatt::{self, Angaben, Blatt, Schriften, Wahl};
use sk_paint::pdf::A4;

/// Höhe der Leiste über dem Blatt (dip).
const LEISTE: f32 = 30.0;
/// Rand um das Blatt (dip).
const LUFT: f32 = 10.0;
/// Pfeilknopf „‹“ bzw. „›“ (dip).
const PFEIL: f32 = 24.0;
/// Kästchen der Häkchen (dip).
const KASTEN: f32 = 14.0;

/// Woraus das Blatt gebaut ist; gleich, dann gilt das gemerkte. Das LV
/// hält der Schlüssel fest, damit seine Adresse nicht neu vergeben wird.
pub(super) struct Schluessel {
    lv: Rc<Lv>,
    angaben: Angaben,
    wahl: (bool, bool, bool),
    schrift: (*const Font, *const Font),
}

impl Schluessel {
    fn gleich(&self, o: &Schluessel) -> bool {
        Rc::ptr_eq(&self.lv, &o.lv)
            && self.angaben == o.angaben
            && self.wahl == o.wahl
            && self.schrift == o.schrift
    }
}

impl AvaView {
    /// Angaben neben dem LV, wie der Kopf am Bildschirm sie zeigt.
    fn angaben(&self, lv: &Lv) -> Angaben {
        // „09.10.2026, 02:05“ → „09.10.2026“
        let stand = self.kopf_umfang.1.split(',').next().unwrap_or("").trim();
        let quelle = (self.preise && !self.preisquelle.is_empty()).then(|| {
            if self.preisquelle.starts_with("Referenzpreise") {
                format!("{}, unverbindlich", self.preisquelle)
            } else {
                self.preisquelle.clone()
            }
        });
        Angaben {
            bauvorhaben: self.bauvorhaben(lv),
            umfang: self.kopf_umfang.0.clone(),
            stand: stand.to_string(),
            preisquelle: quelle,
        }
    }

    /// Das Blatt des gezeigten LV; gemerkt, solange sich nichts ändert.
    pub(super) fn blatt(&self, fonts: &Fonts) -> Option<Rc<Blatt>> {
        let lv = self.lv.as_ref()?;
        let regular = fonts.regular.as_ref()?;
        let fett = fonts.bold.as_ref().unwrap_or(regular);
        let (titelblatt, verzeichnis) = self.blatt_wahl;
        let key = Schluessel {
            lv: lv.clone(),
            angaben: self.angaben(lv),
            wahl: (self.preise, titelblatt, verzeichnis),
            schrift: (regular, fett),
        };
        if let Some((k, b)) = self.blatt_cache.borrow().as_ref() {
            if k.gleich(&key) {
                return Some(b.clone());
            }
        }
        let w = Wahl {
            preise: self.preise,
            titelblatt,
            verzeichnis,
        };
        let s = Schriften { regular, fett };
        let b = Rc::new(lv_blatt::blatt(lv, &key.angaben, w, s));
        *self.blatt_cache.borrow_mut() = Some((key, b.clone()));
        Some(b)
    }

    /// „Als PDF speichern“: Dateiname und Inhalt, genau die Vorschau.
    pub fn pdf(&self, fonts: &Fonts) -> Option<(String, Vec<u8>)> {
        let lv = self.lv.as_deref()?;
        let b = self.blatt(fonts)?;
        let regular = fonts.regular.as_ref()?;
        let fett = fonts.bold.as_ref().unwrap_or(regular);
        let a = self.angaben(lv);
        // „09.10.2026“ → „2026-10-09“
        let d: Vec<&str> = a.stand.split('.').collect();
        let datum = match d.as_slice() {
            [t, m, j] => format!("{j}-{m}-{t}"),
            _ => a.stand.clone(),
        };
        let name = lv_blatt::dateiname(lv, &a.bauvorhaben, &datum, self.preise);
        let titel = format!("LV {}", lv.kopf.los);
        let bytes = sk_paint::pdf::schreiben(&b.seiten, A4, regular, fett, &titel);
        Some((name, bytes))
    }

    /// Seiten des Blatts (mindestens 1).
    fn seiten(&self, fonts: &Fonts) -> usize {
        self.blatt(fonts).map_or(1, |b| b.seiten.len().max(1))
    }

    /// Blättert eine Seite vor oder zurück; `true`, wenn sich die Seite
    /// ändert.
    pub(super) fn blaettern(&mut self, fonts: &Fonts, vor: bool) -> bool {
        let n = self.seiten(fonts);
        let neu = if vor {
            (self.seite + 1).min(n - 1)
        } else {
            self.seite.saturating_sub(1)
        };
        let anders = neu != self.seite;
        self.seite = neu;
        anders
    }

    /// Die gezeigte Seite (ab 0), nie hinter der letzten.
    pub(super) fn seite_jetzt(&self, fonts: &Fonts) -> usize {
        self.seite.min(self.seiten(fonts) - 1)
    }

    /// Teile der Leiste (px): „‹“, „Seite x von y“ (Lage der Grundlinie),
    /// „›“, die Häkchen mit Kästchen und Text-x.
    fn vorschau_leiste(&self, t: &Theme, fonts: &Fonts) -> VorschauLeiste {
        let s = self.scale;
        let (tx, _) = self.tabelle_x(t);
        let y = self.tabelle_top();
        let px = 11.0 * s;
        let f = fonts.regular.as_ref();
        let breite = |text: &str| {
            f.map_or(text.chars().count() as f32 * px * 0.55, |f| {
                f.width(text, px)
            })
        };
        let mitte = y + LEISTE * s * 0.5;
        let zurueck = (tx, mitte - PFEIL * s * 0.5, PFEIL * s, PFEIL * s);
        let text = format!(
            "Seite {} von {}",
            self.seite_jetzt(fonts) + 1,
            self.seiten(fonts)
        );
        // Breite für „Seite 88 von 88“, damit „›“ beim Blättern still steht
        let tw = breite("Seite 88 von 88");
        let text_x = tx + (PFEIL + 8.0) * s;
        let vor = (text_x + tw + 8.0 * s, zurueck.1, PFEIL * s, PFEIL * s);
        let mut x = vor.0 + vor.2 + 24.0 * s;
        let mut haken = Vec::new();
        for (i, label) in ["Titelblatt", "Inhaltsverzeichnis"].into_iter().enumerate() {
            let w = KASTEN * s + 6.0 * s + breite(label);
            haken.push(((x, mitte - 10.0 * s, w, 20.0 * s), label, i == 1));
            x += w + 18.0 * s;
        }
        VorschauLeiste {
            zurueck,
            vor,
            text,
            text_x,
            mitte,
            haken,
        }
    }

    /// Lage des Blatts (px): linke obere Ecke und Pixel je Punkt.
    fn blatt_lage(&self, t: &Theme) -> (f32, f32, f32) {
        let s = self.scale;
        let (tx, r) = self.tabelle_x(t);
        let oben = self.tabelle_top() + (LEISTE + LUFT) * s;
        let unten = self.bottom() - LUFT * s;
        let k = ((r - tx) / A4.0)
            .min((unten - oben).max(0.0) / A4.1)
            .max(0.05);
        let x = tx + ((r - tx) - A4.0 * k) * 0.5;
        (x, oben, k)
    }

    pub(super) fn vorschau_hit(&self, t: &Theme, fonts: &Fonts, x: f32, y: f32) -> Option<Hot> {
        let l = self.vorschau_leiste(t, fonts);
        if inside(l.zurueck, x, y) {
            return Some(Hot::Seite(false));
        }
        if inside(l.vor, x, y) {
            return Some(Hot::Seite(true));
        }
        l.haken
            .iter()
            .find(|(r, ..)| inside(*r, x, y))
            .map(|(_, _, v)| Hot::BlattWahl(*v))
    }

    pub(super) fn paint_vorschau(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let Some(regular) = fonts.regular.as_ref() else {
            return;
        };
        let fett = fonts.bold.as_ref().unwrap_or(regular);
        let l = self.vorschau_leiste(t, fonts);
        let n = self.seiten(fonts);
        let jetzt = self.seite_jetzt(fonts);
        for (r, vor, geht) in [(l.zurueck, false, jetzt > 0), (l.vor, true, jetzt + 1 < n)] {
            if geht && self.hot == Some(Hot::Seite(vor)) {
                let mut p = Path::new();
                p.rounded_rect(r.0, r.1, r.2, r.3, 5.0 * s);
                c.fill(&p, u.sheet_hover);
            }
            let col = if geht { u.sheet_text } else { u.sheet_text_dim };
            // Pfeil ‹ bzw. › gezeichnet
            let (mx, my, d) = (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5, 4.0 * s);
            let (a, b) = if vor {
                (-d * 0.5, d * 0.5)
            } else {
                (d * 0.5, -d * 0.5)
            };
            let mut p = Path::new();
            p.segment((mx + a, my - d), (mx + b, my), 1.5 * s);
            p.segment((mx + b, my), (mx + a, my + d), 1.5 * s);
            c.fill(&p, col);
        }
        let px = 11.0 * s;
        let base = l.mitte + regular.cap_height(px) * 0.5;
        regular.draw(c, &l.text, px, l.text_x, base, u.sheet_text);
        for ((x, _, _, _), label, v) in &l.haken {
            let an = if *v {
                self.blatt_wahl.1
            } else {
                self.blatt_wahl.0
            };
            let hot = self.hot == Some(Hot::BlattWahl(*v));
            // Kästchen in den Farben des Blatts (das Dunkle der Felder
            // passt nicht auf das helle Blatt)
            let (kx, ky, d) = (*x, l.mitte - KASTEN * s * 0.5, KASTEN * s);
            let b = s.round().max(1.0);
            let rand = if an {
                u.accent
            } else if hot {
                u.sheet_text
            } else {
                u.sheet_text_dim
            };
            let mut p = Path::new();
            p.rounded_rect(kx, ky, d, d, 3.0 * s);
            c.fill(&p, rand);
            let mut p = Path::new();
            p.rounded_rect(kx + b, ky + b, d - 2.0 * b, d - 2.0 * b, 3.0 * s - b);
            c.fill(&p, if an { u.accent } else { u.sheet_card });
            if an {
                let k = d / 16.0;
                let mut p = Path::new();
                p.segment(
                    (kx + 3.5 * k, ky + 8.5 * k),
                    (kx + 6.5 * k, ky + 11.5 * k),
                    2.0 * s,
                );
                p.segment(
                    (kx + 6.5 * k, ky + 11.5 * k),
                    (kx + 12.5 * k, ky + 4.5 * k),
                    2.0 * s,
                );
                c.fill(&p, u.on_accent);
            }
            regular.draw(c, label, px, x + (KASTEN + 6.0) * s, base, u.sheet_text);
        }
        // Das Blatt: weißes Papier mit Schatten, auch im dunklen Schema
        let Some(b) = self.blatt(fonts) else {
            return;
        };
        let Some(seite) = b.seiten.get(jetzt) else {
            return;
        };
        let (x, y, k) = self.blatt_lage(t);
        let (w, h) = (A4.0 * k, A4.1 * k);
        c.fill_rect(x + 2.0 * s, y + 3.0 * s, w, h, Rgba(0, 0, 0, 40));
        c.fill_rect(x, y, w, h, Rgba(255, 255, 255, 255));
        sk_paint::pdf::zeichnen(c, seite, (x, y), k, regular, fett);
    }
}

/// Lage der Leiste über dem Blatt.
struct VorschauLeiste {
    zurueck: Rect,
    vor: Rect,
    text: String,
    text_x: f32,
    /// Senkrechte Mitte der Leiste.
    mitte: f32,
    /// Häkchen: Fläche, Text und `true` für „Inhaltsverzeichnis“.
    haken: Vec<(Rect, &'static str, bool)>,
}

//! Blatt „Bauleistung wählen …“ an einer grauen Zeile (KA-2c, paket-ka2 §4,
//! Einstellungen §3 KA-2 Punkt 4, soll-ka-2e): 500 dip im Stil des
//! Preisblatts, Titel „Bauleistung für {Zeile}“, Menge leise, Suchfeld, die
//! Bauleistungen mit EP leise rechts und unten „Klick ordnet zu · Esc
//! schließt“. Ordnung und EP kommen aus `sk_cost::wahl` (Bedienbarkeit 5.1);
//! das Blatt filtert nur nach dem Suchtext.

use crate::preis_blatt::{flaeche, inside, Rect, SPITZE};
use sk_cost::wahl::{Auswahl, Wahl};
use sk_model::{ElementId, Guid};
use sk_paint::{Canvas, Path};
use sk_platform::{Key, Modifiers};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts};

const W: f32 = 500.0;
const PAD: f32 = 16.0;
const SUCHE_H: f32 = 26.0;
const EINTRAG: f32 = 26.0;
/// Zusatzhöhe für „kommt dann zu …“ unter dem überfahrenen Eintrag.
const FREMD: f32 = 14.0;
const LEER: f32 = 38.0;
const FUSS: f32 = 30.0;
/// Höchstens so viele Einträge auf einmal; mehr rollen.
const SICHTBAR: usize = 10;

/// Ergebnis an die Kostenansicht.
#[derive(Clone, Debug, PartialEq)]
pub enum Aus {
    Repaint,
    /// Bauleistung gewählt: an die Schicht der Zeile schreiben.
    Waehlen(Guid),
    Schliessen,
}

/// Eine Zeile der Liste.
#[derive(Clone, Debug, PartialEq)]
enum Eintrag {
    /// „Für Dachdecker gibt es noch keine Bauleistung.“ und leise darunter.
    Leer(String, String),
    Wahl(Wahl),
    /// „+ n weitere in m²“.
    Weitere(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ziel {
    Schliessen,
    Suche,
    Eintrag(usize),
    Innen,
}

pub struct WahlBlatt {
    /// Zeile ohne Bauleistung im Kostenblatt und ihr Bauteil.
    pub ohne: usize,
    pub element: ElementId,
    titel: String,
    menge: String,
    auswahl: Auswahl,
    suche: TextEdit,
    /// „+ n weitere“ aufgeklappt.
    weitere: bool,
    /// Erster sichtbarer Eintrag.
    oben: usize,
    hot: Option<Ziel>,
    anker: Rect,
    pub scale: f32,
    pub fenster: (f32, f32),
}

impl WahlBlatt {
    pub fn neu(
        (ohne, element): (usize, ElementId),
        zeile: &str,
        menge: String,
        auswahl: Auswahl,
    ) -> WahlBlatt {
        WahlBlatt {
            ohne,
            element,
            titel: format!("Bauleistung für {zeile}"),
            menge,
            auswahl,
            suche: TextEdit::new(""),
            weitere: false,
            oben: 0,
            hot: None,
            anker: (0.0, 0.0, 0.0, 0.0),
            scale: 1.0,
            fenster: (0.0, 0.0),
        }
    }

    pub fn set_anker(&mut self, r: Rect) {
        self.anker = r;
    }

    /// Einträge nach Suchtext und Aufklappen. Mit Suchtext steht alles
    /// Passende in der Ordnung von `sk_cost::wahl` ohne Kopfzeilen.
    fn eintraege(&self) -> Vec<Eintrag> {
        let a = &self.auswahl;
        let q = self.suche.text.trim().to_lowercase();
        if !q.is_empty() {
            return a
                .alle()
                .filter(|w| w.kurz.to_lowercase().contains(&q))
                .cloned()
                .map(Eintrag::Wahl)
                .collect();
        }
        let mut out: Vec<Eintrag> = a.eigene.iter().cloned().map(Eintrag::Wahl).collect();
        if let Some((fett, leise)) = a.leer_text() {
            out.push(Eintrag::Leer(fett, leise));
        }
        out.extend(a.aehnlich.iter().cloned().map(Eintrag::Wahl));
        if self.weitere {
            out.extend(a.weitere.iter().cloned().map(Eintrag::Wahl));
        } else if let Some(t) = a.weitere_text() {
            out.push(Eintrag::Weitere(t));
        }
        out
    }

    fn hoch(&self, e: &Eintrag, i: usize) -> f32 {
        match e {
            Eintrag::Leer(..) => LEER,
            Eintrag::Wahl(w) if w.fremd.is_some() && self.markiert() == Some(i) => EINTRAG + FREMD,
            _ => EINTRAG,
        }
    }

    /// Überfahrener Eintrag.
    fn markiert(&self) -> Option<usize> {
        match self.hot {
            Some(Ziel::Eintrag(i)) => Some(i),
            _ => None,
        }
    }

    /// Sichtbare Einträge mit Oberkante relativ zum Listenanfang (dip).
    fn sichtbar(&self) -> Vec<(usize, Eintrag, f32, f32)> {
        let mut y = 0.0;
        let mut out = Vec::new();
        for (i, e) in self.eintraege().into_iter().enumerate().skip(self.oben) {
            if out.len() == SICHTBAR {
                break;
            }
            let h = self.hoch(&e, i);
            out.push((i, e, y, h));
            y += h;
        }
        out
    }

    /// Höhe der Liste (dip): fest für `SICHTBAR` Einträge, damit das Blatt
    /// beim Tippen nicht springt; kleiner, wenn es weniger gibt.
    fn liste_h(&self) -> f32 {
        let n = self.eintraege().len().min(SICHTBAR);
        let sichtbar: f32 = self.sichtbar().iter().map(|(_, _, _, h)| h).sum();
        sichtbar.max(n as f32 * EINTRAG).max(EINTRAG)
    }

    fn hoehe(&self) -> f32 {
        PAD + 22.0 + 16.0 + 8.0 + SUCHE_H + 8.0 + self.liste_h() + FUSS
    }

    fn rect(&self) -> (Rect, bool) {
        let s = self.scale;
        let (w, h) = (W * s, self.hoehe() * s);
        let (ax0, ay0, ax1, ay1) = self.anker;
        let (fw, fh) = self.fenster;
        let rand = 8.0 * s;
        let cx = (ax0 + ax1) * 0.5;
        let x = (cx - w * 0.8).min(fw - w - rand).max(rand);
        let unten = ay1 + SPITZE * s + h + rand <= fh || ay0 - SPITZE * s - h < rand;
        let y = if unten {
            ay1 + SPITZE * s
        } else {
            ay0 - SPITZE * s - h
        };
        ((x, y, w, h), unten)
    }

    fn schliessen_rect(&self) -> Rect {
        let s = self.scale;
        let ((x, y, w, _), _) = self.rect();
        (
            x + w - (PAD + 16.0) * s,
            y + (PAD - 2.0) * s,
            20.0 * s,
            20.0 * s,
        )
    }

    fn suche_rect(&self) -> Rect {
        let s = self.scale;
        let ((x, y, w, _), _) = self.rect();
        (
            x + PAD * s,
            y + (PAD + 22.0 + 16.0 + 8.0) * s,
            w - 2.0 * PAD * s,
            SUCHE_H * s,
        )
    }

    fn liste_y(&self) -> f32 {
        let (_, sy, _, sh) = self.suche_rect();
        sy + sh + 8.0 * self.scale
    }

    fn hit(&self, x: f32, y: f32) -> Option<Ziel> {
        let ((bx, _, bw, _), _) = self.rect();
        if !inside(self.rect().0, x, y) {
            return None;
        }
        if inside(self.schliessen_rect(), x, y) {
            return Some(Ziel::Schliessen);
        }
        if inside(self.suche_rect(), x, y) {
            return Some(Ziel::Suche);
        }
        let s = self.scale;
        let ly = self.liste_y();
        for (i, e, ey, eh) in self.sichtbar() {
            let r = (bx, ly + ey * s, bw, eh * s);
            if inside(r, x, y) && !matches!(e, Eintrag::Leer(..)) {
                return Some(Ziel::Eintrag(i));
            }
        }
        Some(Ziel::Innen)
    }

    /// Name einer Bauleistung, den kein anderer Eintrag enthält (Tests).
    #[cfg(test)]
    pub fn erster_eindeutig(&self) -> Option<String> {
        let alle: Vec<&Wahl> = self.auswahl.alle().collect();
        alle.iter()
            .find(|w| alle.iter().filter(|x| x.kurz.contains(&w.kurz)).count() == 1)
            .map(|w| w.kurz.clone())
    }

    pub fn enthaelt(&self, x: f32, y: f32) -> bool {
        inside(self.rect().0, x, y)
    }

    // --- Ereignisse ----------------------------------------------------------

    fn waehle(&mut self, i: usize) -> Option<Aus> {
        match self.eintraege().into_iter().nth(i)? {
            Eintrag::Wahl(w) => Some(Aus::Waehlen(w.leistung)),
            Eintrag::Weitere(_) => {
                self.weitere = true;
                Some(Aus::Repaint)
            }
            Eintrag::Leer(..) => None,
        }
    }

    pub fn mouse_move(&mut self, x: f32, y: f32) -> bool {
        let hot = self.hit(x, y);
        let look = |h: Option<Ziel>| h.filter(|z| *z != Ziel::Innen);
        let changed = look(hot) != look(self.hot);
        self.hot = hot;
        changed
    }

    /// Klick (px): ein Eintrag ordnet zu, daneben schließt ohne Spur.
    pub fn mouse_down(&mut self, fonts: &Fonts, x: f32, y: f32) -> Option<Aus> {
        match self.hit(x, y) {
            None | Some(Ziel::Schliessen) => Some(Aus::Schliessen),
            Some(Ziel::Suche) => {
                let s = self.scale;
                let (sx, _, _, _) = self.suche_rect();
                let px = 11.0 * s;
                let c = widgets::caret_at(
                    fonts.regular.as_ref(),
                    &self.suche.text,
                    px,
                    sx + 8.0 * s,
                    x,
                );
                self.suche.place(c, false);
                Some(Aus::Repaint)
            }
            Some(Ziel::Eintrag(i)) => self.waehle(i),
            Some(Ziel::Innen) => None,
        }
    }

    pub fn wheel(&mut self, delta: f64) -> bool {
        let n = self.eintraege().len();
        let max = n.saturating_sub(SICHTBAR);
        let neu = if delta > 0.0 {
            self.oben.saturating_sub(1)
        } else {
            (self.oben + 1).min(max)
        };
        let changed = neu != self.oben;
        self.oben = neu;
        changed
    }

    fn suche_neu(&mut self) -> Option<Aus> {
        self.oben = 0;
        Some(Aus::Repaint)
    }

    /// Wählbare Einträge (Wahl und „+ weitere“).
    fn waehlbar(&self) -> Vec<usize> {
        self.eintraege()
            .iter()
            .enumerate()
            .filter(|(_, e)| !matches!(e, Eintrag::Leer(..)))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn key(&mut self, key: Key, mods: Modifiers) -> Option<Aus> {
        let sh = mods.shift;
        match key {
            Key::Escape => return Some(Aus::Schliessen),
            // Enter wählt den einzigen Treffer der Suche
            Key::Enter => {
                let v = self.waehlbar();
                let [i] = v.as_slice() else {
                    return None;
                };
                return self.waehle(*i);
            }
            _ => {}
        }
        let e = &mut self.suche;
        let changed = match key {
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
            self.suche_neu()
        } else {
            Some(Aus::Repaint)
        }
    }

    pub fn text(&mut self, ch: char) -> Option<Aus> {
        if ch.is_control() {
            return None;
        }
        self.suche.insert(ch.encode_utf8(&mut [0; 4]));
        self.suche_neu()
    }

    /// Tooltip: nichts; „kommt dann zu …“ steht am Eintrag selbst.
    pub fn tip_at(&self, _x: f32, _y: f32) -> Option<String> {
        None
    }

    // --- Zeichnen ------------------------------------------------------------

    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts) {
        let s = self.scale;
        let u = &t.ui;
        let ((x, y, w, h), unten) = self.rect();
        flaeche(c, (x, y, w, h), self.anker, unten, s, t);
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let (Some(f), Some(fb)) = (regular, bold) else {
            return;
        };
        let x0 = x + PAD * s;
        let x1 = x + w - PAD * s;
        // Titel, ×, Menge
        let px = 12.0 * s;
        let tb = y + PAD * s + fb.cap_height(px);
        let titel = widgets::ellipsize(Some(fb), &self.titel, px, x1 - x0 - 28.0 * s);
        fb.draw(c, &titel, px, x0, tb.round(), u.sheet_text);
        let (cx, cy, cw, ch) = self.schliessen_rect();
        let col = if self.hot == Some(Ziel::Schliessen) {
            u.sheet_text
        } else {
            u.sheet_text_dim
        };
        let (mx, my, d) = (cx + cw * 0.5, cy + ch * 0.5, 4.0 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
        c.fill(&p, col);
        let px_l = 10.5 * s;
        f.draw(
            c,
            &self.menge,
            px_l,
            x0,
            (y + (PAD + 22.0 + 10.0) * s).round(),
            u.sheet_text_dim,
        );
        // Suchfeld
        let (sx, sy, sw, sh) = self.suche_rect();
        let mut p = Path::new();
        p.rounded_rect(sx, sy, sw, sh, 4.0 * s);
        c.fill(&p, u.accent);
        let b = 1.5 * s;
        let mut p = Path::new();
        p.rounded_rect(sx + b, sy + b, sw - 2.0 * b, sh - 2.0 * b, 4.0 * s - b);
        c.fill(&p, u.sheet_card);
        let px_e = 11.0 * s;
        let base = (sy + (sh + f.cap_height(px_e)) * 0.5).round();
        let tx = sx + 8.0 * s;
        let text = &self.suche.text;
        if text.is_empty() {
            f.draw(c, "Suchen", px_e, tx, base, u.sheet_hint);
        } else {
            let (a, b) = self.suche.selection();
            if a < b {
                let ax = tx + f.width(&text[..a], px_e);
                c.fill_rect(
                    ax,
                    sy + 5.0 * s,
                    f.width(&text[a..b], px_e),
                    sh - 10.0 * s,
                    u.text_select,
                );
            }
            f.draw(c, text, px_e, tx, base, u.sheet_text);
        }
        let kx = (tx + f.width(&text[..self.suche.caret], px_e)).round();
        c.fill_rect(kx, sy + 6.0 * s, s.max(1.0), sh - 12.0 * s, u.sheet_text);
        // Liste
        let ly = self.liste_y();
        let markiert = self.markiert();
        let sichtbar = self.sichtbar();
        if sichtbar.is_empty() {
            f.draw(
                c,
                "Keine Bauleistung passt zur Suche.",
                px_e,
                x0,
                (ly + (EINTRAG * s + f.cap_height(px_e)) * 0.5).round(),
                u.sheet_text_dim,
            );
        }
        for (i, e, ey, eh) in sichtbar {
            let ry = ly + ey * s;
            let zb = (ry + (EINTRAG * s + f.cap_height(px_e)) * 0.5).round();
            if markiert == Some(i) && !matches!(e, Eintrag::Leer(..)) {
                c.fill_rect(x + s, ry, w - 2.0 * s, eh * s, u.sheet_hover);
            }
            match &e {
                Eintrag::Leer(fett, leise) => {
                    fb.draw(c, fett, px_e, x0, (ry + 15.0 * s).round(), u.sheet_text);
                    f.draw(
                        c,
                        leise,
                        px_l,
                        x0,
                        (ry + 31.0 * s).round(),
                        u.sheet_text_dim,
                    );
                }
                Eintrag::Weitere(text) => {
                    fb.draw(c, text, px_e, x0, zb, u.accent);
                }
                Eintrag::Wahl(wl) => {
                    let ep = match wl.ep {
                        Some(ep) => format!("{} €/{}", ep.deutsch(), wl.einheit.zeichen()),
                        None => "ohne Preis".into(),
                    };
                    let ew = f.width(&ep, px_l);
                    f.draw(c, &ep, px_l, x1 - ew, zb, u.sheet_text_dim);
                    let kurz = widgets::ellipsize(Some(f), &wl.kurz, px_e, x1 - x0 - ew - 16.0 * s);
                    f.draw(c, &kurz, px_e, x0, zb, u.sheet_text);
                    if let (Some(g), true) = (&wl.fremd, markiert == Some(i)) {
                        let t = format!("kommt dann zu {g}");
                        f.draw(
                            c,
                            &t,
                            10.0 * s,
                            x0,
                            (zb + FREMD * s).round(),
                            u.sheet_text_dim,
                        );
                    }
                }
            }
        }
        // Fuß
        let fy = y + h - FUSS * s;
        c.fill_rect(x0, fy, x1 - x0, s.max(1.0), u.sheet_rule);
        f.draw(
            c,
            "Klick ordnet zu · Esc schließt",
            10.0 * s,
            x0,
            (fy + 19.0 * s).round(),
            u.sheet_text_dim,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_cost::katalog::Einheit;
    use sk_cost::Cent;

    fn wahl(n: u8, fremd: bool) -> Wahl {
        Wahl {
            leistung: Guid::from_ifc(&format!("{n:0>22}")).unwrap(),
            kurz: format!("Dämmung {n}"),
            einheit: Einheit::M2,
            ep: Some(Cent(1000 + i64::from(n))),
            fremd: fremd.then(|| "WDV-Systeme (DIN 18345)".into()),
        }
    }

    fn blatt() -> WahlBlatt {
        let a = Auswahl {
            eigene: Vec::new(),
            aehnlich: vec![wahl(1, true), wahl(2, true)],
            weitere: vec![wahl(3, true), wahl(4, true)],
            gewerk: "Dachdecker".into(),
            stoffart: Some("Dämmung"),
        };
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model;
        let el = m.elements().iter().next().unwrap().0;
        let mut b = WahlBlatt::neu((0, el), "Dachterrasse", "13,219 m²".into(), a);
        b.fenster = (1200.0, 900.0);
        b.set_anker((600.0, 300.0, 700.0, 322.0));
        b
    }

    /// Ohne Suchtext: Kopfzeile, Ähnliche, „+ 2 weitere in m²“ klappt auf;
    /// Tippen filtert, Enter wählt den einzigen Treffer, Esc schließt.
    #[test]
    fn ordnung_suche_und_wahl() {
        let mut b = blatt();
        let e = b.eintraege();
        assert!(
            matches!(&e[0], Eintrag::Leer(f, _) if f == "Für Dachdecker gibt es noch keine Bauleistung.")
        );
        assert!(matches!(&e[3], Eintrag::Weitere(t) if t == "+ 2 weitere in m²"));
        assert_eq!(b.waehle(3), Some(Aus::Repaint));
        assert_eq!(b.eintraege().len(), 5);
        let mods = Modifiers::default();
        assert_eq!(b.key(Key::Enter, mods), None, "nichts eindeutig");
        for ch in "ung 4".chars() {
            b.text(ch);
        }
        assert_eq!(
            b.key(Key::Enter, mods),
            Some(Aus::Waehlen(wahl(4, true).leistung))
        );
        b.key(Key::Backspace, mods);
        b.key(Key::Backspace, mods);
        assert_eq!(b.eintraege().len(), 4);
        assert_eq!(b.key(Key::Escape, mods), Some(Aus::Schliessen));
    }

    /// Überfahren eines fremden Eintrags macht Platz für „kommt dann zu“;
    /// Klick darauf ordnet zu, Klick daneben schließt.
    #[test]
    fn ueberfahren_und_klick() {
        let mut b = blatt();
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let ((x, _, w, _), _) = b.rect();
        let h0 = b.hoehe();
        let ly = b.liste_y();
        let y = ly + (LEER + EINTRAG * 0.5) * b.scale;
        assert!(b.mouse_move(x + w * 0.5, y));
        assert_eq!(b.markiert(), Some(1));
        assert_eq!(b.hoehe(), h0 + FREMD);
        assert_eq!(
            b.mouse_down(&fonts, x + w * 0.5, y),
            Some(Aus::Waehlen(wahl(1, true).leistung))
        );
        assert_eq!(b.mouse_down(&fonts, 5.0, 5.0), Some(Aus::Schliessen));
    }
}

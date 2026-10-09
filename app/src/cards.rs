//! Kartenleiste über den Blättern des Mengenfensters (KA-2a, paket-ka2 §1
//! und §5, Einstellungen §3 KA-1 Punkte 1–4): eine Karte je Blatt mit ihrer
//! lebenden Zahl. Ein Klick wechselt das Blatt, der Inhalt gleitet in
//! `anim_ms` seitlich, die Karten bleiben stehen. Ändert sich eine Zahl,
//! glimmt sie einmal in Akzent auf; sie zählt nicht hoch.
//!
//! Die Leiste rechnet nichts: Die Zahlen setzt das Fenster aus den Blättern.

use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts};
use std::time::Instant;

/// Ein Blatt des Fensters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blatt {
    #[default]
    Mengen,
    Kosten,
    /// Leistungsverzeichnis (KA-4).
    Ava,
}

impl Blatt {
    /// Die vorhandenen Blätter in der Reihenfolge der Karten.
    pub const ALLE: [Blatt; 3] = [Blatt::Mengen, Blatt::Kosten, Blatt::Ava];

    /// Name auf der Karte und im Fenstertitel.
    pub fn name(self) -> &'static str {
        match self {
            Blatt::Mengen => "Mengen",
            Blatt::Kosten => "Kosten",
            Blatt::Ava => "AVA",
        }
    }

    /// Wort in `einstellungen.txt` (`[mengenfenster] blatt=`).
    pub fn key(self) -> &'static str {
        match self {
            Blatt::Mengen => "mengen",
            Blatt::Kosten => "kosten",
            Blatt::Ava => "ava",
        }
    }

    pub fn from_key(k: &str) -> Option<Blatt> {
        Blatt::ALLE.into_iter().find(|b| b.key() == k)
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Knopf im Paneel „Ansichten“ (Name in Titel und Tooltip); sein Tooltip
/// steht in `hilfe.txt` unter `[knopf]`.
pub const KNOPF: &str = "Mengen · Kosten · AVA";
/// Der Knopf ist ab KA-4 zweizeilig (Einstellungen §3 KA-4 Punkt 8).
pub const KNOPF_ZEILEN: (&str, &str) = ("Mengen", "Kosten · AVA");

/// Karte 236 × 74 dip, Radius 8, Abstand zwischen den Karten.
const CARD_W: f32 = 236.0;
const CARD_H: f32 = 74.0;
const CARD_GAP: f32 = 12.0;
/// Kleinste Kartenbreite (dip), wenn sich die Karten die Breite teilen;
/// so passen sie ab 300 dip Fensterbreite ins Bild (Hinweis L von Test).
const CARD_MIN_W: f32 = 64.0;
/// Ab dieser Kartenbreite (dip) steht das Symbol vor dem Namen.
const CARD_SYMBOL_W: f32 = 120.0;
const RADIUS: f32 = 8.0;
/// Luft über und unter den Karten (dip).
const PAD_TOP: f32 = 14.0;
const PAD_BOTTOM: f32 = 4.0;
/// Höhe der Leiste unter der Titelleiste (dip).
pub const HEIGHT: f32 = PAD_TOP + CARD_H + PAD_BOTTOM;

/// Rechteck in Fensterpixeln: x, y, Breite, Höhe.
type Rect = (f32, f32, f32, f32);

pub struct Karten {
    pub aktiv: Blatt,
    hover: Option<Blatt>,
    zahlen: [String; 3],
    /// Seit wann die geänderte Zahl einer Karte aufglimmt.
    glimm: [Option<Instant>; 3],
    /// Laufender Wechsel: voriges Blatt und Beginn.
    wechsel: Option<(Blatt, Instant)>,
}

impl Karten {
    pub fn new(aktiv: Blatt) -> Karten {
        Karten {
            aktiv,
            hover: None,
            zahlen: Default::default(),
            glimm: [None; 3],
            wechsel: None,
        }
    }

    /// Breite einer Karte (dip): 236, im schmalen Fenster teilen sich die
    /// drei Karten die Breite `breit` (px), aber nicht unter 64.
    fn card_w(s: f32, breit: f32) -> f32 {
        ((breit / s - 2.0 * CARD_GAP) / 3.0).clamp(CARD_MIN_W, CARD_W)
    }

    /// Lage der Karten (px); `x0` linker Rand des Inhalts, `top` Unterkante
    /// der Titelleiste, `breit` verfügbare Breite.
    fn rects(x0: f32, top: f32, s: f32, breit: f32) -> [Rect; 3] {
        let y = top + PAD_TOP * s;
        let cw = Self::card_w(s, breit);
        let r = |i: usize| (x0 + i as f32 * (cw + CARD_GAP) * s, y, cw * s, CARD_H * s);
        [r(0), r(1), r(2)]
    }

    /// Rechtes Ende der dritten Karte (px).
    pub fn ende(x0: f32, s: f32, breit: f32) -> f32 {
        x0 + (3.0 * Self::card_w(s, breit) + 2.0 * CARD_GAP) * s
    }

    /// „Als Tabelle speichern“ rechtsbündig in der Kartenzeile, wenn er mit
    /// 16 dip Abstand rechts neben die Karten passt (Einstellungen, Notiz
    /// Kopf §2): Oberkante (px) relativ zur Unterkante der Karten-Leiste
    /// `blatt_top`, sonst `None`.
    pub fn knopf_y(x0: f32, s: f32, breit: f32, knopf_x: f32, blatt_top: f32) -> Option<f32> {
        (Self::ende(x0, s, breit) + 16.0 * s <= knopf_x)
            .then_some(blatt_top - (HEIGHT - PAD_TOP - 6.0) * s)
    }

    pub fn hit(x0: f32, top: f32, s: f32, breit: f32, x: f64, y: f64) -> Option<Blatt> {
        let (x, y) = (x as f32, y as f32);
        Blatt::ALLE
            .into_iter()
            .zip(Self::rects(x0, top, s, breit))
            .find(|(_, (rx, ry, rw, rh))| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
            .map(|(b, _)| b)
    }

    /// Karte unter der Maus; `true`, wenn sich das Bild ändert.
    pub fn set_hover(&mut self, h: Option<Blatt>) -> bool {
        std::mem::replace(&mut self.hover, h) != h
    }

    /// Blatt wählen; `true`, wenn es ein anderes ist. Mit `anim` gleitet
    /// der Inhalt.
    pub fn waehlen(&mut self, b: Blatt, now: Instant, anim: bool) -> bool {
        if b == self.aktiv {
            return false;
        }
        let vorher = std::mem::replace(&mut self.aktiv, b);
        self.wechsel = anim.then_some((vorher, now));
        true
    }

    pub fn zahl(&self, b: Blatt) -> &str {
        &self.zahlen[b.index()]
    }

    /// Lebende Zahl der Karte setzen. Eine geänderte Zahl glimmt auf (nicht
    /// beim ersten Setzen); `true`, wenn sie sich geändert hat.
    pub fn set_zahl(&mut self, b: Blatt, text: String, now: Instant) -> bool {
        let i = b.index();
        if self.zahlen[i] == text {
            return false;
        }
        if !self.zahlen[i].is_empty() {
            self.glimm[i] = Some(now);
        }
        self.zahlen[i] = text;
        true
    }

    /// Gleiten und Aufglimmen weiterführen; `true`, solange es läuft.
    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        let d = t.size.anim_ms;
        let laeuft = |at: Instant| d > 0.0 && now.duration_since(at).as_secs_f32() * 1000.0 < d;
        let mut busy = false;
        for g in &mut self.glimm {
            match *g {
                Some(at) if laeuft(at) => busy = true,
                _ => *g = None,
            }
        }
        match self.wechsel {
            Some((_, at)) if laeuft(at) => busy = true,
            _ => self.wechsel = None,
        }
        busy
    }

    /// Laufender Wechsel: voriges Blatt und Fortschritt 0..1 (weich).
    pub fn gleiten(&self, t: &Theme, now: Instant) -> Option<(Blatt, f32)> {
        let (vorher, at) = self.wechsel?;
        let d = t.size.anim_ms;
        if d <= 0.0 {
            return None;
        }
        let k = (now.duration_since(at).as_secs_f32() * 1000.0 / d).min(1.0);
        (k < 1.0).then_some((vorher, 1.0 - (1.0 - k).powi(3)))
    }

    /// Richtung des Gleitens: +1, wenn das neue Blatt rechts vom alten liegt.
    pub fn richtung(vorher: Blatt, neu: Blatt) -> f32 {
        if neu.index() > vorher.index() {
            1.0
        } else {
            -1.0
        }
    }

    /// Glimmen der Zahl 0..1 (1 = ganz Akzent).
    fn glimm_von(&self, b: Blatt, t: &Theme, now: Instant) -> f32 {
        let d = t.size.anim_ms;
        match self.glimm[b.index()] {
            Some(at) if d > 0.0 => {
                let k = now.duration_since(at).as_secs_f32() * 1000.0 / d;
                // auf und wieder ab
                (1.0 - (2.0 * k - 1.0).abs()).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Die Leiste zeichnen (Fläche `sheet_bg` darunter zeichnet das Fenster).
    pub fn paint(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        (x0, top, s, breit): (f32, f32, f32, f32),
        now: Instant,
    ) {
        let u = &t.ui;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let line = s.max(1.0);
        for (b, (x, y, w, h)) in Blatt::ALLE.into_iter().zip(Self::rects(x0, top, s, breit)) {
            let aktiv = b == self.aktiv;
            let hover = self.hover == Some(b) && !aktiv;
            let r = RADIUS * s;
            if aktiv {
                // feiner Schatten
                let mut p = Path::new();
                p.rounded_rect(x, y + line, w, h, r);
                c.fill(&p, u.shadow);
            }
            let fill = if aktiv || hover {
                u.sheet_card
            } else {
                u.sheet_tile
            };
            let mut p = Path::new();
            p.rounded_rect(x, y, w, h, r);
            if hover {
                c.fill(&p, u.sheet_rule);
                let mut p = Path::new();
                p.rounded_rect(x + line, y + line, w - 2.0 * line, h - 2.0 * line, r - line);
                c.fill(&p, fill);
            } else {
                c.fill(&p, fill);
            }
            if aktiv {
                let inset = 14.0 * s;
                c.fill_rect(
                    x + inset,
                    y + h - 3.0 * s,
                    w - 2.0 * inset,
                    3.0 * s,
                    u.accent,
                );
            }
            // Auf der weißen aktiven Karte steht die Zahl in voller Farbe
            let (text, dim) = if aktiv {
                (u.sheet_text, u.sheet_text)
            } else {
                (u.sheet_text_dim, u.sheet_text_dim)
            };
            // Schmale Karte: ohne Symbol, der Name rückt nach links
            let name_x = if w >= CARD_SYMBOL_W * s {
                let icon = if aktiv { u.accent } else { u.sheet_text_dim };
                symbol(c, b, (x + 16.0 * s, y + 14.0 * s), s, icon);
                42.0 * s
            } else {
                16.0 * s
            };
            if let Some(f) = bold {
                let name = widgets::ellipsize(Some(f), b.name(), 14.0 * s, w - name_x - 16.0 * s);
                f.draw(c, &name, 14.0 * s, x + name_x, y + 28.0 * s, text);
            }
            // Aktive Karte Kosten: Betrag fett wie im Soll KA-2; Mengen und
            // AVA bleiben regulär wie in KA-1 und KA-4 (spaeter-darstellung 4)
            if let Some(f) = if aktiv && b == Blatt::Kosten {
                bold
            } else {
                regular
            } {
                let g = self.glimm_von(b, t, now);
                let zahl = widgets::ellipsize(Some(f), self.zahl(b), 12.5 * s, w - 32.0 * s);
                f.draw(
                    c,
                    &zahl,
                    12.5 * s,
                    x + 16.0 * s,
                    y + 54.0 * s,
                    mix(dim, u.accent, g),
                );
            }
        }
    }
}

/// Verweis auf dem hellen Blatt (Einstellungen §4): fett in
/// mix(`accent`, `sheet_text`, 0,2), unter der Maus 0,4. Reines `accent` hat
/// auf `sheet_bg` nur etwa 1,8:1.
pub fn verweis(u: &sk_ui::theme::Ui, hot: bool) -> Rgba {
    mix(u.accent, u.sheet_text, if hot { 0.4 } else { 0.2 })
}

/// Farbe zwischen `a` und `b` (0 = a).
fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    let (a, b) = (a.to_f32(), b.to_f32());
    Rgba::from_f32(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k))
}

/// Gezeichnetes Symbol 16 dip, Strich 1 dip: Raster (Mengen), € (Kosten), Liste (AVA).
fn symbol(c: &mut Canvas, b: Blatt, (x, y): (f32, f32), s: f32, col: Rgba) {
    let w = s.max(1.0);
    let d = 16.0 * s;
    let mut p = Path::new();
    match b {
        Blatt::Mengen => {
            // Vier Kacheln 2 × 2 wie im Soll (spaeter-darstellung 4)
            let k = d * 0.4;
            for (ox, oy) in [(0.0, 0.0), (0.6, 0.0), (0.0, 0.6), (0.6, 0.6)] {
                let (x0, y0) = (x + ox * d, y + oy * d);
                p.segment((x0, y0), (x0 + k, y0), w);
                p.segment((x0 + k, y0), (x0 + k, y0 + k), w);
                p.segment((x0 + k, y0 + k), (x0, y0 + k), w);
                p.segment((x0, y0 + k), (x0, y0), w);
            }
        }
        Blatt::Kosten => {
            // Bogen von 45° bis 315° um die Mitte, dazu zwei Querstriche;
            // kräftiger Strich wie im Soll
            let w = 1.8 * s.max(1.0);
            let (cx, cy, r) = (x + d * 0.58, y + d * 0.5, d * 0.42);
            let n = 16;
            let at = |i: usize| {
                let a = (45.0 + 270.0 * i as f32 / n as f32).to_radians();
                (cx + r * a.cos(), cy - r * a.sin())
            };
            for i in 0..n {
                p.segment(at(i), at(i + 1), w);
            }
            let x1 = x + d * 0.1;
            let x2 = x + d * 0.68;
            p.segment((x1, cy - d * 0.12), (x2, cy - d * 0.12), w);
            p.segment((x1, cy + d * 0.12), (x2, cy + d * 0.12), w);
        }
        Blatt::Ava => {
            // Liste: drei Zeilen mit Punkt davor
            for k in 0..3 {
                let yy = y + d * (0.2 + 0.3 * k as f32);
                p.segment((x, yy), (x + d * 0.12, yy), w);
                p.segment((x + d * 0.3, yy), (x + d, yy), w);
            }
        }
    }
    c.fill(&p, col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blaetter_und_zahlen() {
        assert_eq!(Blatt::from_key("kosten"), Some(Blatt::Kosten));
        assert_eq!(Blatt::from_key("ava"), Some(Blatt::Ava));
        let now = Instant::now();
        let mut k = Karten::new(Blatt::Mengen);
        // erstes Setzen glimmt nicht, eine Änderung schon, gleiche Zahl nicht
        assert!(k.set_zahl(Blatt::Kosten, "60.090 € netto".into(), now));
        assert!(k.glimm[1].is_none());
        assert!(!k.set_zahl(Blatt::Kosten, "60.090 € netto".into(), now));
        assert!(k.set_zahl(Blatt::Kosten, "62.502 € netto".into(), now));
        assert!(k.glimm[1].is_some() && k.glimm[0].is_none());
        // Wechsel nach rechts, nochmals dasselbe Blatt ändert nichts
        assert!(k.waehlen(Blatt::Kosten, now, true));
        assert!(!k.waehlen(Blatt::Kosten, now, true));
        assert_eq!(Karten::richtung(Blatt::Mengen, Blatt::Kosten), 1.0);
        // Treffer: zweite Karte rechts neben der ersten
        let (x0, top, s, breit) = (24.0, 32.0, 1.0, 1200.0);
        let y = (top + PAD_TOP + 10.0) as f64;
        assert_eq!(Karten::hit(x0, top, s, breit, 30.0, y), Some(Blatt::Mengen));
        assert_eq!(
            Karten::hit(x0, top, s, breit, (x0 + CARD_W + CARD_GAP + 5.0) as f64, y),
            Some(Blatt::Kosten)
        );
        assert_eq!(
            Karten::hit(x0, top, s, breit, (x0 + CARD_W + 4.0) as f64, y),
            None
        );
        // Schmal (Notiz Kopf, ist-mengen-schmal, Hinweis L: 960 und 720 px
        // bei 150 %): die Karten teilen sich die Breite und bleiben im
        // Fenster, nicht unter 64 dip
        for (breit, s) in [
            (780.0, 1.0),
            (656.0, 1.25),
            (500.0, 1.0),
            (1356.0, 1.5),
            (636.0, 1.5),
            (244.0, 1.0),
        ] {
            let r = Karten::rects(x0, top, s, breit);
            assert!(r[2].2 >= CARD_MIN_W * s - 0.01);
            if breit / s >= 3.0 * CARD_MIN_W + 2.0 * CARD_GAP {
                assert!(r[2].0 + r[2].2 <= x0 + breit + 0.01, "{breit} {s}");
            }
        }
    }
}

//! Kartenleiste über den Blättern des Mengenfensters (KA-2a, paket-ka2 §1
//! und §5, Einstellungen §3 KA-1 Punkte 1–4): eine Karte je Blatt mit ihrer
//! lebenden Zahl. Ein Klick wechselt das Blatt, der Inhalt gleitet in
//! `anim_ms` seitlich, die Karten bleiben stehen. Ändert sich eine Zahl,
//! glimmt sie einmal in Akzent auf; sie zählt nicht hoch.
//!
//! Die Leiste rechnet nichts: Die Zahlen setzt das Fenster aus den Blättern.
//!
//! Die Wohnfläche steht als zweite Karte neben den Mengen (Jörn 13.10.
//! 02:14: „als gesonderte Rubrik in Mengen-Auswertung“).

use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts};
use std::time::Instant;

/// Ein Blatt des Fensters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blatt {
    #[default]
    Mengen,
    /// Wohnflächenberechnung nach WoFlV (G8).
    Wohnflaeche,
    Kosten,
    /// Leistungsverzeichnis (KA-4).
    Ava,
}

impl Blatt {
    /// Die vorhandenen Blätter in der Reihenfolge der Karten.
    pub const ALLE: [Blatt; 4] = [Blatt::Mengen, Blatt::Wohnflaeche, Blatt::Kosten, Blatt::Ava];

    /// Name auf der Karte und im Fenstertitel.
    pub fn name(self) -> &'static str {
        match self {
            Blatt::Mengen => "Mengen",
            Blatt::Wohnflaeche => "Wohnfläche",
            Blatt::Kosten => "Kosten",
            Blatt::Ava => "AVA",
        }
    }

    /// Wort in `einstellungen.txt` (`[mengenfenster] blatt=`).
    pub fn key(self) -> &'static str {
        match self {
            Blatt::Mengen => "mengen",
            Blatt::Wohnflaeche => "wohnflaeche",
            Blatt::Kosten => "kosten",
            Blatt::Ava => "ava",
        }
    }

    pub fn from_key(k: &str) -> Option<Blatt> {
        Blatt::ALLE.into_iter().find(|b| b.key() == k)
    }

    pub(crate) fn index(self) -> usize {
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
/// Speichern-Knopf rechts in der Kartenzeile, in Mengen, Kosten und AVA
/// gleich groß, damit er beim Reiterwechsel nicht springt: Höhe und
/// Innenabstand (dip).
pub const KNOPF_H: f32 = 26.0;
pub const KNOPF_PAD: f32 = 12.0;

/// Rechteck in Fensterpixeln: x, y, Breite, Höhe.
type Rect = (f32, f32, f32, f32);

/// Anzahl der Karten.
const N: usize = Blatt::ALLE.len();

pub struct Karten {
    pub aktiv: Blatt,
    /// Freie Blätter von links: eine bis drei Karten.
    n: usize,
    hover: Option<Blatt>,
    zahlen: [String; N],
    /// Seit wann die geänderte Zahl einer Karte aufglimmt.
    glimm: [Option<Instant>; N],
    /// Laufender Wechsel: voriges Blatt und Beginn.
    wechsel: Option<(Blatt, Instant)>,
    /// Text je Blatt, den eine gesperrte Karte statt ihrer Zahl zeigt. Die
    /// App gibt ihn mit `set_frei`.
    stufen: [&'static str; N],
}

impl Karten {
    pub fn new(aktiv: Blatt) -> Karten {
        Karten {
            aktiv,
            n: Blatt::ALLE.len(),
            hover: None,
            zahlen: Default::default(),
            glimm: [None; N],
            wechsel: None,
            stufen: [""; N],
        }
    }

    /// Breite einer Karte (dip): 236, im schmalen Fenster teilen sich die
    /// Karten die Breite `breit` (px), aber nicht unter 64.
    fn card_w(s: f32, breit: f32) -> f32 {
        let n = N as f32;
        ((breit / s - (n - 1.0) * CARD_GAP) / n).clamp(CARD_MIN_W, CARD_W)
    }

    /// Lage der Karten (px); `x0` linker Rand des Inhalts, `top` Unterkante
    /// der Titelleiste, `breit` verfügbare Breite.
    fn rects(x0: f32, top: f32, s: f32, breit: f32) -> [Rect; N] {
        let y = top + PAD_TOP * s;
        let cw = Self::card_w(s, breit);
        std::array::from_fn(|i| (x0 + i as f32 * (cw + CARD_GAP) * s, y, cw * s, CARD_H * s))
    }

    /// Rechtes Ende der letzten Karte (px).
    pub fn ende(x0: f32, s: f32, breit: f32) -> f32 {
        let n = N as f32;
        x0 + (n * Self::card_w(s, breit) + (n - 1.0) * CARD_GAP) * s
    }

    /// „Als Tabelle speichern“ rechtsbündig in der Kartenzeile, wenn er mit
    /// 16 dip Abstand rechts neben die Karten passt (Einstellungen, Notiz
    /// Kopf §2): Oberkante (px) relativ zur Unterkante der Karten-Leiste
    /// `blatt_top`, sonst `None`.
    pub fn knopf_y(x0: f32, s: f32, breit: f32, knopf_x: f32, blatt_top: f32) -> Option<f32> {
        (Self::ende(x0, s, breit) + 16.0 * s <= knopf_x)
            .then_some(blatt_top - (HEIGHT - PAD_TOP - 6.0) * s)
    }

    /// Die ersten `n` Blätter sind frei (mindestens eins),
    /// die übrigen stehen blass mit ihrer Stufe `stufe` da; ein gesperrtes
    /// aktives Blatt fällt auf das erste zurück.
    pub fn set_frei(&mut self, n: usize, stufe: impl Fn(Blatt) -> &'static str) {
        self.n = n.clamp(1, Blatt::ALLE.len());
        self.stufen = Blatt::ALLE.map(stufe);
        if !self.ist_frei(self.aktiv) {
            (self.aktiv, self.wechsel) = (Blatt::Mengen, None);
        }
    }

    /// Gemerktes Blatt aus den Einstellungen; ein gesperrtes fällt auf das
    /// erste zurück.
    pub fn merken(&mut self, b: Blatt) {
        self.aktiv = if self.ist_frei(b) { b } else { Blatt::Mengen };
    }

    fn ist_frei(&self, b: Blatt) -> bool {
        b.index() < self.n
    }

    /// Die freien Blätter, nur sie nehmen Hover und Klick an.
    fn blaetter(&self) -> impl Iterator<Item = Blatt> {
        Blatt::ALLE.into_iter().take(self.n)
    }

    pub fn hit(&self, x0: f32, top: f32, s: f32, breit: f32, x: f64, y: f64) -> Option<Blatt> {
        let (x, y) = (x as f32, y as f32);
        self.blaetter()
            .zip(Self::rects(x0, top, s, breit))
            .find(|(_, (rx, ry, rw, rh))| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
            .map(|(b, _)| b)
    }

    /// Liegt `(x, y)` auf einer Karte, auch einer blassen? Ein Klick dort
    /// geht nicht an die Liste darunter (Review #156).
    pub fn auf_karte(x0: f32, top: f32, s: f32, breit: f32, x: f64, y: f64) -> bool {
        let (x, y) = (x as f32, y as f32);
        Self::rects(x0, top, s, breit)
            .iter()
            .any(|(rx, ry, rw, rh)| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
    }

    /// Karte unter der Maus; `true`, wenn sich das Bild ändert.
    pub fn set_hover(&mut self, h: Option<Blatt>) -> bool {
        std::mem::replace(&mut self.hover, h) != h
    }

    /// Blatt wählen; `true`, wenn es ein anderes ist. Mit `anim` gleitet
    /// der Inhalt.
    pub fn waehlen(&mut self, b: Blatt, now: Instant, anim: bool) -> bool {
        if b == self.aktiv || !self.ist_frei(b) {
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
        // Gesperrte Blätter (Bedienbarkeit 41.5): blasse Kachel ohne Hover,
        // statt der Zahl die Stufe, ab der es sie gibt
        let blass = mix(u.sheet_text_dim, u.sheet_tile, 0.45);
        for (b, (x, y, w, h)) in Blatt::ALLE.into_iter().zip(Self::rects(x0, top, s, breit)) {
            let frei = self.ist_frei(b);
            let aktiv = b == self.aktiv;
            let hover = frei && self.hover == Some(b) && !aktiv;
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
            } else if frei {
                (u.sheet_text_dim, u.sheet_text_dim)
            } else {
                (blass, blass)
            };
            // Schmale Karte: ohne Symbol, der Name rückt nach links
            let name_x = if w >= CARD_SYMBOL_W * s {
                let icon = if aktiv { u.accent } else { dim };
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
                let zahl = if frei {
                    self.zahl(b)
                } else {
                    self.stufen[b.index()]
                };
                let zahl = widgets::ellipsize(Some(f), zahl, 12.5 * s, w - 32.0 * s);
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

/// Verweis auf dem Blatt (Einstellungen §4): fett in mix(`accent`,
/// `sheet_text`, 0,2), unter der Maus 0,4. Auf dem früheren hellen Blatt
/// hatte reines `accent` nur etwa 1,8:1; die Mischung trägt beide Schemata.
pub fn verweis(u: &sk_ui::theme::Ui, hot: bool) -> Rgba {
    mix(u.accent, u.sheet_text, if hot { 0.4 } else { 0.2 })
}

/// Fläche eines Knopfs auf dem Blatt („Als Tabelle speichern“): gehoben wie
/// die aktive Karte, unter der Maus heller, gedrückt eingesenkt. Schrift
/// `sheet_text`.
pub fn knopf(u: &sk_ui::theme::Ui, gedrueckt: bool, hot: bool) -> Rgba {
    if gedrueckt {
        u.sheet_tile
    } else if hot {
        mix(u.sheet_card, u.sheet_text, 0.12)
    } else {
        u.sheet_card
    }
}

/// Farbe zwischen `a` und `b` (0 = a).
fn mix(a: Rgba, b: Rgba, k: f32) -> Rgba {
    let (a, b) = (a.to_f32(), b.to_f32());
    Rgba::from_f32(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k))
}

/// Gezeichnetes Symbol 16 dip, Strich 1 dip: Raster (Mengen), Grundriss
/// (Wohnfläche), € (Kosten), Liste (AVA).
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
        Blatt::Wohnflaeche => {
            // Grundriss: Umriss mit einer Innenwand, zwei Räume
            let (x1, y1) = (x + d, y + d);
            let (xm, ym) = (x + d * 0.55, y + d * 0.55);
            p.segment((x, y), (x1, y), w);
            p.segment((x1, y), (x1, y1), w);
            p.segment((x1, y1), (x, y1), w);
            p.segment((x, y1), (x, y), w);
            p.segment((xm, y), (xm, ym), w);
            p.segment((xm, ym), (x1, ym), w);
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
        assert!(k.glimm[Blatt::Kosten.index()].is_none());
        assert!(!k.set_zahl(Blatt::Kosten, "60.090 € netto".into(), now));
        assert!(k.set_zahl(Blatt::Kosten, "62.502 € netto".into(), now));
        assert!(k.glimm[Blatt::Kosten.index()].is_some() && k.glimm[0].is_none());
        // Wechsel nach rechts, nochmals dasselbe Blatt ändert nichts
        assert!(k.waehlen(Blatt::Kosten, now, true));
        assert!(!k.waehlen(Blatt::Kosten, now, true));
        assert_eq!(Karten::richtung(Blatt::Mengen, Blatt::Kosten), 1.0);
        // Treffer: Wohnfläche rechts neben Mengen, dann Kosten
        assert_eq!(Blatt::from_key("wohnflaeche"), Some(Blatt::Wohnflaeche));
        let (x0, top, s, breit) = (24.0, 32.0, 1.0, 1200.0);
        let y = (top + PAD_TOP + 10.0) as f64;
        assert_eq!(k.hit(x0, top, s, breit, 30.0, y), Some(Blatt::Mengen));
        let zweite = (x0 + CARD_W + CARD_GAP + 5.0) as f64;
        let dritte = (x0 + 2.0 * (CARD_W + CARD_GAP) + 5.0) as f64;
        assert_eq!(
            k.hit(x0, top, s, breit, zweite, y),
            Some(Blatt::Wohnflaeche)
        );
        assert_eq!(k.hit(x0, top, s, breit, dritte, y), Some(Blatt::Kosten));
        assert_eq!(
            k.hit(x0, top, s, breit, (x0 + CARD_W + 4.0) as f64, y),
            None
        );
        // Nur Mengen frei (Pro): Mengen und Wohnfläche, Kosten fällt auf
        // Mengen zurück und lässt sich nicht wählen
        k.set_frei(2, |b| if b == Blatt::Ava { "Ultimate" } else { "" });
        assert_eq!(k.stufen[Blatt::Ava.index()], "Ultimate");
        assert_eq!(k.aktiv, Blatt::Mengen);
        assert_eq!(
            k.hit(x0, top, s, breit, zweite, y),
            Some(Blatt::Wohnflaeche)
        );
        assert_eq!(k.hit(x0, top, s, breit, dritte, y), None);
        assert!(Karten::auf_karte(x0, top, s, breit, dritte, y));
        assert!(!Karten::auf_karte(x0, top, s, breit, dritte, y + 200.0));
        assert!(!k.waehlen(Blatt::Ava, now, false));
        assert_eq!(
            k.blaetter().collect::<Vec<_>>(),
            [Blatt::Mengen, Blatt::Wohnflaeche]
        );
        k.set_frei(4, |_| "");
        assert!(k.waehlen(Blatt::Ava, now, false));
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
            let n = N as f32;
            assert!(r[N - 1].2 >= CARD_MIN_W * s - 0.01);
            if breit / s >= n * CARD_MIN_W + (n - 1.0) * CARD_GAP {
                assert!(r[N - 1].0 + r[N - 1].2 <= x0 + breit + 0.01, "{breit} {s}");
            }
        }
    }
}

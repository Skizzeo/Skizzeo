//! Verwaltungskennwort (KA-3b1, Einstellungen §3 KA-3 Punkt 9,
//! soll-ka-3c-kennwort): das Blatt „Verwaltungskennwort setzen“ über der
//! Verwaltung und die Abfrage „Verwaltung öffnen“ an einem Platz, an dem das
//! Kennwort noch nicht eingegeben ist. Die Abfrage ist ein kleines Blatt
//! ohne Abdunkeln: Das Fenster ist dann nur so groß wie sie.
//!
//! Den Weg über `pw=` nennt nur die Hilfe, nie der Bildschirm
//! (Bedienbarkeit 5.2).

use super::*;
use sk_cost::verwaltung::Pruefwert;

/// Abfrage „Verwaltung öffnen“ (dip).
pub(super) const ABF_W: f32 = 540.0;
pub(super) const ABF_H: f32 = 118.0;
/// Blatt „Verwaltungskennwort setzen“ (dip).
const SETZ_W: f32 = 560.0;
const SETZ_H: f32 = 390.0;
const PAD: f32 = 22.0;

pub(super) const FALSCH: &str = "Kennwort stimmt nicht.";
pub(super) const VERGESSEN_NUTZER: &str = "Vergessen? Frag deinen BIM-Administrator.";
const WIRKUNG: &str = "Mit Kennwort ändert nur, wer es kennt, den Firmenkatalog. Änderungen sammeln sich in einem Entwurf. Erst „Freigeben“ macht sie gültig, an allen Plätzen. Projekte bleiben auf ihrem Stand, bis sie übernehmen.";
const LEISE: &str = "Schutz vor Versehen, keine Sicherheit. Vergessen? Die Hilfe sagt, wie man es zurücksetzt. Leeres Kennwort heißt: zurück zum Einzelplatz. Setzen und Entfernen stehen im Protokoll.";
pub(super) const VERSCHIEDEN: &str = "Die beiden Eingaben sind verschieden.";
#[cfg(test)]
pub(super) const SAETZE: [&str; 5] = [FALSCH, VERGESSEN_NUTZER, WIRKUNG, LEISE, VERSCHIEDEN];

/// Abfrage beim Öffnen.
#[derive(Default)]
pub(super) struct Abfrage {
    pub te: TextEdit,
    pub falsch: bool,
    pub hover: bool,
    pub pressed: bool,
}

/// Blatt „Verwaltungskennwort setzen“.
#[derive(Default)]
pub(super) struct Setzen {
    pub a: TextEdit,
    pub b: TextEdit,
    /// Fokus im Feld „Noch einmal“.
    pub zweit: bool,
    pub verschieden: bool,
    pub hover: Option<SZiel>,
    pub pressed: Option<SZiel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SZiel {
    Erstes,
    Zweites,
    Abbrechen,
    Setzen,
    Schliessen,
}

/// Punkte statt Zeichen; Schreibmarke hinter dem letzten Punkt.
fn punkte(c: &mut Canvas, r: Rect, n: usize, fokus: bool, s: f32, t: &Theme) {
    let u = &t.ui;
    let d = 7.0 * s;
    let x0 = r.x + t.size.field_pad * s;
    let cy = r.y + r.h * 0.5;
    for i in 0..n {
        let x = x0 + i as f32 * (d + 4.0 * s);
        if x + d > r.x + r.w - 6.0 * s {
            break;
        }
        let mut p = Path::new();
        p.rounded_rect(x, cy - d * 0.5, d, d, d * 0.5);
        c.fill(&p, u.text);
    }
    if fokus {
        let x = (x0 + n as f32 * (d + 4.0 * s)).min(r.x + r.w - 6.0 * s);
        c.fill_rect(x.round(), r.y + 7.0 * s, s.max(1.0), r.h - 14.0 * s, u.text);
    }
}

fn feld_rahmen(
    c: &mut Canvas,
    fonts: &Fonts,
    r: Rect,
    fokus: bool,
    falsch: bool,
    s: f32,
    t: &Theme,
) {
    let st = FieldState {
        text: "",
        focus: fokus,
        invalid: falsch,
        ..Default::default()
    };
    widgets::text_field(c, fonts, r, &st, s, t);
}

/// Taste in einem Kennwortfeld; `true`, wenn sie dort etwas getan hat.
fn taste(te: &mut TextEdit, key: Key, mods: Modifiers) -> bool {
    match key {
        Key::Backspace => te.backspace(),
        Key::Delete => te.delete(),
        Key::Left => te.left(mods.shift),
        Key::Right => te.right(mods.shift),
        Key::Home => te.home(mods.shift),
        Key::End => te.end(mods.shift),
        Key::Char('A') if mods.ctrl => te.select_all(),
        _ => return false,
    }
    true
}

impl Verwaltung {
    /// Mit Verwaltungskennwort an einem Platz ohne Eingabe: erst die
    /// Abfrage.
    pub fn sperren(&mut self) {
        self.abfrage = Some(Abfrage::default());
    }

    // --- Abfrage --------------------------------------------------------------

    fn abf_feld(&self, w: &Win) -> Rect {
        self.r(w, 110.0, 52.0, 260.0, 32.0)
    }

    fn abf_knopf(&self, w: &Win) -> Rect {
        let (ww, _) = self.dip(w);
        self.r(w, ww - PAD - 110.0, 52.0, 110.0, 32.0)
    }

    fn abf_oeffnen(&mut self, out: &mut Out) {
        let Some(a) = self.abfrage.as_mut() else {
            return;
        };
        // Es gilt das freigegebene Kennwort, nicht eines im Entwurf
        let lib = self.freigabe.as_ref().map_or(&self.lib0, |f| &f.lib);
        if sk_cost::verwaltung::kennwort_stimmt(lib, &a.te.text) {
            self.abfrage = None;
            self.pos = None;
            out.frei = true;
            out.moved = true;
        } else {
            a.falsch = true;
            a.te = TextEdit::new("");
        }
        out.repaint = true;
    }

    pub(super) fn abfrage_handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out::default();
        let knopf = self.abf_knopf(&cx.win);
        let Some(a) = self.abfrage.as_mut() else {
            return out;
        };
        match *e {
            Event::MouseMove { x, y, .. } => {
                let h = knopf.contains(x, y);
                if h != a.hover {
                    a.hover = h;
                    out.repaint = true;
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                a.pressed = knopf.contains(x, y);
                out.repaint = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let p = std::mem::take(&mut a.pressed);
                if p && knopf.contains(x, y) {
                    self.abf_oeffnen(&mut out);
                }
                out.repaint = true;
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => match key {
                Key::Escape => out.closed = true,
                Key::Enter => self.abf_oeffnen(&mut out),
                k => {
                    if taste(&mut a.te, k, mods) {
                        a.falsch = false;
                        out.repaint = true;
                    }
                }
            },
            Event::Text(c) if !c.is_control() => {
                a.te.insert(&c.to_string());
                a.falsch = false;
                out.repaint = true;
            }
            _ => {}
        }
        out
    }

    pub(super) fn abfrage_malen(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some(a) = self.abfrage.as_ref() else {
            return;
        };
        let (s, u) = (w.scale, &t.ui);
        let f = self.frame(w);
        let (ww, _) = self.dip(w);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        if let Some(b) = bold {
            b.draw(
                c,
                "Verwaltung öffnen",
                14.0 * s,
                f.x + PAD * s,
                f.y + 32.0 * s,
                u.text,
            );
        }
        if let Some(r) = regular {
            let stand = self.titel_stand();
            let px = 12.0 * s;
            let x = f.x + (ww - PAD) * s - r.width(&stand, px);
            r.draw(c, &stand, px, x, f.y + 32.0 * s, u.text_dim);
            let fy = f.y + 52.0 * s;
            let base = fy + (32.0 * s + r.cap_height(13.0 * s)) * 0.5;
            r.draw(
                c,
                "Kennwort",
                13.0 * s,
                f.x + PAD * s,
                base.round(),
                u.text_dim,
            );
        }
        let fr = self.abf_feld(w);
        feld_rahmen(c, fonts, fr, true, a.falsch, s, t);
        punkte(c, fr, a.te.text.chars().count(), true, s, t);
        let st = ButtonState {
            hover: a.hover,
            pressed: a.pressed,
            active: true,
            disabled: false,
        };
        widgets::button(c, fonts, self.abf_knopf(w), "Öffnen", st, s, t);
        if !sk_cost::verwaltung::kennwort_lesbar(&self.lib0) {
            if let Some(r) = regular {
                let text = sk_cost::befund::r72_pw();
                let px = 12.0 * s;
                let text = widgets::ellipsize(Some(r), &text, px, (ww - 2.0 * PAD) * s);
                r.draw(
                    c,
                    &text,
                    px,
                    f.x + PAD * s,
                    f.y + 102.0 * s,
                    u.field_invalid,
                );
            }
        } else if a.falsch {
            let y = f.y + 102.0 * s;
            let mut x = fr.x;
            if let Some(b) = bold {
                b.draw(c, FALSCH, 12.0 * s, x, y, u.field_invalid);
                x += b.width(FALSCH, 12.0 * s) + 8.0 * s;
            }
            if let Some(r) = regular {
                r.draw(c, VERGESSEN_NUTZER, 12.0 * s, x, y, u.text_dim);
            }
        }
    }

    /// „Stand 3“ für die Abfrage.
    fn titel_stand(&self) -> String {
        match self.vorher.kopf.as_ref() {
            Some(k) if k.stand > 0 => format!("Stand {}", k.stand),
            _ => String::new(),
        }
    }

    // --- Blatt „Verwaltungskennwort setzen“ -----------------------------------

    /// Blatt in der Mitte des Fensters (px).
    fn setz_rect(&self, w: &Win) -> Rect {
        let (ww, hh) = self.dip(w);
        self.r(w, (ww - SETZ_W) * 0.5, (hh - SETZ_H) * 0.5, SETZ_W, SETZ_H)
    }

    /// Teile des Blatts (px).
    fn setz_teile(&self, w: &Win) -> [(SZiel, Rect); 5] {
        let r = self.setz_rect(w);
        let s = w.scale;
        let at = |x: f32, y: f32, ww: f32, hh: f32| {
            Rect::new(
                (r.x + x * s).round(),
                (r.y + y * s).round(),
                (ww * s).round(),
                (hh * s).round(),
            )
        };
        [
            (SZiel::Erstes, at(200.0, 156.0, 220.0, 32.0)),
            (SZiel::Zweites, at(200.0, 198.0, 220.0, 32.0)),
            (
                SZiel::Abbrechen,
                at(SETZ_W - 312.0, SETZ_H - 54.0, 120.0, 34.0),
            ),
            (
                SZiel::Setzen,
                at(SETZ_W - 182.0, SETZ_H - 54.0, 160.0, 34.0),
            ),
            (SZiel::Schliessen, at(SETZ_W - 44.0, 14.0, 28.0, 28.0)),
        ]
    }

    fn setz_hit(&self, w: &Win, x: f64, y: f64) -> Option<SZiel> {
        self.setz_teile(w)
            .into_iter()
            .find(|(_, r)| r.contains(x, y))
            .map(|(z, _)| z)
    }

    /// „Kennwort setzen“: beide Eingaben gleich, dann eine Operation; leer
    /// heißt zurück zum Einzelplatz.
    fn setz_fertig(&mut self) {
        let Some(sb) = self.setz.as_mut() else {
            return;
        };
        if sb.a.text != sb.b.text {
            sb.verschieden = true;
            sb.zweit = true;
            return;
        }
        // Salz je Kennwort vom System (Review 3at); nur der Prüfwert geht in
        // die Operation
        let mut salz = [0u8; 16];
        if let Err(e) = sk_platform::zufall(&mut salz) {
            self.meldung = Some(format!(
                "Kennwort nicht gesetzt: kein Zufall vom System ({e})."
            ));
            self.setz = None;
            return;
        }
        let pw = Pruefwert::neu(&sb.a.text, salz);
        self.setz = None;
        // Ohne Kennwort ist „leer“ nichts zu tun
        let gesetzt = sk_cost::verwaltung::hat_kennwort(&self.lib0);
        self.ops.retain(|o| !matches!(o, Op::KennwortSetzen { .. }));
        if gesetzt || !pw.ist_leer() {
            self.ops.push(Op::KennwortSetzen { pw });
        }
        self.neu_rechnen();
    }

    pub(super) fn setz_handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = Out {
            repaint: true,
            ..Default::default()
        };
        let h = |v: &Self, x, y| v.setz_hit(&cx.win, x, y);
        match *e {
            Event::MouseMove { x, y, .. } => {
                let z = h(self, x, y);
                if let Some(sb) = self.setz.as_mut() {
                    out.repaint = sb.hover != z;
                    sb.hover = z;
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let z = h(self, x, y);
                if let Some(sb) = self.setz.as_mut() {
                    match z {
                        Some(SZiel::Erstes) => sb.zweit = false,
                        Some(SZiel::Zweites) => sb.zweit = true,
                        _ => sb.pressed = z,
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let z = h(self, x, y);
                let p = self.setz.as_mut().and_then(|sb| sb.pressed.take());
                if p.is_some() && p == z {
                    match p {
                        Some(SZiel::Setzen) => self.setz_fertig(),
                        Some(SZiel::Abbrechen | SZiel::Schliessen) => self.setz = None,
                        _ => {}
                    }
                }
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => match key {
                Key::Escape => self.setz = None,
                Key::Enter => {
                    let zweit = self.setz.as_ref().is_some_and(|sb| sb.zweit);
                    if zweit {
                        self.setz_fertig();
                    } else if let Some(sb) = self.setz.as_mut() {
                        sb.zweit = true;
                    }
                }
                Key::Tab => {
                    if let Some(sb) = self.setz.as_mut() {
                        sb.zweit = !sb.zweit;
                    }
                }
                k => {
                    if let Some(sb) = self.setz.as_mut() {
                        let te = if sb.zweit { &mut sb.b } else { &mut sb.a };
                        if taste(te, k, mods) {
                            sb.verschieden = false;
                        }
                    }
                }
            },
            Event::Text(c) if !c.is_control() => {
                if let Some(sb) = self.setz.as_mut() {
                    let te = if sb.zweit { &mut sb.b } else { &mut sb.a };
                    te.insert(&c.to_string());
                    sb.verschieden = false;
                }
            }
            _ => out.repaint = false,
        }
        out
    }

    pub(super) fn setz_malen(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let Some(sb) = self.setz.as_ref() else {
            return;
        };
        let (s, u) = (w.scale, &t.ui);
        let f = self.frame(w);
        // Abdunkeln wie bei Dialogen
        c.fill_rect(f.x, f.y, f.w, f.h, sk_paint::Rgba(0, 0, 0, 110));
        let r = self.setz_rect(w);
        widgets::panel(c, r, s, t);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let x0 = r.x + PAD * s;
        if let Some(b) = bold {
            b.draw(
                c,
                "Verwaltungskennwort setzen",
                16.0 * s,
                x0,
                r.y + 36.0 * s,
                u.text,
            );
        }
        let line = s.round().max(1.0);
        c.fill_rect(r.x, r.y + 56.0 * s, r.w, line, u.border);
        c.fill_rect(r.x, r.y + r.h - 72.0 * s, r.w, line, u.border);
        let teile = self.setz_teile(w);
        let rect = |z: SZiel| {
            teile
                .iter()
                .find(|(x, _)| *x == z)
                .map(|(_, r)| *r)
                .unwrap()
        };
        // ×
        let x = rect(SZiel::Schliessen);
        let col = if sb.hover == Some(SZiel::Schliessen) {
            u.text
        } else {
            u.text_dim
        };
        let (mx, my, d) = (x.x + x.w * 0.5, x.y + x.h * 0.5, 4.5 * s);
        let mut p = Path::new();
        p.segment((mx - d, my - d), (mx + d, my + d), 1.4 * s);
        p.segment((mx - d, my + d), (mx + d, my - d), 1.4 * s);
        c.fill(&p, col);
        if let Some(rg) = regular {
            let px = 13.0 * s;
            let mut y = r.y + 88.0 * s;
            for z in widgets::wrap(Some(rg), WIRKUNG, px, r.w - 2.0 * PAD * s) {
                rg.draw(c, &z, px, x0, y, u.text);
                y += 20.0 * s;
            }
            for (z, text) in [(SZiel::Erstes, "Kennwort"), (SZiel::Zweites, "Noch einmal")] {
                let fr = rect(z);
                let base = fr.y + (fr.h + rg.cap_height(px)) * 0.5;
                rg.draw(c, text, px, x0, base.round(), u.text_dim);
            }
            let pxk = 11.5 * s;
            let mut y = r.y + 254.0 * s;
            if sb.verschieden {
                if let Some(b) = bold {
                    b.draw(
                        c,
                        VERSCHIEDEN,
                        pxk,
                        rect(SZiel::Zweites).x,
                        r.y + 246.0 * s,
                        u.field_invalid,
                    );
                }
                y += 14.0 * s;
            }
            for z in widgets::wrap(Some(rg), LEISE, pxk, r.w - 2.0 * PAD * s) {
                rg.draw(c, &z, pxk, x0, y, u.text_dim);
                y += 17.0 * s;
            }
        }
        for (z, te) in [(SZiel::Erstes, &sb.a), (SZiel::Zweites, &sb.b)] {
            let fokus = sb.zweit == (z == SZiel::Zweites);
            let fr = rect(z);
            feld_rahmen(
                c,
                fonts,
                fr,
                fokus,
                z == SZiel::Zweites && sb.verschieden,
                s,
                t,
            );
            punkte(c, fr, te.text.chars().count(), fokus, s, t);
        }
        for (z, text, active) in [
            (SZiel::Abbrechen, "Abbrechen", false),
            (SZiel::Setzen, "Kennwort setzen", true),
        ] {
            let st = ButtonState {
                hover: sb.hover == Some(z),
                pressed: sb.pressed == Some(z),
                active,
                disabled: false,
            };
            widgets::button(c, fonts, rect(z), text, st, s, t);
        }
    }
}

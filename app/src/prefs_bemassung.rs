//! Reiter „Bemaßung“ des Einstellungsfensters (Konzept
//! `bemassung/konzept.md` §6): der Stil der Maßketten dieses Projekts. Jede
//! Eingabe wirkt sofort in Grundriss und 3D, im Schritt „Einstellungen
//! geändert“ wie bei den Stiften. „Als Firmenstandard speichern“ merkt den
//! Stil für neue Projekte (in `einstellungen.txt`, ab „Übernehmen“ bzw.
//! „OK“), „Firmenstandard übernehmen“ setzt ihn in dieses Projekt.

use super::{
    label, BarId, ColorTarget, Ctx, Drag, FieldId, Out, Prefs, Target, TextKind, UiText, Win,
    VALUE_X,
};
use crate::bemassung::SCHRIFTEN;
use crate::scene::Scene;
use sk_model::bemassung::{Aussenbezug, BrhBezug, Kettenart, Stil, Symbol, Zahl};
use sk_paint::font::Font;
use sk_paint::{Canvas, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Knöpfe im Reiter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Knopf {
    Symbol(Symbol),
    /// Platz in [`SCHRIFTEN`].
    Schrift(usize),
    Rundung(u8),
    Bezug(Aussenbezug),
    Kette(Kettenart),
    Hoehe,
    Brh,
    BrhBezug(BrhBezug),
    /// „Als Firmenstandard speichern“ und „Firmenstandard übernehmen“.
    Speichern,
    Uebernehmen,
}

/// Eine Zeile des Reiters.
enum Zeile {
    Ueber(&'static str),
    /// Zwei Zahlenfelder, je mit Bezeichnung.
    Paar(&'static str, Zahl, &'static str, Zahl),
    /// Bezeichnung und Knöpfe, je mit der Zahl der Spalten, die er belegt.
    Knoepfe(&'static str, &'static [(Knopf, usize)]),
    /// Rundung (zwei halbe Knöpfe) und Farbe.
    RundungFarbe,
}

const ZEILEN: [Zeile; 15] = [
    Zeile::Ueber("Darstellung"),
    Zeile::Paar("Maßlinie", Zahl::Linie, "Maßhilfslinie", Zahl::Hilfslinie),
    Zeile::Paar(
        "Überstand der Maßlinie",
        Zahl::Ueberstand,
        "Hilfslinie über Maßlinie",
        Zahl::HilfsUeber,
    ),
    Zeile::Knoepfe(
        "Begrenzung",
        &[
            (Knopf::Symbol(Symbol::Kreis), 1),
            (Knopf::Symbol(Symbol::Schraeg), 1),
            (Knopf::Symbol(Symbol::Punkt), 1),
            (Knopf::Symbol(Symbol::Pfeil), 1),
        ],
    ),
    Zeile::Paar(
        "Größe der Begrenzung",
        Zahl::SymbolGroesse,
        "Strich der Begrenzung",
        Zahl::SymbolStrich,
    ),
    Zeile::Knoepfe(
        "Schrift",
        &[
            (Knopf::Schrift(0), 1),
            (Knopf::Schrift(1), 1),
            (Knopf::Schrift(2), 1),
            (Knopf::Schrift(3), 1),
        ],
    ),
    Zeile::Paar(
        "Schrifthöhe",
        Zahl::SchriftHoehe,
        "Hochzahl",
        Zahl::Hochzahl,
    ),
    Zeile::Paar(
        "Zahl über Maßlinie",
        Zahl::TextAbstand,
        "BRH vor der Wand",
        Zahl::BrhAbstand,
    ),
    Zeile::RundungFarbe,
    Zeile::Ueber("Außenmaßketten"),
    Zeile::Paar(
        "Erste Kette vor der Wand",
        Zahl::Erste,
        "Abstand der Ketten",
        Zahl::Abstand,
    ),
    Zeile::Knoepfe(
        "Wände bemaßen",
        &[
            (Knopf::Bezug(Aussenbezug::Rohbau), 2),
            (Knopf::Bezug(Aussenbezug::Wand), 2),
        ],
    ),
    Zeile::Knoepfe(
        "Ketten",
        &[
            (Knopf::Kette(Kettenart::Oeffnung), 1),
            (Knopf::Kette(Kettenart::Wand), 1),
            (Knopf::Kette(Kettenart::Sprung), 1),
            (Knopf::Kette(Kettenart::Gesamt), 1),
        ],
    ),
    Zeile::Knoepfe(
        "Öffnungen",
        &[
            (Knopf::Hoehe, 1),
            (Knopf::Brh, 1),
            (Knopf::BrhBezug(BrhBezug::Rohdecke), 1),
            (Knopf::BrhBezug(BrhBezug::Estrich), 1),
        ],
    ),
    Zeile::Knoepfe(
        "Firmenstandard",
        &[(Knopf::Speichern, 2), (Knopf::Uebernehmen, 2)],
    ),
];

/// Leise Zeile unter den Knöpfen zum Firmenstandard.
pub const GLEICH: &str =
    "Neue Projekte bekommen den Firmenstandard. Dieses Projekt entspricht ihm.";
pub const ANDERS: &str =
    "Neue Projekte bekommen den Firmenstandard. Dieses Projekt weicht davon ab.";

/// Beschriftung eines Knopfs.
pub fn knopf_text(k: Knopf) -> &'static str {
    match k {
        Knopf::Symbol(s) => s.name(),
        Knopf::Schrift(i) => SCHRIFTEN.get(i).map_or("", |s| s.0),
        Knopf::Rundung(1) => "1 mm",
        Knopf::Rundung(_) => "5 mm",
        Knopf::Bezug(b) => b.name(),
        Knopf::Kette(Kettenart::Oeffnung) => "Öffnungen",
        Knopf::Kette(Kettenart::Wand) => "Wände",
        Knopf::Kette(Kettenart::Sprung) => "Vor-/Rücksprünge",
        Knopf::Kette(Kettenart::Gesamt) => "Gesamtmaß",
        Knopf::Hoehe => "Höhe",
        Knopf::Brh => "Brüstungshöhe",
        Knopf::BrhBezug(BrhBezug::Rohdecke) => "BRH ab Rohdecke",
        Knopf::BrhBezug(BrhBezug::Estrich) => "BRH ab Estrich",
        Knopf::Speichern => "Als Firmenstandard speichern",
        Knopf::Uebernehmen => "Firmenstandard übernehmen",
    }
}

/// Ist der Knopf im Stil `s` eingeschaltet bzw. gewählt?
pub fn an(k: Knopf, s: &Stil) -> bool {
    match k {
        Knopf::Symbol(x) => s.symbol == x,
        Knopf::Schrift(i) => SCHRIFTEN
            .get(i)
            .is_some_and(|f| f.0.eq_ignore_ascii_case(&s.schrift)),
        Knopf::Rundung(r) => s.rundung == r,
        Knopf::Bezug(b) => s.bezug == b,
        Knopf::Kette(k) => s.kette(k),
        Knopf::Hoehe => s.hoehe,
        Knopf::Brh => s.brh,
        Knopf::BrhBezug(b) => s.brh_bezug == b,
        Knopf::Speichern | Knopf::Uebernehmen => false,
    }
}

/// Der Stil nach einem Klick auf den Knopf `k` (Firmenstandard: unverändert).
pub fn klick(k: Knopf, s: &Stil) -> Stil {
    let mut n = s.clone();
    match k {
        Knopf::Symbol(x) => n.symbol = x,
        Knopf::Schrift(i) => {
            if let Some(f) = SCHRIFTEN.get(i) {
                n.schrift = f.0.to_string();
            }
        }
        Knopf::Rundung(r) => n.rundung = r,
        Knopf::Bezug(b) => n.bezug = b,
        Knopf::Kette(k) => n.set_kette(k, !s.kette(k)),
        Knopf::Hoehe => n.hoehe = !s.hoehe,
        Knopf::Brh => n.brh = !s.brh,
        Knopf::BrhBezug(b) => n.brh_bezug = b,
        Knopf::Speichern | Knopf::Uebernehmen => {}
    }
    n
}

/// Erste Kette und Abstand stehen in m, die übrigen Werte in ihrer
/// Einheit aus [`Zahl::range`].
fn in_m(z: Zahl) -> bool {
    matches!(z, Zahl::Erste | Zahl::Abstand)
}

/// Einheit im Feld.
pub fn einheit(z: Zahl) -> &'static str {
    if in_m(z) {
        "m"
    } else {
        z.range().3
    }
}

/// Feldtext eines Zahlenwerts, mit Komma.
pub fn zahl_text(z: Zahl, v: f64) -> String {
    let (v, dec) = if in_m(z) {
        (v / 1000.0, 2)
    } else {
        (v, z.range().2)
    };
    format!("{v:.dec$}").replace('.', ",")
}

/// Liest eine Eingabe in den Stil; außerhalb des Bereichs ein Fehler mit
/// dem erlaubten Bereich.
pub fn zahl_setzen(s: &Stil, z: Zahl, text: &str) -> Result<Stil, String> {
    let (lo, hi, ..) = z.range();
    let k = if in_m(z) { 1000.0 } else { 1.0 };
    let bereich = || {
        format!(
            "erlaubt {} bis {} {}",
            zahl_text(z, lo),
            zahl_text(z, hi),
            einheit(z)
        )
    };
    let t = text.trim();
    let t = t.strip_suffix(einheit(z)).unwrap_or(t).trim_end();
    let v = t
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(bereich)?;
    let mut n = s.clone();
    if n.set_zahl(z, v * k) {
        Ok(n)
    } else {
        Err(bereich())
    }
}

/// Lage im Reiter (Fensterkoordinaten, um den Bildlauf verschoben).
pub(super) struct Layout {
    /// Sichtbarer Inhalt; was darüber hinausragt, ist abgeschnitten.
    pub area: Rect,
    pub bar: Option<Rect>,
    pub scroll: f32,
    pub content_h: f32,
    pub items: Vec<(Rect, Target)>,
    /// Bezeichnungen, Überschriften und die leisen Zeilen zum
    /// Firmenstandard ([`TextKind::Dim`]).
    pub texts: Vec<UiText>,
}

/// Zeilenabstand der leisen Zeilen zum Firmenstandard (dip).
const LEISE_ZEILE: f32 = 17.0;

/// Lage ab (0, 0) in der Breite `breite`; dazu die Höhe des Inhalts.
///
/// Eine Knopfzeile bricht um, sobald ein Text samt Innenabstand nicht in
/// seinen Knopf passt (fett gemessen): erst zwei Knöpfe je Zeile, dann
/// einer. Passt eine rechte Bezeichnung nicht vor ihr Feld, stehen alle
/// Paare zweizeilig (Einstellungen zu #116, B1).
fn lage(
    t: &Theme,
    s: f32,
    fonts: &Fonts,
    breite: f32,
    leise: &str,
) -> (Vec<(Rect, Target)>, Vec<UiText>, f32) {
    let (lx, vx) = (0.0, VALUE_X * s);
    let gap = 8.0 * s;
    let bw = ((breite - VALUE_X * s - 3.0 * gap) / 4.0)
        .min(118.0 * s)
        .floor();
    let fh = t.size.field_height * s;
    let schritt = fh + 5.0 * s;
    let px = t.size.font * s;
    let pad = t.size.field_pad * s;
    let fett = fonts.bold.as_ref().or(fonts.regular.as_ref());
    let normal = fonts.regular.as_ref();
    let breit = |f: Option<&Font>, text: &str| f.map_or(0.0, |f| f.width(text, px));
    let passt = |text: &str, w: f32| breit(fett, text) + 2.0 * pad <= w;
    let zelle = |i: usize, n: usize, y: f32| {
        let n = n.max(1) as f32;
        Rect::new(vx + i as f32 * (bw + gap), y, n * bw + (n - 1.0) * gap, fh)
    };
    // Rechte Bezeichnung ab der zweiten Spalte, ihr Feld in der vierten
    let rx = vx + bw + gap + 12.0 * s;
    let rechts = zelle(3, 1, 0.0).x - gap - rx;
    let zweizeilig = ZEILEN
        .iter()
        .any(|z| matches!(*z, Zeile::Paar(.., b, _) if breit(normal, b) > rechts));
    let mut items = Vec::new();
    let mut texts = Vec::new();
    let mut y = 0.0;
    for z in &ZEILEN {
        match *z {
            Zeile::Ueber(text) => {
                texts.push(UiText::heading(lx, y + 18.0 * s, text));
                y += 30.0 * s;
                continue;
            }
            Zeile::Paar(a, za, b, zb) => {
                texts.push(UiText::label(lx, y + 18.0 * s, a));
                items.push((zelle(0, 1, y), Target::Field(FieldId::Mass(za))));
                if zweizeilig {
                    y += schritt;
                    texts.push(UiText::label(lx, y + 18.0 * s, b));
                    items.push((zelle(0, 1, y), Target::Field(FieldId::Mass(zb))));
                } else {
                    texts.push(UiText::label(rx, y + 18.0 * s, b));
                    items.push((zelle(3, 1, y), Target::Field(FieldId::Mass(zb))));
                }
            }
            Zeile::Knoepfe(name, knoepfe) => {
                texts.push(UiText::label(lx, y + 18.0 * s, name));
                let f = [1, 2, 4]
                    .into_iter()
                    .find(|f| {
                        knoepfe
                            .iter()
                            .all(|(k, n)| passt(knopf_text(*k), zelle(0, (n * f).min(4), y).w))
                    })
                    .unwrap_or(4);
                let mut i = 0;
                for (k, n) in knoepfe {
                    let n = (n * f).min(4);
                    if i + n > 4 {
                        y += schritt;
                        i = 0;
                    }
                    items.push((zelle(i, n, y), Target::Mass(*k)));
                    i += n;
                }
            }
            Zeile::RundungFarbe => {
                texts.push(UiText::label(lx, y + 18.0 * s, "Rundung"));
                let r = zelle(0, 1, y);
                let halb = ((r.w - gap) * 0.5).floor();
                let rundung = [Knopf::Rundung(1), Knopf::Rundung(5)];
                let eng = rundung.iter().all(|k| passt(knopf_text(*k), halb));
                let kw = if eng { halb } else { bw };
                for (i, k) in rundung.into_iter().enumerate() {
                    let x = r.x + i as f32 * (kw + gap);
                    items.push((Rect::new(x, y, kw, fh), Target::Mass(k)));
                }
                let (fx, q) = if zweizeilig || !eng {
                    y += schritt;
                    (lx, zelle(0, 1, y))
                } else {
                    (rx, zelle(3, 1, y))
                };
                texts.push(UiText::label(fx, y + 18.0 * s, "Farbe"));
                let sw = Rect::new(q.x, q.y, q.w.min(64.0 * s), q.h);
                items.push((sw, Target::Swatch(ColorTarget::Mass)));
            }
        }
        y += schritt;
    }
    // Leise Zeilen zum Firmenstandard, im schmalen Fenster umbrochen
    let klein = t.size.font_small * s;
    y += 12.0 * s;
    for zeile in widgets::wrap(normal, leise, klein, breite) {
        texts.push(UiText::dim(lx, y, zeile));
        y += LEISE_ZEILE * s;
    }
    (items, texts, y)
}

impl Prefs {
    /// Firmenstandard, wie der Reiter ihn zeigt (ohne geladenen: Werk).
    pub(super) fn firmenstil_jetzt(&self) -> Stil {
        self.firmenstil.clone().unwrap_or_default()
    }

    /// Leise Zeile: Weicht dieses Projekt vom Firmenstandard ab?
    fn mass_leise(&self, sc: &Scene) -> &'static str {
        if *sc.model().massstil() == self.firmenstil_jetzt() {
            GLEICH
        } else {
            ANDERS
        }
    }

    /// Lage im Reiter; reicht die Höhe nicht, mit Bildlaufleiste rechts.
    pub(super) fn mass_layout(&self, t: &Theme, w: &Win, fonts: &Fonts, sc: &Scene) -> Layout {
        let c = self.content(t, w);
        let s = w.scale;
        let leise = self.mass_leise(sc);
        let mut l = lage(t, s, fonts, c.w, leise);
        let mut bar = None;
        if l.2 > c.h {
            let bw = t.size.scrollbar * s;
            l = lage(t, s, fonts, c.w - bw - 8.0 * s, leise);
            bar = Some(Rect::new(c.x + c.w - bw, c.y, bw, c.h));
        }
        let (items, texts, content_h) = l;
        let scroll = self.mass_scroll.clamp(0.0, (content_h - c.h).max(0.0));
        let dy = c.y - scroll;
        Layout {
            area: c,
            bar,
            scroll,
            content_h,
            items: items
                .into_iter()
                .map(|(r, tg)| (Rect::new(r.x + c.x, r.y + dy, r.w, r.h), tg))
                .collect(),
            texts: texts
                .into_iter()
                .map(|mut x| {
                    x.x += c.x;
                    x.y += dy;
                    x
                })
                .collect(),
        }
    }

    /// Reiter malen (Fensterkoordinaten über `at` ins Bild), auf den
    /// sichtbaren Inhalt begrenzt.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_mass(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sc: &Scene,
        at: &dyn Fn(Rect) -> Rect,
    ) {
        let s = w.scale;
        let u = &t.ui;
        let l = self.mass_layout(t, w, fonts, sc);
        let area = at(l.area);
        let mut cc = Canvas::new(area.w.ceil() as usize, area.h.ceil() as usize);
        let rel = |r: Rect| Rect::new(r.x - l.area.x, r.y - l.area.y, r.w, r.h);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        for tx in &l.texts {
            let (x, y) = (tx.x - l.area.x, tx.y - l.area.y);
            if y < -20.0 * s || y > area.h + 20.0 * s {
                continue;
            }
            let (f, px, farbe) = match tx.kind {
                TextKind::Heading => (bold, t.size.font_title, u.text),
                TextKind::Dim => (regular, t.size.font_small, u.field_unit),
                _ => (regular, t.size.font, u.text_dim),
            };
            label(&mut cc, f, &tx.text, px * s, x, y, farbe);
        }
        let stil = sc.model().massstil();
        let gleich = *stil == self.firmenstil_jetzt();
        for (r, tg) in &l.items {
            let rr = rel(*r);
            if rr.y + rr.h < 0.0 || rr.y > area.h {
                continue;
            }
            // Lage im Fensterbild, auf den sichtbaren Bereich begrenzt
            let seen = Rect::new(area.x + rr.x, area.y + rr.y, rr.w, rr.h);
            let seen = crate::window_kit::intersect(seen, area).unwrap_or(seen);
            let hover = self.hov(*tg, seen);
            match *tg {
                Target::Mass(k) => {
                    let st = ButtonState {
                        hover,
                        active: an(k, stil),
                        disabled: matches!(k, Knopf::Speichern | Knopf::Uebernehmen) && gleich,
                        ..Default::default()
                    };
                    widgets::button(&mut cc, fonts, rr, knopf_text(k), st, s, t);
                }
                Target::Field(f @ FieldId::Mass(z)) => {
                    let val = zahl_text(z, stil.zahl(z));
                    let st = self.field_state(f, &val, einheit(z), seen);
                    widgets::field(&mut cc, fonts, rr, &st, s, t);
                }
                Target::Swatch(ct) => {
                    widgets::swatch(&mut cc, rr, self.color_of(ct, sc, t), hover, s, t);
                }
                _ => {}
            }
        }
        c.blit(&cc, area.x as i32, area.y as i32);
        if let Some(b) = l.bar {
            let total = l.content_h.max(1.0);
            let hover = self.hov(Target::Bar(BarId::Mass), at(b))
                || matches!(self.drag, Some(Drag::Bar(BarId::Mass, _)));
            widgets::scrollbar(c, at(b), l.scroll / total, l.area.h / total, hover, s, t);
        }
    }

    /// Wert eines Zahlenfelds außerhalb der Eingabe.
    pub(super) fn mass_feld(&self, z: Zahl, sc: &Scene) -> String {
        zahl_text(z, sc.model().massstil().zahl(z))
    }

    /// Eingabe in ein Zahlenfeld: gültig wirkt sie sofort.
    pub(super) fn mass_wert(
        &mut self,
        z: Zahl,
        text: &str,
        cx: &mut Ctx,
        out: &mut Out,
    ) -> Result<(), String> {
        let n = zahl_setzen(cx.scene.model().massstil(), z, text)?;
        self.mass_setzen(n, cx, out);
        Ok(())
    }

    /// Klick auf einen Knopf des Reiters.
    pub(super) fn mass_klick(&mut self, k: Knopf, cx: &mut Ctx, out: &mut Out) {
        let stil = cx.scene.model().massstil().clone();
        match k {
            Knopf::Speichern => self.firmenstil = Some(stil),
            Knopf::Uebernehmen => {
                let f = self.firmenstil_jetzt();
                self.mass_setzen(f, cx, out);
            }
            k => self.mass_setzen(klick(k, &stil), cx, out),
        }
        out.repaint = true;
    }

    /// Neuer Stil im offenen Schritt.
    pub(super) fn mass_setzen(&mut self, n: Stil, cx: &mut Ctx, out: &mut Out) {
        if n != *cx.scene.model().massstil() {
            out.model |= cx.scene.edit_attr(|m| m.set_massstil(n));
        }
    }

    /// Farbe von Linien und Schrift.
    pub(super) fn mass_farbe(&self, sc: &Scene) -> Rgba {
        Rgba::from_rgb8(sc.model().massstil().farbe)
    }

    pub(super) fn set_mass_farbe(&mut self, c: Rgba, cx: &mut Ctx, out: &mut Out) {
        let mut n = cx.scene.model().massstil().clone();
        n.farbe = [c.0, c.1, c.2];
        self.mass_setzen(n, cx, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Knöpfe schalten bzw. wählen; Zahlen mit Komma, Kettenabstände in m,
    /// Bereich geprüft.
    #[test]
    fn knoepfe_und_zahlen() {
        let w = Stil::werk();
        assert!(an(Knopf::Symbol(Symbol::Kreis), &w));
        assert!(an(Knopf::Schrift(0), &w), "Segoe UI");
        let n = klick(Knopf::Symbol(Symbol::Schraeg), &w);
        assert_eq!(n.symbol, Symbol::Schraeg);
        let n = klick(Knopf::Kette(Kettenart::Sprung), &w);
        assert!(!n.kette(Kettenart::Sprung) && n.kette(Kettenart::Wand));
        let n = klick(Knopf::Schrift(3), &w);
        assert_eq!(n.schrift, "ISOCPEUR");
        assert!(an(Knopf::Schrift(3), &n) && !an(Knopf::Schrift(0), &n));
        assert_eq!(klick(Knopf::Speichern, &w), w);
        assert_eq!(zahl_text(Zahl::Erste, 2000.0), "2,00");
        assert_eq!(zahl_text(Zahl::Linie, 0.18), "0,18");
        assert_eq!(zahl_text(Zahl::Hochzahl, 65.0), "65");
        assert_eq!(einheit(Zahl::Abstand), "m");
        assert_eq!(einheit(Zahl::SchriftHoehe), "mm");
        let n = zahl_setzen(&w, Zahl::Erste, "1,5").unwrap();
        assert_eq!(n.erste, 1500.0);
        let n = zahl_setzen(&n, Zahl::SchriftHoehe, " 3.5 mm").unwrap();
        assert_eq!(n.schrift_mm, 3.5);
        let e = zahl_setzen(&w, Zahl::Abstand, "30").unwrap_err();
        assert!(e.contains("0,10 bis 20,00 m"), "{e}");
        assert!(zahl_setzen(&w, Zahl::Linie, "0").is_err());
        assert!(zahl_setzen(&w, Zahl::Linie, "x").is_err());
        for z in Zahl::ALL {
            let t = zahl_text(z, w.zahl(z));
            assert_eq!(zahl_setzen(&w, z, &t).unwrap(), w, "{z:?} {t}");
        }
    }

    /// Ein Knopf wirkt sofort im Projekt, „Abbrechen“ nimmt ihn zurück;
    /// „Als Firmenstandard speichern“ landet mit „OK“ in den Einstellungen,
    /// „Firmenstandard übernehmen“ setzt ihn ins Projekt, Zurücksetzen heißt
    /// Werk.
    #[test]
    fn projekt_firmenstandard_und_zuruecksetzen() {
        use crate::settings::Settings;
        let th = Theme::dark();
        let mut s = Scene::with_model(sk_model::Model::new());
        let mut st = Settings::new(["skizzeo".to_string()].into_iter(), None);
        let w = Win {
            w: 1440,
            h: 900,
            top: 32,
            scale: 1.0,
        };
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut p = Prefs::open(&mut s, &th).with_firmenstil(Stil::werk());
        p.show_tab(super::super::Tab::Bemassung);
        let mut th2 = th.clone();
        let mut out = Out::default();
        {
            let mut cx = Ctx {
                scene: &mut s,
                theme: &mut th2,
                settings: &mut st,
                fonts: &fonts,
                win: w,
            };
            p.mass_klick(Knopf::Symbol(Symbol::Pfeil), &mut cx, &mut out);
            assert!(out.model);
            assert!(p.mass_wert(Zahl::Erste, "x", &mut cx, &mut out).is_err());
            p.mass_wert(Zahl::Erste, "2,5", &mut cx, &mut out).unwrap();
        }
        assert_eq!(s.model().massstil().symbol, Symbol::Pfeil);
        assert_eq!(s.model().massstil().erste, 2500.0);
        p.cancel(&mut s, &mut th2);
        assert_eq!(*s.model().massstil(), Stil::werk());
        // Firmenstandard: speichern mit OK, dann in ein anderes Projekt
        let mut p = Prefs::open(&mut s, &th).with_firmenstil(Stil::werk());
        {
            let mut cx = Ctx {
                scene: &mut s,
                theme: &mut th2,
                settings: &mut st,
                fonts: &fonts,
                win: w,
            };
            p.mass_klick(Knopf::Kette(Kettenart::Sprung), &mut cx, &mut out);
            p.mass_klick(Knopf::Speichern, &mut cx, &mut out);
        }
        p.ok(&mut s, &th, &mut st);
        assert!(!st.firmenstil().kette(Kettenart::Sprung));
        let mut s2 = Scene::with_model(sk_model::Model::new());
        let mut p = Prefs::open(&mut s2, &th).with_firmenstil(st.firmenstil());
        {
            let mut cx = Ctx {
                scene: &mut s2,
                theme: &mut th2,
                settings: &mut st,
                fonts: &fonts,
                win: w,
            };
            p.mass_klick(Knopf::Uebernehmen, &mut cx, &mut out);
        }
        assert!(!s2.model().massstil().kette(Kettenart::Sprung));
        p.reset_tab(&mut s2, &mut th2, super::super::Tab::Bemassung);
        assert_eq!(*s2.model().massstil(), Stil::werk());
        p.ok(&mut s2, &th, &mut st);
    }

    /// Schriften für die Prüfung der Breiten: die des Systems, sonst
    /// Liberation Sans; ohne beide `None`.
    fn schriften() -> Option<Fonts> {
        let sys = Fonts::system();
        if sys.regular.is_some() && sys.bold.is_some() {
            return Some(sys);
        }
        let lib = |n: &str| {
            std::fs::read(format!("/usr/share/fonts/truetype/liberation/{n}"))
                .ok()
                .and_then(sk_paint::font::Font::parse)
        };
        Some(Fonts {
            regular: Some(lib("LiberationSans-Regular.ttf")?),
            bold: Some(lib("LiberationSans-Bold.ttf")?),
            italic: None,
        })
    }

    /// Standardfenster (860 × 620 dip), kleinstes (640 × 440) und dieses
    /// bei 150 %.
    const FENSTER: [Win; 3] = [
        Win {
            w: 1440,
            h: 900,
            top: 32,
            scale: 1.0,
        },
        Win {
            w: 600,
            h: 496,
            top: 32,
            scale: 1.0,
        },
        Win {
            w: 960,
            h: 744,
            top: 48,
            scale: 1.5,
        },
    ];

    /// Jeder Knopftext passt fett samt Innenabstand in seinen Knopf, jede
    /// Bezeichnung vor ihr Feld, nichts überlappt; reicht die Höhe nicht,
    /// läuft der Reiter mit Bildlaufleiste bis zur letzten Zeile
    /// (Einstellungen zu #116, B1).
    #[test]
    fn texte_passen_bei_860_und_640() {
        let Some(fonts) = schriften() else {
            eprintln!("keine Schrift zum Messen, Test übersprungen");
            return;
        };
        let th = Theme::dark();
        let mut sc = Scene::with_model(sk_model::Model::new());
        let mut p = Prefs::open(&mut sc, &th).with_firmenstil(Stil::werk());
        p.show_tab(super::super::Tab::Bemassung);
        let (regular, bold) = (
            fonts.regular.as_ref().unwrap(),
            fonts.bold.as_ref().unwrap(),
        );
        let breiten: Vec<f32> = FENSTER
            .iter()
            .map(|w| p.content(&th, w).w / w.scale)
            .collect();
        assert_eq!(breiten, [658.0, 438.0, 438.0], "Inhalt (dip)");
        for w in FENSTER {
            let s = w.scale;
            let px = th.size.font * s;
            let pad = th.size.field_pad * s;
            for scroll in [0.0, f32::MAX] {
                p.mass_scroll = scroll;
                let mut zuletzt = 0.0f32;
                let l = p.mass_layout(&th, &w, &fonts, &sc);
                let a = l.area;
                assert_eq!(a, p.content(&th, &w));
                let rechts = a.x + a.w - l.bar.map_or(0.0, |b| b.w + 8.0 * s);
                assert_eq!(l.bar.is_some(), l.content_h > a.h + 0.5, "{w:?}");
                for (i, (r, tg)) in l.items.iter().enumerate() {
                    assert!(r.x >= a.x && r.x + r.w <= rechts + 0.5, "{w:?} {tg:?}");
                    if let Target::Mass(k) = *tg {
                        let tw = bold.width(knopf_text(k), px) + 2.0 * pad;
                        assert!(tw <= r.w, "{w:?}: „{}“ {tw:.0} > {:.0}", knopf_text(k), r.w);
                    }
                    for (q, tq) in &l.items[i + 1..] {
                        let frei = r.x + r.w <= q.x
                            || q.x + q.w <= r.x
                            || r.y + r.h <= q.y
                            || q.y + q.h <= r.y;
                        assert!(frei, "{w:?}: {tg:?} überlappt {tq:?}");
                    }
                    zuletzt = zuletzt.max(r.y + r.h);
                }
                for tx in &l.texts {
                    let (f, g) = match tx.kind {
                        TextKind::Heading => (bold, th.size.font_title),
                        TextKind::Dim => (regular, th.size.font_small),
                        _ => (regular, th.size.font),
                    };
                    let ende = tx.x + f.width(&tx.text, g * s);
                    // bis vor das nächste Feld bzw. den nächsten Knopf daneben
                    let bis = l
                        .items
                        .iter()
                        .filter(|(r, _)| r.x > tx.x && r.y < tx.y && tx.y < r.y + r.h)
                        .map(|(r, _)| r.x - 4.0 * s)
                        .fold(rechts, f32::min);
                    assert!(ende <= bis, "{w:?}: „{}“ {ende:.0} > {bis:.0}", tx.text);
                }
                // Ganz unten liegt die letzte Zeile im sichtbaren Inhalt
                if scroll > 0.0 || l.bar.is_none() {
                    assert!(zuletzt <= a.y + a.h + 0.5, "{w:?}");
                }
            }
        }
        p.mass_scroll = 0.0;
    }

    /// Ist-Bilder des Reiters im Standardfenster und im kleinsten, dort
    /// oben und ganz unten (nur mit SKIZZEO_ISTBILDER, Schrift aus
    /// SKIZZEO_SCHRIFT, sonst wie beim Messen).
    #[test]
    fn istbild_reiter_bemassung() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let schrift = || {
            std::env::var_os("SKIZZEO_SCHRIFT")
                .and_then(|p| std::fs::read(p).ok())
                .and_then(sk_paint::font::Font::parse)
        };
        let fonts = match (schrift(), schrift()) {
            (Some(a), Some(b)) => Fonts {
                regular: Some(a),
                bold: Some(b),
                italic: None,
            },
            _ => schriften().unwrap_or_else(Fonts::system),
        };
        let th = Theme::dark();
        let mut s = Scene::with_model(sk_model::Model::new());
        let mut p = Prefs::open(&mut s, &th).with_firmenstil(Stil::werk());
        p.show_tab(super::super::Tab::Bemassung);
        for (w, name) in [
            (FENSTER[0], "ist-bem-reiter.png"),
            (FENSTER[1], "ist-bem-reiter-klein.png"),
        ] {
            p.mass_scroll = 0.0;
            let (c, _, _) = p.paint(&th, &fonts, &w, &s);
            std::fs::write(ziel.join(name), c.to_png()).unwrap();
            if p.mass_layout(&th, &w, &fonts, &s).bar.is_some() {
                p.mass_scroll = f32::MAX;
                let (c, _, _) = p.paint(&th, &fonts, &w, &s);
                let unten = name.replace(".png", "-unten.png");
                std::fs::write(ziel.join(unten), c.to_png()).unwrap();
            }
        }
    }
}

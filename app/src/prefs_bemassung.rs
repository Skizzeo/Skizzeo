//! Reiter „Bemaßung“ des Einstellungsfensters (Konzept
//! `bemassung/konzept.md` §6): der Stil der Maßketten dieses Projekts. Jede
//! Eingabe wirkt sofort in Grundriss und 3D, im Schritt „Einstellungen
//! geändert“ wie bei den Stiften. „Als Firmenstandard speichern“ merkt den
//! Stil für neue Projekte (in `einstellungen.txt`, ab „Übernehmen“ bzw.
//! „OK“), „Firmenstandard übernehmen“ setzt ihn in dieses Projekt.

use super::{label, ColorTarget, Ctx, FieldId, Out, Prefs, Target, TextKind, UiText, Win, VALUE_X};
use crate::bemassung::SCHRIFTEN;
use crate::scene::Scene;
use sk_model::bemassung::{Aussenbezug, BrhBezug, Kettenart, Stil, Symbol, Zahl};
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
        "Außenmaß bis",
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

/// Lage im Reiter.
pub(super) struct Layout {
    pub items: Vec<(Rect, Target)>,
    pub texts: Vec<UiText>,
    /// Grundlinie der leisen Zeile zum Firmenstandard.
    pub stand: (f32, f32),
}

impl Prefs {
    /// Firmenstandard, wie der Reiter ihn zeigt (ohne geladenen: Werk).
    pub(super) fn firmenstil_jetzt(&self) -> Stil {
        self.firmenstil.clone().unwrap_or_default()
    }

    pub(super) fn mass_layout(&self, t: &Theme, w: &Win) -> Layout {
        let c = self.content(t, w);
        let s = w.scale;
        let (lx, vx) = (c.x, c.x + VALUE_X * s);
        let gap = 8.0 * s;
        let bw = ((c.w - VALUE_X * s - 3.0 * gap) / 4.0)
            .min(118.0 * s)
            .floor();
        let fh = t.size.field_height * s;
        let zelle = |i: usize, n: usize, y: f32| {
            let n = n.max(1) as f32;
            Rect::new(vx + i as f32 * (bw + gap), y, n * bw + (n - 1.0) * gap, fh)
        };
        // Rechte Bezeichnung ab der zweiten Spalte, ihr Feld in der vierten
        let rx = vx + bw + gap + 12.0 * s;
        let mut items = Vec::new();
        let mut texts = Vec::new();
        let mut y = c.y;
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
                    texts.push(UiText::label(rx, y + 18.0 * s, b));
                    items.push((zelle(3, 1, y), Target::Field(FieldId::Mass(zb))));
                }
                Zeile::Knoepfe(name, knoepfe) => {
                    texts.push(UiText::label(lx, y + 18.0 * s, name));
                    let mut i = 0;
                    for (k, n) in knoepfe {
                        items.push((zelle(i, *n, y), Target::Mass(*k)));
                        i += n;
                    }
                }
                Zeile::RundungFarbe => {
                    texts.push(UiText::label(lx, y + 18.0 * s, "Rundung"));
                    let r = zelle(0, 1, y);
                    let hw = ((r.w - gap) * 0.5).floor();
                    for (i, k) in [1, 5].into_iter().enumerate() {
                        let x = r.x + i as f32 * (hw + gap);
                        items.push((Rect::new(x, y, hw, fh), Target::Mass(Knopf::Rundung(k))));
                    }
                    texts.push(UiText::label(rx, y + 18.0 * s, "Farbe"));
                    let q = zelle(3, 1, y);
                    let sw = Rect::new(q.x, q.y, q.w.min(64.0 * s), q.h);
                    items.push((sw, Target::Swatch(ColorTarget::Mass)));
                }
            }
            y += fh + 5.0 * s;
        }
        Layout {
            items,
            texts,
            stand: (lx, y + 12.0 * s),
        }
    }

    /// Reiter malen (Fensterkoordinaten über `at` ins Bild).
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
        let l = self.mass_layout(t, w);
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        for tx in &l.texts {
            let p = at(Rect::new(tx.x, tx.y, 0.0, 0.0));
            let (f, px, farbe) = match tx.kind {
                TextKind::Heading => (bold, t.size.font_title, u.text),
                _ => (regular, t.size.font, u.text_dim),
            };
            label(c, f, &tx.text, px * s, p.x, p.y, farbe);
        }
        let stil = sc.model().massstil();
        let firma = self.firmenstil_jetzt();
        for (r, tg) in &l.items {
            let rr = at(*r);
            let hover = self.hov(*tg, rr);
            match *tg {
                Target::Mass(k) => {
                    let gleich = *stil == firma;
                    let st = ButtonState {
                        hover,
                        active: an(k, stil),
                        disabled: matches!(k, Knopf::Speichern | Knopf::Uebernehmen) && gleich,
                        ..Default::default()
                    };
                    widgets::button(c, fonts, rr, knopf_text(k), st, s, t);
                }
                Target::Field(f @ FieldId::Mass(z)) => {
                    let val = zahl_text(z, stil.zahl(z));
                    let st = self.field_state(f, &val, einheit(z), rr);
                    widgets::field(c, fonts, rr, &st, s, t);
                }
                Target::Swatch(ct) => {
                    widgets::swatch(c, rr, self.color_of(ct, sc, t), hover, s, t);
                }
                _ => {}
            }
        }
        let text = if *stil == firma { GLEICH } else { ANDERS };
        let p = at(Rect::new(l.stand.0, l.stand.1, 0.0, 0.0));
        let px = t.size.font_small * s;
        label(c, regular, text, px, p.x, p.y, u.field_unit);
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
        // Treffer im Reiter: jedes Feld und jeder Knopf liegt im Inhalt
        let l = p.mass_layout(&th, &w);
        let c = p.content(&th, &w);
        for (r, _) in &l.items {
            assert!(r.x >= c.x && r.x + r.w <= c.x + c.w + 0.5, "{r:?}");
            assert!(r.y >= c.y && r.y + r.h <= c.y + c.h + 0.5, "{r:?}");
        }
        assert!(l.stand.1 <= c.y + c.h);
    }

    /// Ist-Bild des Reiters (nur mit SKIZZEO_ISTBILDER, Schrift aus
    /// SKIZZEO_SCHRIFT).
    #[test]
    fn istbild_reiter_bemassung() {
        let Some(ziel) = std::env::var_os("SKIZZEO_ISTBILDER").map(std::path::PathBuf::from) else {
            return;
        };
        let mut fonts = Fonts::system();
        let schrift = || {
            std::env::var_os("SKIZZEO_SCHRIFT")
                .and_then(|p| std::fs::read(p).ok())
                .and_then(sk_paint::font::Font::parse)
        };
        if let (Some(a), Some(b)) = (schrift(), schrift()) {
            fonts.regular = Some(a);
            fonts.bold = Some(b);
        }
        let th = Theme::dark();
        let mut s = Scene::with_model(sk_model::Model::new());
        let mut p = Prefs::open(&mut s, &th).with_firmenstil(Stil::werk());
        p.show_tab(super::super::Tab::Bemassung);
        let w = Win {
            w: 1440,
            h: 900,
            top: 32,
            scale: 1.0,
        };
        let (c, _, _) = p.paint(&th, &fonts, &w, &s);
        std::fs::write(ziel.join("ist-bem-reiter.png"), c.to_png()).unwrap();
    }
}

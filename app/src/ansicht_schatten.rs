//! Schatten in den vier Ansichten (Sonnenstand S7, Analyse §3.3, §3.4):
//! im Papiermodus werden abgewandte Flächen und Flächen im Schlagschatten
//! grau getönt oder schraffiert. Das Licht kommt klassisch parallel zur
//! Raumdiagonale von vorne links bzw. vorne rechts der jeweiligen Ansicht,
//! unter 45° von vorne oben oder von der Sonne des Sonnenstands-Systems.
//!
//! Bedienung: ein Zahnrad links neben dem Paneel „Ansichten“ öffnet ein
//! kleines Feld mit Schatten an/aus, Fläche/Schraffur, Licht und „Auf alle
//! Ansichten übertragen“. Die Wahl steht je Ansicht im Modell
//! ([`ViewShade`], `[viewshade]`) als Ansichtszustand, ohne
//! Rückgängig-Schritt, und nur, wo sie von der Vorgabe abweicht.

use crate::sonne_view;
use crate::ui::ViewKind;
use sk_math::{vec3, Vec3};
use sk_model::{Location, ShadeLight, Sun, ViewShade};
use sk_paint::{Canvas, Path};
use sk_platform::{Event, MouseButton};
use sk_render::{schatten, PaperShade};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, Fonts, Rect};

/// Anteil der Tinte in der grauen Schattenfläche.
pub const TON: f32 = 0.25;
/// Schraffur: Abstand und Strich auf dem Blatt (mm).
pub const SCHRAFFUR_MM: f32 = 1.5;
pub const STRICH_MM: f32 = 0.18;
/// Tooltip am Zahnrad.
pub const TIP: &str = "Schatten dieser Ansicht";
/// Tooltip an „Sonne“ ohne Nordrichtung.
pub const OHNE_NORD: &str = "Nordrichtung fehlt";
/// Hinweis im Feld, wenn die Sonne zu tief steht.
pub const UNTER: &str = "Sonne unter 2°: kein Schatten";

// ===== Reine Funktionen =====

/// Platz der Ansicht in [`sk_model::SHADE_VIEWS`]; `None` für 3D,
/// Grundriss und Schnitt.
pub fn platz(v: ViewKind) -> Option<usize> {
    match v {
        ViewKind::Front => Some(0),
        ViewKind::Back => Some(1),
        ViewKind::Left => Some(2),
        ViewKind::Right => Some(3),
        _ => None,
    }
}

/// Blickrichtung und Rechts der Ansicht (waagerecht, Modell), wie ihre
/// Kamera.
fn achsen(v: ViewKind) -> Option<(Vec3, Vec3)> {
    let (f, r) = match v {
        ViewKind::Front => ((0.0, 1.0), (1.0, 0.0)),
        ViewKind::Back => ((0.0, -1.0), (-1.0, 0.0)),
        ViewKind::Left => ((1.0, 0.0), (0.0, -1.0)),
        ViewKind::Right => ((-1.0, 0.0), (0.0, 1.0)),
        _ => return None,
    };
    Some((vec3(f.0, f.1, 0.0), vec3(r.0, r.1, 0.0)))
}

/// Klassisches Licht der Ansicht (§3.4): parallel zur Raumdiagonale, von
/// vorne links bzw. vorne rechts und oben, 35,26° hoch. Richtung zum Licht:
/// normiert(−vor ∓ rechts + oben), „vorne links“ wirft nach rechts unten.
pub fn klassisch(v: ViewKind, rechts: bool) -> Option<Vec3> {
    let (f, r) = achsen(v)?;
    let seite = if rechts { r } else { r * -1.0 };
    Some((f * -1.0 + seite + vec3(0.0, 0.0, 1.0)).normalized())
}

/// Licht „vorne oben“ (§8 11:55, Rückfrage a): normiert(−vor + oben),
/// 45° hoch ohne Seitenanteil; wirft nur nach unten.
pub fn von_oben(v: ViewKind) -> Option<Vec3> {
    let (f, _) = achsen(v)?;
    Some((f * -1.0 + vec3(0.0, 0.0, 1.0)).normalized())
}

/// Ob die Sonne als Licht wählbar ist: nur mit gesetzter Nordrichtung.
pub fn sonne_waehlbar(l: &Location) -> bool {
    l.north.is_some()
}

/// Der Stand der Sonne für die Ansichten: aus der Leiste, sonst heute
/// 12:00 wie beim ersten Einschalten. Wer „Sonne“ wählt, schreibt diesen
/// Stand fest ([`festschreiben`]), damit die Ansicht nicht mit dem Tag
/// wechselt.
pub fn sonne_der_ansichten(s: Option<Sun>) -> Sun {
    s.unwrap_or_else(|| sonne_view::anfang(sonne_view::uhr()))
}

/// Stand für `[sun]`, wenn eine Ansicht „Sonne“ wählt und noch keiner
/// gespeichert ist (§8 11:55): der Stand der Leiste, mit dem System aus.
pub fn festschreiben(s: Option<Sun>, wahl: ViewShade) -> Option<Sun> {
    (s.is_none() && wahl.light == ShadeLight::Sun).then(|| Sun {
        on: false,
        ..sonne_der_ansichten(None)
    })
}

/// Richtung zum Licht der Ansicht `v` mit der Wahl `vs`; `None`, wenn
/// sie keinen Schatten zeigt (aus, Sonne ohne Nordrichtung oder unter
/// [`schatten::MIN_HOEHE`]).
pub fn licht(v: ViewKind, vs: ViewShade, l: &Location, sun: Sun) -> Option<Vec3> {
    if !vs.on {
        return None;
    }
    match vs.light {
        ShadeLight::FrontLeft => klassisch(v, false),
        ShadeLight::FrontRight => klassisch(v, true),
        ShadeLight::Top => von_oben(v),
        ShadeLight::Sun => {
            if !sonne_waehlbar(l) {
                return None;
            }
            sonne_view::zur_sonne(l, &sun).filter(|d| d.z >= schatten::min_sinus())
        }
    }
}

/// Steht die Sonne der Ansichten zu tief für Schatten?
pub fn sonne_zu_tief(l: &Location, sun: Sun) -> bool {
    sonne_view::zur_sonne(l, &sun).is_none_or(|d| d.z < schatten::min_sinus())
}

/// Schatten für den Renderer: Richtung `d` zum Licht, Darstellung aus
/// `vs`, Tinte des Stifts „Ansichtsmuster“, Pixel je mm und Skalierung.
pub fn papier(vs: ViewShade, d: Vec3, tinte: [f32; 3], px_per_mm: f32, s: f32) -> PaperShade {
    PaperShade {
        zum_licht: [d.x, d.y, d.z],
        hatch: vs.hatch,
        tone: [tinte[0], tinte[1], tinte[2], TON],
        hatch_px: [
            SCHRAFFUR_MM * px_per_mm * s,
            (STRICH_MM * px_per_mm * s).max(1.0),
        ],
        hatch_ink: tinte,
    }
}

// ===== Zahnrad und Feld =====

/// Teil von Zahnrad und Feld.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    Zahnrad,
    An(bool),
    Schraffur(bool),
    Licht(ShadeLight),
    Alle,
}

/// Maße (dip).
const RAD: f32 = 28.0;
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const NAME_W: f32 = 84.0;
const KNOPF_W: f32 = 92.0;
/// Breite einer Knopfreihe: vier Lichter.
const REIHE: f32 = 4.0 * KNOPF_W + 3.0 * GAP;

/// Zeilennamen im Feld.
const NAMEN: [&str; 3] = ["Schatten", "Darstellung", "Licht"];

/// Lage von Zahnrad und Feld in der Ansicht (Pixel).
#[derive(Clone, Debug, PartialEq)]
pub struct Lage {
    pub zahnrad: Rect,
    /// Das offene Feld und seine Knöpfe; die Namen stehen links daneben.
    pub feld: Rect,
    pub teile: Vec<(Teil, Rect)>,
    /// Zeilen (Name, Grundlinie y) und die Zeile für den Hinweis.
    pub namen: Vec<(&'static str, Rect)>,
    pub hinweis: Option<Rect>,
}

/// Zahnrad oben links neben dem Paneel „Ansichten“ (`rechts`: dessen
/// linker Rand in Ansichtspixeln), das Feld darunter, rechtsbündig.
pub fn lage(rechts: f32, mit_hinweis: bool, s: f32, t: &Theme) -> Lage {
    let r = |x: f32, y: f32, w: f32, h: f32| {
        Rect::new(
            (x * s).round(),
            (y * s).round(),
            (w * s).round(),
            (h * s).round(),
        )
    };
    let m = t.size.panel_margin;
    let h = t.size.field_height;
    let x1 = rechts / s - m;
    let zahnrad = r(x1 - RAD, m, RAD, RAD);
    let breite = 2.0 * PAD + NAME_W + REIHE;
    let x0 = x1 - breite;
    let y0 = m + RAD + 4.0;
    let kx = x0 + PAD + NAME_W;
    let zeile = |i: f32| y0 + PAD + i * (h + GAP);
    let mut teile = vec![(Teil::Zahnrad, zahnrad)];
    let mut namen = Vec::new();
    for (i, name) in NAMEN.iter().enumerate() {
        namen.push((*name, r(x0 + PAD, zeile(i as f32), NAME_W, h)));
    }
    let k = |j: f32, i: f32| r(kx + j * (KNOPF_W + GAP), zeile(i), KNOPF_W, h);
    teile.extend([
        (Teil::An(true), k(0.0, 0.0)),
        (Teil::An(false), k(1.0, 0.0)),
        (Teil::Schraffur(false), k(0.0, 1.0)),
        (Teil::Schraffur(true), k(1.0, 1.0)),
        (Teil::Licht(ShadeLight::FrontLeft), k(0.0, 2.0)),
        (Teil::Licht(ShadeLight::FrontRight), k(1.0, 2.0)),
        (Teil::Licht(ShadeLight::Top), k(2.0, 2.0)),
        (Teil::Licht(ShadeLight::Sun), k(3.0, 2.0)),
        (Teil::Alle, r(kx, zeile(3.0), REIHE, h)),
    ]);
    let mut unten = zeile(4.0);
    let hinweis = mit_hinweis.then(|| {
        let q = r(kx, unten - GAP * 0.5, REIHE, h * 0.8);
        unten += h * 0.8;
        q
    });
    let feld = r(x0, y0, breite, unten - GAP + PAD - y0);
    Lage {
        zahnrad,
        feld,
        teile,
        namen,
        hinweis,
    }
}

/// Beschriftung eines Knopfs.
pub fn text(t: Teil) -> &'static str {
    match t {
        Teil::Zahnrad => "",
        Teil::An(true) => "An",
        Teil::An(false) => "Aus",
        Teil::Schraffur(false) => "Fläche",
        Teil::Schraffur(true) => "Schraffur",
        Teil::Licht(ShadeLight::FrontLeft) => "Vorne links",
        Teil::Licht(ShadeLight::FrontRight) => "Vorne rechts",
        Teil::Licht(ShadeLight::Top) => "Vorne oben",
        Teil::Licht(ShadeLight::Sun) => "Sonne",
        Teil::Alle => "Auf alle Ansichten übertragen",
    }
}

/// Ob der Knopf die Wahl `vs` zeigt.
pub fn gewaehlt(t: Teil, vs: ViewShade) -> bool {
    match t {
        Teil::An(on) => vs.on == on,
        Teil::Schraffur(h) => vs.hatch == h,
        Teil::Licht(l) => vs.light == l,
        _ => false,
    }
}

/// Die Wahl nach einem Klick auf `t`.
pub fn waehlen(t: Teil, vs: ViewShade) -> ViewShade {
    match t {
        Teil::An(on) => ViewShade { on, ..vs },
        Teil::Schraffur(hatch) => ViewShade { hatch, ..vs },
        Teil::Licht(light) => ViewShade { light, ..vs },
        _ => vs,
    }
}

/// Was Zahnrad und Feld zeigen; gleich: kein neues Bild.
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    pub vs: ViewShade,
    pub offen: bool,
    pub hover: Option<Teil>,
    pub sonne_ok: bool,
    pub zu_tief: bool,
    /// Linker Rand des Paneels „Ansichten“ (Bits) und Skalierung (Bits).
    pub rechts: u32,
    pub scale: u32,
}

/// Zahnrad (Kreis mit acht Zähnen und Loch) in `r`.
fn zahnrad(c: &mut Canvas, r: Rect, farbe: sk_paint::Rgba) {
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    let (aussen, innen, loch) = (r.w * 0.36, r.w * 0.27, r.w * 0.12);
    let n = 8;
    let mut p = Path::new();
    for i in 0..4 * n {
        // Je Zahn: zwei Punkte außen, zwei innen
        let a = (i as f32 + 0.5) * std::f32::consts::TAU / (4 * n) as f32;
        let rr = if (i / 2) % 2 == 0 { aussen } else { innen };
        let (x, y) = (cx + rr * a.cos(), cy + rr * a.sin());
        if i == 0 {
            p.move_to(x, y);
        } else {
            p.line_to(x, y);
        }
    }
    p.close();
    // Loch gegen den Umlauf
    for i in 0..24 {
        let a = -(i as f32) * std::f32::consts::TAU / 24.0;
        let (x, y) = (cx + loch * a.cos(), cy + loch * a.sin());
        if i == 0 {
            p.move_to(x, y);
        } else {
            p.line_to(x, y);
        }
    }
    p.close();
    c.fill(&p, farbe);
}

/// Zahnrad und (offen) Feld zeichnen: Bild und Lage in der Ansicht.
pub fn malen(b: &Bild, fonts: &Fonts, t: &Theme) -> (Canvas, i32, i32) {
    let s = f32::from_bits(b.scale);
    let l = lage(f32::from_bits(b.rechts), b.zu_tief, s, t);
    let sh = (t.size.panel_shadow * s).ceil();
    let alles = if b.offen {
        let x0 = l.feld.x.min(l.zahnrad.x);
        let y0 = l.zahnrad.y;
        let x1 = (l.feld.x + l.feld.w).max(l.zahnrad.x + l.zahnrad.w);
        let y1 = l.feld.y + l.feld.h;
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    } else {
        l.zahnrad
    };
    let (w, h) = ((alles.w + 2.0 * sh) as usize, (alles.h + 2.0 * sh) as usize);
    let mut c = Canvas::new(w, h);
    let (dx, dy) = (alles.x - sh, alles.y - sh);
    let ab = |q: Rect| Rect::new(q.x - dx, q.y - dy, q.w, q.h);
    let st = ButtonState {
        hover: b.hover == Some(Teil::Zahnrad),
        active: b.offen,
        ..Default::default()
    };
    widgets::button(&mut c, fonts, ab(l.zahnrad), "", st, s, t);
    let farbe = if b.offen { t.ui.on_accent } else { t.ui.text };
    zahnrad(&mut c, ab(l.zahnrad), farbe);
    if !b.offen {
        return (c, dx as i32, dy as i32);
    }
    widgets::panel(&mut c, ab(l.feld), s, t);
    let f = fonts.regular.as_ref();
    let px = t.size.font * s;
    for (name, q) in &l.namen {
        if let Some(f) = f {
            let q = ab(*q);
            let y = q.y + (q.h + f.cap_height(px)) * 0.5;
            f.draw(&mut c, name, px, q.x.round(), y.round(), t.ui.text);
        }
    }
    for (teil, q) in l.teile.iter().filter(|p| p.0 != Teil::Zahnrad) {
        let gesperrt = *teil == Teil::Licht(ShadeLight::Sun) && !b.sonne_ok;
        let st = ButtonState {
            hover: b.hover == Some(*teil) && !gesperrt,
            active: gewaehlt(*teil, b.vs),
            disabled: gesperrt,
            ..Default::default()
        };
        widgets::button(&mut c, fonts, ab(*q), text(*teil), st, s, t);
    }
    if let (Some(q), Some(f)) = (l.hinweis, f) {
        let q = ab(q);
        let px = t.size.font_small * s;
        let y = q.y + (q.h + f.cap_height(px)) * 0.5;
        f.draw(&mut c, UNTER, px, q.x.round(), y.round(), t.ui.text_dim);
    }
    (c, dx as i32, dy as i32)
}

/// Bedienzustand: Feld offen, Teil unter der Maus (`unter` auch, wenn er
/// gesperrt ist, für den Tooltip).
#[derive(Clone, Debug, Default)]
pub struct Schalter {
    pub offen: bool,
    pub hover: Option<Teil>,
    pub unter: Option<Teil>,
}

/// Ergebnis eines Ereignisses.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// Neue Wahl für diese Ansicht.
    pub wahl: Option<ViewShade>,
    /// Die Wahl dieser Ansicht in alle vier übertragen.
    pub alle: bool,
}

impl Schalter {
    pub fn reset(&mut self) {
        *self = Schalter::default();
    }

    fn treffer(&self, l: &Lage, m: (f64, f64)) -> Option<Teil> {
        l.teiles(self.offen)
            .find(|(_, q)| q.contains(m.0, m.1))
            .map(|p| p.0)
    }

    /// Mausereignis in der Ansicht (ohne Titelleiste); `vs`: die Wahl
    /// dieser Ansicht, `sonne_ok`: Sonne wählbar.
    pub fn handle(&mut self, e: &Event, l: &Lage, vs: ViewShade, sonne_ok: bool) -> Ausgang {
        let mut out = Ausgang::default();
        let gesperrt = |t: Teil| t == Teil::Licht(ShadeLight::Sun) && !sonne_ok;
        let im_feld = |m: (f64, f64)| self.offen && l.feld.contains(m.0, m.1);
        match *e {
            Event::MouseMove { x, y, .. } => {
                self.unter = self.treffer(l, (x, y));
                let t = self.unter.filter(|t| !gesperrt(*t));
                out.redraw = t != self.hover;
                self.hover = t;
                out.consumed = t.is_some() || im_feld((x, y));
            }
            Event::MouseLeave => {
                self.unter = None;
                out.redraw = self.hover.take().is_some();
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => match self.treffer(l, (x, y)) {
                Some(Teil::Zahnrad) => {
                    self.offen = !self.offen;
                    out.consumed = true;
                    out.redraw = true;
                }
                Some(t) if gesperrt(t) => out.consumed = true,
                Some(Teil::Alle) => {
                    out.alle = true;
                    out.consumed = true;
                    out.redraw = true;
                }
                Some(t) => {
                    let neu = waehlen(t, vs);
                    out.wahl = (neu != vs).then_some(neu);
                    out.consumed = true;
                    out.redraw = true;
                }
                None if im_feld((x, y)) => out.consumed = true,
                // Klick daneben schließt das Feld und gehört der Ansicht
                None => {
                    out.redraw = std::mem::take(&mut self.offen);
                }
            },
            _ => {}
        }
        out
    }
}

impl Lage {
    /// Die treffbaren Teile: geschlossen nur das Zahnrad.
    fn teiles(&self, offen: bool) -> impl Iterator<Item = &(Teil, Rect)> {
        self.teile
            .iter()
            .filter(move |p| offen || p.0 == Teil::Zahnrad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::SHADE_VIEWS;
    use sk_platform::Modifiers;

    /// §3.4: Raumdiagonale, 35,26° hoch; „vorne links“ ist je Ansicht ihr
    /// eigenes vorne links.
    #[test]
    fn klassisches_licht() {
        let h = (1.0f64 / 3.0f64.sqrt()).asin().to_degrees();
        assert!((h - 35.26).abs() < 0.01);
        let d = klassisch(ViewKind::Front, false).unwrap();
        let k = 1.0 / 3.0f64.sqrt();
        assert!((d - vec3(-k, -k, k)).length() < 1e-12, "{d:?}");
        assert!((klassisch(ViewKind::Front, true).unwrap() - vec3(k, -k, k)).length() < 1e-12);
        assert!((klassisch(ViewKind::Back, false).unwrap() - vec3(k, k, k)).length() < 1e-12);
        assert!((klassisch(ViewKind::Left, false).unwrap() - vec3(-k, k, k)).length() < 1e-12);
        assert!((klassisch(ViewKind::Right, false).unwrap() - vec3(k, -k, k)).length() < 1e-12);
        // Vorne oben: 45° ohne Seitenanteil
        let o = 0.5f64.sqrt();
        assert!((von_oben(ViewKind::Front).unwrap() - vec3(0.0, -o, o)).length() < 1e-12);
        assert!((von_oben(ViewKind::Left).unwrap() - vec3(-o, 0.0, o)).length() < 1e-12);
        for v in [ViewKind::Persp, ViewKind::Plan, ViewKind::Section] {
            assert_eq!(klassisch(v, false), None);
            assert_eq!(von_oben(v), None);
            assert_eq!(platz(v), None);
        }
        let plaetze: Vec<usize> = [
            ViewKind::Front,
            ViewKind::Back,
            ViewKind::Left,
            ViewKind::Right,
        ]
        .iter()
        .filter_map(|v| platz(*v))
        .collect();
        assert_eq!(plaetze, [0, 1, 2, 3]);
        assert_eq!(SHADE_VIEWS.len(), 4);
    }

    /// Sonne nur mit Nordrichtung und ab 2°; aus heißt kein Licht.
    #[test]
    fn licht_der_ansicht() {
        let mut l = Location::default();
        let sun = Sun {
            date: sk_math::sonne::Datum::new(2026, 6, 21).unwrap(),
            minutes: 15 * 60,
            on: false,
        };
        let sonne = ViewShade {
            light: ShadeLight::Sun,
            ..ViewShade::WERK
        };
        assert!(!sonne_waehlbar(&l));
        assert_eq!(licht(ViewKind::Front, sonne, &l, sun), None);
        l.north = Some(0.0);
        let d = licht(ViewKind::Front, sonne, &l, sun).unwrap();
        assert_eq!(Some(d), sonne_view::zur_sonne(&l, &sun));
        let nacht = Sun {
            minutes: 23 * 60,
            ..sun
        };
        assert_eq!(licht(ViewKind::Front, sonne, &l, nacht), None);
        assert!(sonne_zu_tief(&l, nacht) && !sonne_zu_tief(&l, sun));
        let aus = ViewShade {
            on: false,
            ..ViewShade::WERK
        };
        assert_eq!(licht(ViewKind::Front, aus, &l, sun), None);
        assert_eq!(
            licht(ViewKind::Back, ViewShade::WERK, &l, sun),
            klassisch(ViewKind::Back, false)
        );
        let oben = ViewShade {
            light: ShadeLight::Top,
            ..ViewShade::WERK
        };
        assert_eq!(
            licht(ViewKind::Back, oben, &l, nacht),
            von_oben(ViewKind::Back)
        );
        // „Sonne“ ohne gespeicherten Stand schreibt den der Leiste fest,
        // mit dem System aus; sonst bleibt [sun], wie es ist
        let fest = festschreiben(None, sonne).unwrap();
        assert!(!fest.on && fest.minutes == 12 * 60);
        assert_eq!(festschreiben(Some(sun), sonne), None);
        assert_eq!(festschreiben(None, oben), None);
    }

    /// Zahnrad öffnet das Feld, Knöpfe wählen, „Sonne“ ohne Nord gesperrt,
    /// „auf alle“ meldet sich, Klick daneben schließt.
    #[test]
    fn zahnrad_und_feld() {
        let t = Theme::dark();
        let l = lage(900.0, false, 1.0, &t);
        // Das Feld liegt links vom Paneel und unter dem Zahnrad
        assert!(l.zahnrad.x + l.zahnrad.w <= 900.0);
        assert!(l.feld.x + l.feld.w <= 900.0 && l.feld.y > l.zahnrad.y + l.zahnrad.h);
        for (_, q) in l.teile.iter().skip(1) {
            assert!(q.x >= l.feld.x && q.x + q.w <= l.feld.x + l.feld.w);
            assert!(q.y >= l.feld.y && q.y + q.h <= l.feld.y + l.feld.h);
        }
        let mitte = |t: Teil| {
            let q = l.teile.iter().find(|p| p.0 == t).unwrap().1;
            ((q.x + q.w * 0.5) as f64, (q.y + q.h * 0.5) as f64)
        };
        let klick = |p: (f64, f64)| Event::MouseDown {
            button: MouseButton::Left,
            x: p.0,
            y: p.1,
            mods: Modifiers::default(),
        };
        let mut sch = Schalter::default();
        let vs = ViewShade::WERK;
        // Geschlossen trifft nur das Zahnrad
        let o = sch.handle(&klick(mitte(Teil::An(false))), &l, vs, false);
        assert!(!o.consumed && o.wahl.is_none());
        let o = sch.handle(&klick(mitte(Teil::Zahnrad)), &l, vs, false);
        assert!(o.consumed && sch.offen);
        let o = sch.handle(&klick(mitte(Teil::Schraffur(true))), &l, vs, false);
        assert_eq!(o.wahl, Some(ViewShade { hatch: true, ..vs }));
        let o = sch.handle(&klick(mitte(Teil::An(true))), &l, vs, false);
        assert!(o.consumed && o.wahl.is_none(), "schon an");
        let o = sch.handle(&klick(mitte(Teil::Licht(ShadeLight::Sun))), &l, vs, false);
        assert!(o.consumed && o.wahl.is_none(), "ohne Nord gesperrt");
        let o = sch.handle(&klick(mitte(Teil::Licht(ShadeLight::Sun))), &l, vs, true);
        assert_eq!(o.wahl.map(|w| w.light), Some(ShadeLight::Sun));
        let o = sch.handle(&klick(mitte(Teil::Alle)), &l, vs, true);
        assert!(o.alle && o.consumed);
        // Im Feld zwischen den Knöpfen: verbraucht, bleibt offen
        let n = l.namen[0].1;
        let o = sch.handle(
            &klick(((n.x + 2.0) as f64, (n.y + 2.0) as f64)),
            &l,
            vs,
            true,
        );
        assert!(o.consumed && sch.offen);
        let o = sch.handle(&klick((10.0, 500.0)), &l, vs, true);
        assert!(!o.consumed && o.redraw && !sch.offen);
        // Bild: offen größer als zu
        let fonts = Fonts {
            regular: None,
            bold: None,
            italic: None,
        };
        let mut b = Bild {
            vs,
            offen: false,
            hover: None,
            sonne_ok: true,
            zu_tief: false,
            rechts: 900f32.to_bits(),
            scale: 1f32.to_bits(),
        };
        let (zu, _, _) = malen(&b, &fonts, &t);
        b.offen = true;
        let (auf, x, y) = malen(&b, &fonts, &t);
        assert!(auf.width > zu.width && auf.height > zu.height);
        assert!(x as f32 <= l.feld.x && y as f32 <= l.zahnrad.y);
    }

    /// §5 S7: 50 cm Dachüberstand ergibt bei klassischem Licht 50 cm
    /// Schattenband nach unten und 50 cm zur Seite (±1 %), gerechnet mit
    /// dem Tiefenbild der Schattenkarte. Die Seite folgt dem Licht:
    /// „vorne links“ wirft nach rechts, „vorne rechts“ nach links, „vorne
    /// oben“ nur nach unten (§8 11:55).
    #[test]
    fn ueberstand_gibt_gleich_breites_band() {
        use sk_render::schatten::{karte, Tiefenbild, GROESSE};
        let wand = (vec3(0.0, 0.0, 0.0), vec3(6000.0, 365.0, 3000.0));
        let platte = (vec3(1000.0, -500.0, 3000.0), vec3(5000.0, 365.0, 3200.0));
        let huelle = (vec3(0.0, -500.0, 0.0), vec3(6000.0, 365.0, 3200.0));
        let vorn = vec3(0.0, -1.0, 0.0);
        // Licht und Versatz der Unterkante zur Seite (mm)
        for (licht, versatz) in [
            (ShadeLight::FrontLeft, 500.0),
            (ShadeLight::FrontRight, -500.0),
            (ShadeLight::Top, 0.0),
        ] {
            let vs = ViewShade {
                light: licht,
                ..ViewShade::WERK
            };
            let d = self::licht(ViewKind::Front, vs, &Location::default(), sonne(12 * 60)).unwrap();
            let mut t = Tiefenbild::neu(karte(d, huelle, GROESSE).unwrap());
            for q in [wand, platte] {
                t.netz(&sonne_view::quader_netz(q).faces);
            }
            let hell = |x: f64, z: f64| t.sonne(vec3(x, 0.0, z), vorn);
            // Nach unten: Band bis 2,50 m
            assert_eq!(hell(3000.0, 2500.0 + 5.0), 0.0, "{licht:?}");
            assert_eq!(hell(3000.0, 2500.0 - 5.0), 1.0, "{licht:?}");
            // Die Unterkante des Bands (Platte 1,00 … 5,00 m) liegt um den
            // Versatz zur Seite, dazwischen schräg unter 45°
            let z = 2500.0 + 5.0;
            let (a, e) = (1000.0 + versatz, 5000.0 + versatz);
            assert_eq!(hell(a - 15.0, z), 1.0, "{licht:?}");
            assert_eq!(hell(a + 15.0, z), 0.0, "{licht:?}");
            assert_eq!(hell(e - 15.0, z), 0.0, "{licht:?}");
            assert_eq!(hell(e + 15.0, z), 1.0, "{licht:?}");
            // 20 cm unter der Platte: die Kante zur Lichtseite (von der
            // vorderen Unterkante geworfen) um 20 cm versetzt, von oben gar
            // nicht
            let (x, links_hell) = if versatz > 0.0 {
                (1200.0, 1.0)
            } else if versatz < 0.0 {
                (4800.0, 0.0)
            } else {
                (1000.0, 1.0)
            };
            assert_eq!(hell(x - 15.0, 2800.0), links_hell, "{licht:?}");
            assert_eq!(hell(x + 15.0, 2800.0), 1.0 - links_hell, "{licht:?}");
        }
    }

    fn sonne(minutes: u32) -> Sun {
        Sun {
            date: sk_math::sonne::Datum::new(2026, 6, 21).unwrap(),
            minutes,
            on: false,
        }
    }
}

//! Nordpfeil (Sonnenstand S2, Analyse §3.2, §3.6, §8): Mit der Kachel unter
//! „Projektdaten“ wird der Pfeil im 3D-Fenster oder im Grundriss aufgezogen,
//! erst der Fußpunkt, dann die Richtung (Einrasten 5°, mit Umschalt 15°;
//! Zahl + Enter setzt genau). Danach steht er in 3D und im Grundriss, ist
//! anklickbar, an der Spitze drehbar („Nordrichtung geändert“) und am Schaft
//! verschiebbar („Nordpfeil verschoben“), je ein Schritt beim Loslassen.
//!
//! Liegt der Fußpunkt im Hüllquader des Gebäudes oder fehlt er, steht der
//! Pfeil vorne links daneben; geschrieben wird dabei nichts (§8, 07:10).
//! Die Entscheidungen stehen als reine Funktionen oben.

use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::ui::MeasureKind;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::Foot;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::{Helper, SOLID};

/// Schritt beim Aufziehen und Drehen.
pub const LABEL_DREHEN: &str = "Nordrichtung geändert";
/// Schritt beim Verschieben.
pub const LABEL_SCHIEBEN: &str = "Nordpfeil verschoben";

/// Länge des Pfeils im Grundriss (dip) und in 3D (mm).
const LAENGE_PX: f64 = 64.0;
const LAENGE_3D: f64 = 3000.0;
/// Abstand des Platzes vorne links vom Gebäude (mm).
const ABSTAND: f64 = 2000.0;
/// Greifabstand (dip) am Schaft und um die Spitze.
const PICK_PX: f64 = 8.0;
const SPITZE_PX: f64 = 12.0;
/// Raster beim Verschieben (mm).
const STEP: f64 = 10.0;
/// Luft zur Kante, wenn der Pfeil ins Gebäude geschoben wird (mm, §8 08:05).
pub const LUFT: f64 = 500.0;

// ===== Reine Funktionen =====

/// Richtung von `a` nach `b` in Grad im Uhrzeigersinn von +y (0 … < 360);
/// `None` für denselben Punkt.
pub fn winkel(a: Foot, b: Foot) -> Option<f64> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    (dx.hypot(dy) > 1e-6).then(|| dx.atan2(dy).to_degrees().rem_euclid(360.0))
}

/// Auf 5° (mit Umschalt 15°) gerundet, 0 … < 360.
pub fn einrasten(w: f64, grob: bool) -> f64 {
    let step = if grob { 15.0 } else { 5.0 };
    ((w / step).round() * step).rem_euclid(360.0)
}

/// Einheitsvektor der Nordrichtung `nord` (Grad im Uhrzeigersinn von +y).
pub fn richtung(nord: f64) -> [f64; 2] {
    let r = nord.to_radians();
    [r.sin(), r.cos()]
}

/// Wo der Pfeil steht: am Fußpunkt, außer er fehlt oder liegt im
/// Hüllquader des Gebäudes (Grundriss); dann vorne links daneben, ohne
/// Gebäude am Ursprung.
pub fn anzeige_fuss(fuss: Option<Foot>, bounds: Option<(Vec3, Vec3)>) -> Foot {
    let innen = |p: Foot, (lo, hi): (Vec3, Vec3)| {
        (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1])
    };
    match (fuss, bounds) {
        (Some(p), Some(b)) if !innen(p, b) => p,
        (Some(p), None) => p,
        (_, Some((lo, _))) => [lo.x - ABSTAND, lo.y - ABSTAND],
        (None, None) => [0.0, 0.0],
    }
}

/// Fußpunkt beim Verschieben (§8 08:05): Liegt `p` im Hüllquader des
/// Gebäudes, rastet er auf den nächsten Punkt außerhalb, [`LUFT`] vor der
/// nächsten Kante; sonst bleibt er. Gezeigt und geschrieben wird derselbe
/// Punkt, ohne Sprung.
pub fn vor_der_kante(p: Foot, bounds: Option<(Vec3, Vec3)>) -> Foot {
    let Some((lo, hi)) = bounds else {
        return p;
    };
    if !((lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1])) {
        return p;
    }
    let kanten = [
        (p[0] - lo.x, [lo.x - LUFT, p[1]]),
        (hi.x - p[0], [hi.x + LUFT, p[1]]),
        (p[1] - lo.y, [p[0], lo.y - LUFT]),
        (hi.y - p[1], [p[0], hi.y + LUFT]),
    ];
    kanten
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(p, |k| k.1)
}

/// Text der Kachel und der Pille: „N 12°“.
pub fn grad_text(nord: f64) -> String {
    format!("N {}°", grad(nord))
}

/// Grad auf eine Stelle mit Komma; 359,96° heißt „0“, nicht „360“.
fn grad(nord: f64) -> String {
    let g = ((nord * 10.0).round() / 10.0).rem_euclid(360.0);
    g.to_string().replace('.', ",")
}

/// Tooltip der Kachel (§8 07:30): „Nordrichtung 12° · Sonnenstand“, ohne
/// Nordrichtung „Nordrichtung festlegen“.
pub fn tip(nord: Option<f64>) -> String {
    nord.map_or_else(
        || "Nordrichtung festlegen".into(),
        |n| format!("Nordrichtung {}° · Sonnenstand", grad(n)),
    )
}

/// Spitze des Pfeils, in Vielfachen der Länge: Länge und halbe Breite.
const KOPF: f64 = 0.3;
const BREIT: f64 = 0.14;
/// „N“ vor der Spitze: Abstand seiner Mitte vom Fußpunkt, Höhe, Breite.
const N_AB: f64 = 1.3;
const N_HOCH: f64 = 0.22;
const N_BREIT: f64 = 0.16;

/// Umriss des Pfeils (mm): Schaft bis zur Spitze, die Spitze als offenes
/// Dreieck, davor das „N“.
pub fn linien(fuss: Foot, nord: f64, laenge: f64) -> Vec<(Foot, Foot)> {
    let at = achse(fuss, nord);
    let l = laenge;
    let (k, b) = (l - KOPF * l, BREIT * l);
    let mut v = vec![
        (at(0.0, 0.0), at(l, 0.0)),
        (at(l, 0.0), at(k, -b)),
        (at(l, 0.0), at(k, b)),
        (at(k, -b), at(k, b)),
    ];
    v.extend(n_linien(fuss, nord, l));
    v
}

/// Das „N“ vor der Spitze, in Pfeilrichtung versetzt, aber aufrecht zur
/// Modellachse +y: im Grundriss immer lesbar (Befund B, §8 07:55).
pub fn n_linien(fuss: Foot, nord: f64, laenge: f64) -> [(Foot, Foot); 3] {
    let [dx, dy] = richtung(nord);
    let (cx, cy) = (fuss[0] + dx * N_AB * laenge, fuss[1] + dy * N_AB * laenge);
    let (h, b) = (N_HOCH * laenge * 0.5, N_BREIT * laenge * 0.5);
    [
        ([cx - b, cy - h], [cx - b, cy + h]),
        ([cx - b, cy + h], [cx + b, cy - h]),
        ([cx + b, cy - h], [cx + b, cy + h]),
    ]
}

/// Punkt `v` längs der Nordrichtung und `u` quer nach rechts davon.
fn achse(fuss: Foot, nord: f64) -> impl Fn(f64, f64) -> Foot {
    let [dx, dy] = richtung(nord);
    // Quer nach rechts
    let (qx, qy) = (dy, -dx);
    move |v: f64, u: f64| [fuss[0] + dx * v + qx * u, fuss[1] + dy * v + qy * u]
}

/// Länge des Pfeils (mm): im Grundriss gleich groß auf dem Bildschirm, in
/// 3D fest.
pub fn laenge(cam: &Camera, h: f64, scale: f64) -> f64 {
    cam.ortho.map_or(LAENGE_3D, |half| {
        LAENGE_PX * scale * 2.0 * half / h.max(1.0)
    })
}

// ===== Zustand =====

/// Was die Maus am gesetzten Pfeil trifft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Griff {
    /// Spitze: drehen.
    Spitze,
    /// Schaft: verschieben.
    Schaft,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Zug {
    Drehen,
    /// Abstand vom Griffpunkt zum Fußpunkt.
    Schieben([f64; 2]),
}

/// Der Stand des Modells, an dem der Pfeil hängt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    /// Gesetzte Nordrichtung.
    pub nord: Option<f64>,
    /// Gezeigter Fußpunkt ([`anzeige_fuss`]).
    pub fuss: Foot,
    /// Fußpunkt im Modell; Drehen lässt ihn, wie er ist.
    pub gesetzt: Option<Foot>,
}

/// Ein Schritt für das Modell: Bezeichnung, Richtung, Fußpunkt.
pub type Setzen = (&'static str, f64, Option<Foot>);

#[derive(Default)]
pub struct Ausgang {
    pub redraw: bool,
    pub consumed: bool,
    pub commit: Option<Setzen>,
}

#[derive(Default)]
pub struct Nordpfeil {
    /// Werkzeug „Aufziehen“ eingeschaltet (Kachel).
    pub aktiv: bool,
    /// Beim Aufziehen: Fußpunkt gesetzt, die Richtung folgt der Maus.
    anfang: Option<Foot>,
    /// Vorschau beim Aufziehen, Drehen und Verschieben: Richtung und
    /// Fußpunkt, solange die Maus nicht losgelassen ist.
    vorschau: Option<(f64, Foot)>,
    /// Zahl + Enter beim Aufziehen.
    eingabe: Option<MeasureInput>,
    hover: Option<Griff>,
    zug: Option<Zug>,
    /// Hüllquader des Gebäudes (die App setzt ihn vor jedem Ereignis):
    /// Verschieben hinein rastet davor ein ([`vor_der_kante`]).
    pub gebaeude: Option<(Vec3, Vec3)>,
    /// Wo die Taste beim Setzen des Fußpunkts gedrückt wurde (Bildpunkt):
    /// Loslassen weit genug davon setzt die Richtung (Aufziehen in einem
    /// Zug).
    unten: Option<(f64, f64)>,
}

impl Nordpfeil {
    /// Werkzeug ein- oder ausschalten; aus verwirft Angefangenes.
    pub fn set_aktiv(&mut self, on: bool) {
        self.aktiv = on;
        self.anfang = None;
        self.eingabe = None;
        self.unten = None;
        if self.zug.is_none() {
            self.vorschau = None;
        }
    }

    /// Werkzeug an, mitten im Drehen oder Verschieben oder über dem Pfeil:
    /// Band, Schnittlinie und Wandwerkzeug bekommen die Maus nicht.
    pub fn is_busy(&self) -> bool {
        self.aktiv || self.zieht() || self.hover.is_some()
    }

    /// Mitten im Aufziehen, Drehen oder Verschieben (ohne bloßes
    /// Darüberfahren): Tastenkürzel warten.
    pub fn zieht(&self) -> bool {
        self.anfang.is_some() || self.zug.is_some()
    }

    /// Maus über einem Griff (Zeiger „Hand“).
    pub fn over(&self) -> Option<Griff> {
        self.hover
    }

    pub fn input(&self) -> Option<&MeasureInput> {
        self.eingabe.as_ref()
    }

    /// Der gezeigte Pfeil: Richtung und Fußpunkt (Vorschau oder Modell).
    pub fn gezeigt(&self, st: Stand) -> Option<(f64, Foot)> {
        self.vorschau.or(st.nord.map(|n| (n, st.fuss)))
    }

    /// Esc: erst die Eingabe, dann der angefangene Pfeil bzw. das Drehen
    /// oder Verschieben, dann das Werkzeug. `true`, wenn etwas geschlossen
    /// wurde.
    pub fn escape(&mut self) -> bool {
        if self.eingabe.take().is_some() {
            return true;
        }
        if self.zug.take().is_some() {
            self.vorschau = None;
            return true;
        }
        if self.anfang.take().is_some() {
            self.vorschau = None;
            return true;
        }
        if self.aktiv {
            self.set_aktiv(false);
            return true;
        }
        false
    }

    /// Griff unter der Maus (Bildkoordinaten der Ansicht).
    fn griff(
        &self,
        st: Stand,
        cam: &Camera,
        m: (f64, f64),
        w: f64,
        h: f64,
        s: f64,
    ) -> Option<Griff> {
        let (nord, fuss) = self.gezeigt(st)?;
        let l = laenge(cam, h, s);
        let [dx, dy] = richtung(nord);
        let p = |q: Foot| cam.project(vec3(q[0], q[1], 0.0), w, h);
        let a = p(fuss)?;
        let b = p([fuss[0] + dx * l, fuss[1] + dy * l])?;
        let d = |q: (f64, f64)| (q.0 - m.0).hypot(q.1 - m.1);
        if d(b) < SPITZE_PX * s {
            Some(Griff::Spitze)
        } else if dist_to_segment(m, a, b) < PICK_PX * s {
            Some(Griff::Schaft)
        } else {
            None
        }
    }

    /// Ereignis der Ansicht (Koordinaten ohne Titelleiste). `enabled`: 3D
    /// oder Grundriss und kein anderes Werkzeug.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        st: Stand,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
    ) -> Ausgang {
        let mut out = Ausgang::default();
        if !enabled && self.zug.is_none() && self.anfang.is_none() {
            out.redraw = self.hover.take().is_some();
            return out;
        }
        // Auf 10 mm: ein Pfeil am Ursprung steht genau bei x=0 y=0
        let r = |v: f64| (v / STEP).round() * STEP + 0.0;
        let boden = |x: f64, y: f64| cam.ground_point(x, y, w, h).map(|g| [r(g.x), r(g.y)]);
        match *e {
            Event::MouseMove { x, y, mods } => {
                if let Some(a) = self.anfang {
                    if self.eingabe.is_none() {
                        if let Some(n) = boden(x, y).and_then(|g| winkel(a, g)) {
                            self.vorschau = Some((einrasten(n, mods.shift), a));
                            out.redraw = true;
                        }
                    }
                    out.consumed = true;
                } else if let Some(z) = self.zug {
                    let Some(g) = boden(x, y) else {
                        return out;
                    };
                    let (n, f) = self.gezeigt(st).unwrap_or((0.0, st.fuss));
                    let neu = match z {
                        Zug::Drehen => winkel(f, g).map(|w| (einrasten(w, mods.shift), f)),
                        Zug::Schieben(off) => {
                            let p = [r(g[0] - off[0]), r(g[1] - off[1])];
                            Some((n, vor_der_kante(p, self.gebaeude)))
                        }
                    };
                    if neu.is_some() && neu != self.vorschau {
                        self.vorschau = neu;
                        out.redraw = true;
                    }
                    out.consumed = true;
                } else if !self.aktiv {
                    let g = self.griff(st, cam, (x, y), w, h, scale);
                    out.redraw = g != self.hover;
                    self.hover = g;
                }
            }
            Event::MouseLeave if self.zug.is_none() => {
                out.redraw = self.hover.take().is_some();
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if self.aktiv {
                    out.consumed = true;
                    let Some(g) = boden(x, y) else {
                        return out;
                    };
                    match self.anfang {
                        None => {
                            self.anfang = Some(g);
                            self.unten = Some((x, y));
                            self.vorschau = Some((st.nord.unwrap_or(0.0), g));
                        }
                        Some(a) => self.aufgezogen(a, g, &mut out),
                    }
                    out.redraw = true;
                    return out;
                }
                let Some(griff) = self.griff(st, cam, (x, y), w, h, scale) else {
                    return out;
                };
                let (n, f) = self.gezeigt(st).unwrap_or((0.0, st.fuss));
                self.zug = Some(match griff {
                    Griff::Spitze => Zug::Drehen,
                    Griff::Schaft => {
                        let g = boden(x, y).unwrap_or(f);
                        Zug::Schieben([g[0] - f[0], g[1] - f[1]])
                    }
                });
                self.vorschau = Some((n, f));
                self.hover = Some(griff);
                out.consumed = true;
                out.redraw = true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(z) = self.zug.take() {
                    if let (Some((n, f)), Some(alt)) = (self.vorschau.take(), st.nord) {
                        // Drehen schreibt keinen Fußpunkt, der nur gezeigt ist (§8)
                        out.commit = match z {
                            Zug::Drehen if n != alt => Some((LABEL_DREHEN, n, st.gesetzt)),
                            Zug::Schieben(_) if f != st.fuss => Some((LABEL_SCHIEBEN, n, Some(f))),
                            _ => None,
                        };
                    }
                    out.consumed = true;
                    out.redraw = true;
                } else if self.aktiv {
                    // Mit gedrückter Taste aufgezogen: Loslassen setzt
                    let weit = |(a, b): (f64, f64)| (x - a).hypot(y - b) >= PICK_PX * scale;
                    if let (Some(a), true) = (self.anfang, self.unten.take().is_some_and(weit)) {
                        if let Some(g) = boden(x, y) {
                            self.aufgezogen(a, g, &mut out);
                            out.redraw = true;
                        }
                    }
                    out.consumed = true;
                }
            }
            _ => {}
        }
        out
    }

    /// Zweiter Punkt beim Aufziehen: die Richtung der Vorschau gilt (sie
    /// ist eingerastet), solange er nicht auf dem Fußpunkt liegt.
    fn aufgezogen(&mut self, a: Foot, g: Foot, out: &mut Ausgang) {
        if let Some((n, _)) = self.vorschau.filter(|_| winkel(a, g).is_some()) {
            out.commit = Some((LABEL_DREHEN, n, Some(a)));
            self.set_aktiv(false);
            self.vorschau = None;
        }
    }

    /// Taste beim Aufziehen: Zahl öffnet die Eingabe der Richtung, Enter
    /// setzt sie. `true`, wenn verbraucht.
    pub fn key(&mut self, key: Key, mods: Modifiers, out: &mut Ausgang) -> bool {
        let Some(a) = self.anfang else {
            return false;
        };
        let Some(i) = self.eingabe.as_mut() else {
            let Key::Char(ch) = key else {
                return false;
            };
            if !opens(ch, MeasureKind::Angle, mods) {
                return false;
            }
            let mut i = MeasureInput::new(MeasureKind::Angle, None);
            i.push(ch);
            self.eingabe = Some(i);
            self.vorschau_aus_eingabe(a);
            out.redraw = true;
            return true;
        };
        match i.key(key, mods) {
            InputOutcome::Ignored => return false,
            InputOutcome::Changed | InputOutcome::Refused => {}
            InputOutcome::Emptied | InputOutcome::Escape => self.eingabe = None,
            InputOutcome::EnterEmpty => self.eingabe = None,
            InputOutcome::Enter => {
                if let Some(Ok(n)) = i.value(0) {
                    out.commit = Some((LABEL_DREHEN, n.rem_euclid(360.0), Some(a)));
                    self.set_aktiv(false);
                    self.vorschau = None;
                }
            }
        }
        if self.eingabe.is_some() {
            self.vorschau_aus_eingabe(a);
        }
        out.redraw = true;
        true
    }

    fn vorschau_aus_eingabe(&mut self, a: Foot) {
        if let Some(Ok(n)) = self.eingabe.as_ref().and_then(|i| i.value(0)) {
            self.vorschau = Some((n.rem_euclid(360.0), a));
        }
    }

    /// Pille beim Aufziehen und Drehen: Spitze des Pfeils und Text.
    pub fn label(&self, cam: &Camera, h: f64, scale: f64) -> Option<(Vec3, String)> {
        if self.anfang.is_none() && self.zug != Some(Zug::Drehen) {
            return None;
        }
        let (n, f) = self.vorschau?;
        // Hinter dem „N“
        let l = laenge(cam, h, scale) * 1.9;
        let [dx, dy] = richtung(n);
        let text = match &self.eingabe {
            Some(i) => i.text(["Nord", ""]),
            None => grad_text(n),
        };
        Some((vec3(f[0] + dx * l, f[1] + dy * l, 0.0), text))
    }

    /// Linien für den Renderer (Grundriss und 3D).
    #[allow(clippy::too_many_arguments)]
    pub fn helpers(
        &self,
        st: Stand,
        cam: &Camera,
        h: f64,
        scale: f32,
        ink: [f32; 4],
        hot: [f32; 4],
        width: f32,
    ) -> Vec<Helper> {
        let Some((n, f)) = self.gezeigt(st) else {
            return Vec::new();
        };
        let color = if self.zug.is_some() || self.anfang.is_some() || self.hover.is_some() {
            hot
        } else {
            ink
        };
        let l = laenge(cam, h, scale as f64);
        linien(f, n, l)
            .into_iter()
            .map(|(a, b)| Helper {
                a: [a[0] as f32, a[1] as f32, 2.0],
                b: [b[0] as f32, b[1] as f32, 2.0],
                color,
                width: width * scale,
                dash: 0.0,
                pattern: SOLID,
                occlude: false,
                round: true,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winkel_und_einrasten() {
        assert_eq!(winkel([0.0, 0.0], [0.0, 5.0]), Some(0.0));
        assert_eq!(winkel([0.0, 0.0], [5.0, 0.0]), Some(90.0));
        assert_eq!(winkel([1.0, 1.0], [1.0, -4.0]), Some(180.0));
        assert!((winkel([0.0, 0.0], [-1.0, 1.0]).unwrap() - 315.0).abs() < 1e-9);
        assert_eq!(winkel([2.0, 2.0], [2.0, 2.0]), None);
        assert_eq!(einrasten(12.4, false), 10.0);
        assert_eq!(einrasten(12.6, false), 15.0);
        assert_eq!(einrasten(22.0, true), 15.0);
        assert_eq!(einrasten(358.0, false), 0.0);
        let [x, y] = richtung(90.0);
        assert!((x - 1.0).abs() < 1e-12 && y.abs() < 1e-12);
        assert_eq!(grad_text(12.0), "N 12°");
        assert_eq!(grad_text(347.5), "N 347,5°");
        assert_eq!(grad_text(359.96), "N 0°");
        assert_eq!(tip(None), "Nordrichtung festlegen");
        assert_eq!(tip(Some(12.0)), "Nordrichtung 12° · Sonnenstand");
    }

    /// §8: Fehlt der Fußpunkt oder liegt er im Gebäude, steht der Pfeil
    /// vorne links daneben; ohne Gebäude am Ursprung.
    #[test]
    fn anzeige_neben_dem_gebaeude() {
        let b = Some((vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 6000.0)));
        assert_eq!(anzeige_fuss(None, None), [0.0, 0.0]);
        assert_eq!(anzeige_fuss(Some([5.0, 6.0]), None), [5.0, 6.0]);
        assert_eq!(anzeige_fuss(None, b), [-2000.0, -2000.0]);
        assert_eq!(anzeige_fuss(Some([0.0, 0.0]), b), [-2000.0, -2000.0]);
        assert_eq!(anzeige_fuss(Some([5000.0, 4000.0]), b), [-2000.0, -2000.0]);
        assert_eq!(anzeige_fuss(Some([12000.0, 0.0]), b), [12000.0, 0.0]);
    }

    /// Der Pfeil zeigt in die Nordrichtung: die Spitze liegt bei Nord 90°
    /// rechts vom Fußpunkt, das „N“ davor.
    #[test]
    fn linien_zeigen_nach_norden() {
        let l = linien([100.0, 200.0], 90.0, 1000.0);
        let (a, b) = l[0];
        assert_eq!(a, [100.0, 200.0]);
        assert!((b[0] - 1100.0).abs() < 1e-9 && (b[1] - 200.0).abs() < 1e-9);
        assert_eq!(l.len(), 7);
        assert!(l[4..].iter().all(|(p, q)| p[0] > 1100.0 && q[0] > 1100.0));
    }

    /// Befund B (§8 07:55): Das „N“ steht aufrecht zu +y, bei jeder
    /// Nordrichtung dieselben Linien, nur verschoben, und frei vor der
    /// Spitze.
    #[test]
    fn n_bleibt_aufrecht() {
        let f = [100.0, 200.0];
        let n0 = n_linien(f, 0.0, 1000.0);
        for nord in [90.0, 180.0, 270.0, 33.0] {
            let n = n_linien(f, nord, 1000.0);
            let [dx, dy] = richtung(nord);
            let d = [(dx - 0.0) * N_AB * 1000.0, (dy - 1.0) * N_AB * 1000.0];
            for (a, b) in n.iter().zip(&n0) {
                for (p, q) in [(a.0, b.0), (a.1, b.1)] {
                    assert!((p[0] - q[0] - d[0]).abs() < 1e-9, "{nord}");
                    assert!((p[1] - q[1] - d[1]).abs() < 1e-9, "{nord}");
                }
            }
            // Senkrechte Striche des N laufen längs y
            assert_eq!(n[0].0[0], n[0].1[0]);
            assert!(n[0].1[1] > n[0].0[1]);
            // Ganz vor der Spitze: kein Punkt des N näher am Fußpunkt als die Spitze
            let spitze = 1000.0;
            for (p, q) in n {
                for r in [p, q] {
                    assert!((r[0] - f[0]).hypot(r[1] - f[1]) > spitze * 1.05, "{nord}");
                }
            }
        }
    }

    #[test]
    fn vor_der_kante_rastet() {
        let b = Some((vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 3000.0)));
        assert_eq!(vor_der_kante([-1.0, 5.0], b), [-1.0, 5.0]);
        assert_eq!(vor_der_kante([5.0, 5.0], None), [5.0, 5.0]);
        assert_eq!(vor_der_kante([1000.0, 4000.0], b), [-LUFT, 4000.0]);
        assert_eq!(vor_der_kante([9000.0, 4000.0], b), [10000.0 + LUFT, 4000.0]);
        assert_eq!(vor_der_kante([5000.0, 700.0], b), [5000.0, -LUFT]);
        assert_eq!(vor_der_kante([5000.0, 7900.0], b), [5000.0, 8000.0 + LUFT]);
        // Auf der Kante zählt als innen
        assert_eq!(vor_der_kante([0.0, 4000.0], b), [-LUFT, 4000.0]);
    }
}

#[cfg(test)]
#[path = "nordpfeil_istbilder.rs"]
mod istbilder;

#[cfg(test)]
mod abnahme_tests {
    use super::*;
    use crate::scene::Scene;
    use sk_model::{szo, GuidGen, Model, RefSide, WallChain};
    use std::f64::consts::FRAC_PI_2;

    const W: f64 = 1000.0;
    const H: f64 = 800.0;
    const M: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn grundriss() -> Camera {
        Camera::parallel(vec3(0.0, 0.0, 0.0), FRAC_PI_2, -FRAC_PI_2, 10000.0)
    }

    fn bild(cam: &Camera, p: Foot) -> (f64, f64) {
        cam.project(vec3(p[0], p[1], 0.0), W, H).unwrap()
    }

    fn stand(s: &Scene) -> Stand {
        let m = s.model();
        Stand {
            nord: m.location().north,
            fuss: anzeige_fuss(m.north_foot(), s.bounds()),
            gesetzt: m.north_foot(),
        }
    }

    /// Ein Ereignis an den Pfeil und, wie in der App, ein Schritt daraus.
    fn ereignis(n: &mut Nordpfeil, s: &mut Scene, cam: &Camera, e: Event) -> Ausgang {
        n.gebaeude = s.bounds();
        let out = n.handle(&e, stand(s), cam, W, H, 1.0, true);
        if let Some((label, nord, fuss)) = out.commit {
            s.nordpfeil_setzen(label, nord, fuss);
        }
        out
    }

    fn unten(cam: &Camera, p: Foot) -> Event {
        let (x, y) = bild(cam, p);
        Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        }
    }

    fn oben(cam: &Camera, p: Foot) -> Event {
        let (x, y) = bild(cam, p);
        Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods: M,
        }
    }

    fn zu(cam: &Camera, p: Foot, shift: bool) -> Event {
        let (x, y) = bild(cam, p);
        Event::MouseMove {
            x,
            y,
            mods: Modifiers { shift, ..M },
        }
    }

    fn rechteck(x: f64) -> WallChain {
        WallChain {
            base: 0.0,
            points: vec![
                vec3(x, -1000.0, 0.0),
                vec3(x, 4000.0, 0.0),
                vec3(x + 5000.0, 4000.0, 0.0),
                vec3(x + 5000.0, -1000.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        }
    }

    fn laden(text: &str) -> Model {
        szo::read_with(text, GuidGen::with_seed(1), &sk_cost::lesen::ABSCHNITTE_SZO)
            .unwrap()
            .model
    }

    /// Abnahme S2 (§8 07:10): Pfeil am Ursprung aufziehen, danach ein
    /// Gebäude darüber zeichnen. Der Pfeil steht sichtbar daneben, die
    /// Datei behält x=0 y=0; Strg+Z stellt die alte Datei her.
    #[test]
    fn abnahme_s2_aufziehen_und_gebaeude_darueber() {
        let cam = grundriss();
        let mut s = Scene::with_model(Model::with_seed(1));
        let leer = szo::write(s.model());
        let mut n = Nordpfeil::default();
        n.set_aktiv(true);
        // Fußpunkt am Ursprung, Richtung nach rechts oben, 5° eingerastet
        assert!(ereignis(&mut n, &mut s, &cam, unten(&cam, [3.0, -2.0])).consumed);
        assert!(n.is_busy() && n.zieht());
        ereignis(&mut n, &mut s, &cam, oben(&cam, [3.0, -2.0]));
        assert!(n.zieht(), "Klick ohne Ziehen setzt nur den Fußpunkt");
        ereignis(&mut n, &mut s, &cam, zu(&cam, [1000.0, 1100.0], false));
        let (text, pille) = n.label(&cam, H, 1.0).map(|(p, t)| (t, p)).unwrap();
        assert_eq!(text, "N 40°");
        assert!(pille.x > 1000.0 && pille.y > 1000.0, "hinter der Spitze");
        let out = ereignis(&mut n, &mut s, &cam, unten(&cam, [1000.0, 1100.0]));
        assert_eq!(out.commit, Some((LABEL_DREHEN, 40.0, Some([0.0, 0.0]))));
        assert!(!n.aktiv && !n.zieht());
        assert_eq!(s.undo_label(), Some(LABEL_DREHEN));
        let gesetzt = szo::write(s.model());
        assert!(
            gesetzt.contains("[location] north=40 x=0 y=0\n"),
            "{gesetzt}"
        );
        assert_eq!(stand(&s).fuss, [0.0, 0.0]);

        // Gebäude über dem Ursprung: der Pfeil steht vorne links daneben
        s.add_wall(&rechteck(-2500.0)).unwrap();
        let (lo, _) = s.bounds().unwrap();
        let st = stand(&s);
        assert_eq!(st.fuss, [lo.x - ABSTAND, lo.y - ABSTAND]);
        let h = n.helpers(st, &cam, H, 1.0, [1.0; 4], [1.0; 4], 1.5);
        assert_eq!(h.len(), 7, "Umriss und N");
        let mit_haus = szo::write(s.model());
        assert!(mit_haus.contains("[location] north=40 x=0 y=0\n"));
        assert_eq!(szo::write(&laden(&mit_haus)), mit_haus);

        // Drehen am gezeigten Platz schreibt keinen Fußpunkt
        let [dx, dy] = richtung(40.0);
        let l = laenge(&cam, H, 1.0);
        let spitze = [st.fuss[0] + dx * l, st.fuss[1] + dy * l];
        ereignis(&mut n, &mut s, &cam, zu(&cam, spitze, false));
        assert_eq!(n.over(), Some(Griff::Spitze));
        assert!(ereignis(&mut n, &mut s, &cam, unten(&cam, spitze)).consumed);
        let rechts = [st.fuss[0] + 5000.0, st.fuss[1] + 300.0];
        ereignis(&mut n, &mut s, &cam, zu(&cam, rechts, true));
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, rechts));
        assert_eq!(out.commit, Some((LABEL_DREHEN, 90.0, Some([0.0, 0.0]))));
        assert!(szo::write(s.model()).contains("[location] north=90 x=0 y=0\n"));

        // Verschieben am Schaft: neuer Fußpunkt, eigener Schritt
        let st = stand(&s);
        let mitte = [st.fuss[0] + l * 0.5, st.fuss[1]];
        ereignis(&mut n, &mut s, &cam, zu(&cam, mitte, false));
        assert_eq!(n.over(), Some(Griff::Schaft));
        ereignis(&mut n, &mut s, &cam, unten(&cam, mitte));
        let hin = [mitte[0] - 1234.0, mitte[1] - 3000.0];
        ereignis(&mut n, &mut s, &cam, zu(&cam, hin, false));
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, hin));
        let (label, nord, fuss) = out.commit.unwrap();
        assert_eq!((label, nord), (LABEL_SCHIEBEN, 90.0));
        let f = fuss.unwrap();
        assert!(f.iter().all(|v| v % STEP == 0.0), "{f:?}");
        assert!((f[0] - (st.fuss[0] - 1234.0)).abs() <= STEP);
        assert_eq!(s.undo_label(), Some(LABEL_SCHIEBEN));
        assert_eq!(stand(&s).fuss, f);

        // Strg+Z bis zum Anfang: bytegleich; Strg+Y wieder hin
        let ende = szo::write(s.model());
        assert!(s.undo() && s.undo());
        assert_eq!(szo::write(s.model()), mit_haus);
        assert!(s.undo() && s.undo());
        // Bis auf die Zähler der Kennungen (bleiben nach Rückgängig)
        let ohne_projekt =
            |t: &str| -> String { t.lines().filter(|z| !z.starts_with("[project]")).collect() };
        assert!(ohne_projekt(&szo::write(s.model())) == ohne_projekt(&leer));
        while s.redo() {}
        assert_eq!(szo::write(s.model()), ende);
    }

    /// Aufziehen in einem Zug (Taste halten), Zahl + Enter, Esc der Reihe
    /// nach.
    #[test]
    fn aufziehen_ziehen_zahl_und_esc() {
        let cam = grundriss();
        let mut s = Scene::with_model(Model::with_seed(1));
        let mut n = Nordpfeil::default();
        n.set_aktiv(true);
        ereignis(&mut n, &mut s, &cam, unten(&cam, [500.0, 500.0]));
        ereignis(&mut n, &mut s, &cam, zu(&cam, [500.0, 2500.0], false));
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, [500.0, 2500.0]));
        assert_eq!(out.commit, Some((LABEL_DREHEN, 0.0, Some([500.0, 500.0]))));
        assert_eq!(s.model().north_foot(), Some([500.0, 500.0]));

        // Zahl + Enter
        n.set_aktiv(true);
        ereignis(&mut n, &mut s, &cam, unten(&cam, [0.0, 0.0]));
        let mut out = Ausgang::default();
        for k in [
            Key::Char('3'),
            Key::Char('3'),
            Key::Char(','),
            Key::Char('5'),
        ] {
            assert!(n.key(k, M, &mut out));
        }
        assert!(n.input().is_some());
        assert!(n.label(&cam, H, 1.0).unwrap().1.starts_with("Nord"));
        assert!(n.key(Key::Enter, M, &mut out));
        assert_eq!(out.commit, Some((LABEL_DREHEN, 33.5, Some([0.0, 0.0]))));

        // Esc: Eingabe, Fußpunkt, Werkzeug
        n.set_aktiv(true);
        ereignis(&mut n, &mut s, &cam, unten(&cam, [0.0, 0.0]));
        n.key(Key::Char('7'), M, &mut Ausgang::default());
        assert!(n.escape() && n.input().is_none() && n.zieht());
        assert!(n.escape() && !n.zieht() && n.aktiv);
        assert!(n.escape() && !n.aktiv);
        assert!(!n.escape());
    }

    /// §8 08:05: In die Gebäudemitte geschoben rastet der Fußpunkt schon
    /// beim Ziehen 500 mm vor der nächsten Kante ein; geschrieben wird
    /// genau der gezeigte Punkt.
    #[test]
    fn verschieben_ins_gebaeude_rastet_vor_der_kante() {
        let cam = grundriss();
        let mut s = Scene::with_model(Model::with_seed(1));
        s.add_wall(&rechteck(-2500.0)).unwrap();
        let (lo, hi) = s.bounds().unwrap();
        assert!(s.nordpfeil_setzen(LABEL_DREHEN, 0.0, Some([lo.x - 3000.0, 0.0])));
        let mut n = Nordpfeil::default();
        let st = stand(&s);
        let l = laenge(&cam, H, 1.0);
        let mitte_schaft = [st.fuss[0], st.fuss[1] + l * 0.5];
        ereignis(&mut n, &mut s, &cam, zu(&cam, mitte_schaft, false));
        assert_eq!(n.over(), Some(Griff::Schaft));
        ereignis(&mut n, &mut s, &cam, unten(&cam, mitte_schaft));
        // Fußpunkt in die Mitte, näher an der linken als an den anderen Kanten
        let ziel = [lo.x + 1200.0, (lo.y + hi.y) * 0.5];
        let griff = [ziel[0], ziel[1] + l * 0.5];
        ereignis(&mut n, &mut s, &cam, zu(&cam, griff, false));
        let (_, vorschau) = n.gezeigt(stand(&s)).unwrap();
        assert_eq!(vorschau[0], lo.x - LUFT, "schon beim Ziehen");
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, griff));
        let (label, _, fuss) = out.commit.unwrap();
        assert_eq!(label, LABEL_SCHIEBEN);
        assert_eq!(fuss, Some(vorschau));
        assert_eq!(s.model().north_foot(), Some(vorschau));
        assert_eq!(stand(&s).fuss, vorschau, "gezeigt = geschrieben");
    }

    /// Ein S1-Stand ohne Fußpunkt: der Pfeil steht vorne links, Drehen
    /// schreibt weiter keinen; die Referenzhäuser bleiben ohne Pfeil
    /// bytegleich.
    #[test]
    fn ohne_fusspunkt_und_referenzhaus() {
        let text = include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo");
        let alt = szo::write(&laden(text));
        let mut s = Scene::with_model(laden(&alt));
        assert_eq!(stand(&s).nord, None);
        assert_eq!(szo::write(s.model()), alt);
        assert!(s.nordpfeil_setzen(LABEL_DREHEN, 15.0, None));
        let (lo, _) = s.bounds().unwrap();
        assert_eq!(stand(&s).fuss, [lo.x - ABSTAND, lo.y - ABSTAND]);
        let neu = szo::write(s.model());
        assert!(neu.contains("[location] north=15\n"), "{neu}");
        assert!(s.undo());
        assert_eq!(szo::write(s.model()), alt);
    }
}

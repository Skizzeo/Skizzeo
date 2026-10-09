//! Nordpfeil (Sonnenstand S2, Analyse §3.2, §3.6, §8): Mit der Kachel unter
//! „Projektdaten“ wird der Pfeil im 3D-Fenster oder im Grundriss aufgezogen,
//! erst der Fußpunkt, dann die Richtung (Einrasten 5°, mit Umschalt 15°;
//! Zahl + Enter setzt genau). Danach steht er in 3D und im Grundriss, ist
//! anklickbar, an der Spitze drehbar („Nordrichtung geändert“) und am
//! Pfeilkörper verschiebbar („Nordpfeil verschoben“), je ein Schritt beim
//! Loslassen. Gestalt nach Jörns Skizze (§8 08:42): schlanker
//! Pfeil, längs geteilt, links weiß, rechts gefüllt, mit eingezogenem Fuß;
//! das „N“ aufrecht zum Bildschirm; gemalt als Bild über der Ansicht.
//!
//! Liegt der Fußpunkt im Hüllquader des Gebäudes oder fehlt er, steht der
//! Pfeil vorne links daneben; geschrieben wird dabei nichts (§8, 07:10).
//! Die Entscheidungen stehen als reine Funktionen oben.

use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::ui::MeasureKind;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::Foot;
use sk_paint::{Canvas, Path, Rgba};
use sk_platform::{Event, Key, Modifiers, MouseButton};

/// Schritt beim Aufziehen und Drehen.
pub const LABEL_DREHEN: &str = "Nordrichtung geändert";
/// Schritt beim Verschieben.
pub const LABEL_SCHIEBEN: &str = "Nordpfeil verschoben";

/// Länge des Pfeils im Grundriss (dip) und in 3D (mm).
/// Dreimal so groß wie zuerst (Jörn 08:42: „visuell kaum zu erkennen“).
const LAENGE_PX: f64 = 192.0;
const LAENGE_3D: f64 = 9000.0;
/// Höchstlänge in 3D auf dem Bildschirm (dip), nah am Pfeil.
const KAPPE_PX: f64 = 2.0 * LAENGE_PX;
/// Freiraum um das ganze Zeichen am Platz neben dem Gebäude (mm, §8 08:45).
const FREI: f64 = 1000.0;
/// Greifabstand (dip) am Schaft und um die Spitze.
const PICK_PX: f64 = 8.0;
const SPITZE_PX: f64 = 12.0;
/// Erst ab dieser Strecke (dip) gilt ein Druck auf den Pfeil als Ziehen
/// (wie ein Klick in der Auswahl).
const ZUG_PX: f64 = 4.0;
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

/// Wie weit das Zeichen bei der Länge `laenge` (mm) höchstens vom
/// Fußpunkt reicht: Pfeil, „N“ davor, Ecken des Fußes (obere Schranke).
pub fn n_reichweite(laenge: f64) -> f64 {
    (1.0 + N_LUFT + N_HOCH + N_BREIT) * laenge
}

/// Versatz des Platzes vorne links vom Gebäude in x und y (mm): das ganze
/// Zeichen bleibt mindestens [`FREI`] vom Hüllquader entfernt, bei jeder
/// Nordrichtung (§8 08:45). Wächst im Grundriss mit dem Zoom mit; der
/// Platz wird nie geschrieben.
pub fn ersatz_abstand(laenge: f64) -> f64 {
    FREI + n_reichweite(laenge)
}

/// Wo der Pfeil steht: am Fußpunkt, außer er fehlt oder liegt im
/// Hüllquader des Gebäudes (Grundriss, eine ältere Datei); dann vorne
/// links daneben ([`ersatz_abstand`]), ohne Gebäude am Ursprung.
pub fn anzeige_fuss(fuss: Option<Foot>, bounds: Option<(Vec3, Vec3)>, laenge: f64) -> Foot {
    let innen = |p: Foot, (lo, hi): (Vec3, Vec3)| {
        (lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1])
    };
    match (fuss, bounds) {
        (Some(p), Some(b)) if !innen(p, b) => p,
        (Some(p), None) => p,
        (_, Some((lo, _))) => {
            let a = ersatz_abstand(laenge);
            [lo.x - a, lo.y - a]
        }
        (None, None) => [0.0, 0.0],
    }
}

/// Fußpunkt beim Verschieben (§8 08:05): Liegt `p` im Hüllquader des
/// Gebäudes, rastet er auf den nächsten Punkt außerhalb, [`LUFT`] vor der
/// nächsten Kante; sonst bleibt er. Gezeigt und geschrieben wird derselbe
/// Punkt, ohne Sprung. Die eingerastete Koordinate ist auf [`STEP`] nach
/// außen gerundet, vom Gebäude weg (§8 08:50): die Luft bleibt mindestens
/// [`LUFT`], die Datei hat Rasterwerte.
pub fn vor_der_kante(p: Foot, bounds: Option<(Vec3, Vec3)>) -> Foot {
    let Some((lo, hi)) = bounds else {
        return p;
    };
    if !((lo.x..=hi.x).contains(&p[0]) && (lo.y..=hi.y).contains(&p[1])) {
        return p;
    }
    let ab = |v: f64| (v / STEP).floor() * STEP + 0.0;
    let auf = |v: f64| (v / STEP).ceil() * STEP + 0.0;
    let kanten = [
        (p[0] - lo.x, [ab(lo.x - LUFT), p[1]]),
        (hi.x - p[0], [auf(hi.x + LUFT), p[1]]),
        (p[1] - lo.y, [p[0], ab(lo.y - LUFT)]),
        (hi.y - p[1], [p[0], auf(hi.y + LUFT)]),
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

/// Tooltip der Kachel: ohne Nordrichtung „Nordrichtung festlegen“, sonst
/// der Schalter des Sonnenstands (§8 09:25).
pub fn tip(nord: Option<f64>, sonne: bool) -> String {
    match (nord, sonne) {
        (None, _) => "Nordrichtung festlegen".into(),
        (Some(_), true) => "Sonnenstand an".into(),
        (Some(_), false) => "Sonnenstand aus".into(),
    }
}

/// Halb gefüllter Pfeil (Jörns Skizze, §8 08:42), in Vielfachen der Länge
/// vom Fußpunkt (Mitte der Grundlinie) bis zur Spitze: halbe Breite der
/// Grundlinie, wie weit der Fuß in der Mitte nach vorn eingezogen ist.
const BREIT: f64 = 0.18;
const KERBE: f64 = 0.15;
/// „N“ vor der Spitze: Luft zur Spitze, Höhe, Breite.
const N_LUFT: f64 = 0.06;
const N_HOCH: f64 = 0.12;
const N_BREIT: f64 = 0.09;

/// Das Zeichen in Modellkoordinaten (mm): schlanker Pfeil, längs geteilt,
/// mit eingezogenem Fuß; die linke Hälfte weiß, die rechte gefüllt.
#[derive(Clone, Debug, PartialEq)]
pub struct Gestalt {
    pub spitze: Foot,
    /// Linke und rechte Ecke des Fußes.
    pub links: Foot,
    pub rechts: Foot,
    /// Eingezogene Mitte des Fußes, Ende der Teilungslinie.
    pub kerbe: Foot,
}

pub fn gestalt(fuss: Foot, nord: f64, laenge: f64) -> Gestalt {
    let at = achse(fuss, nord);
    let l = laenge;
    Gestalt {
        spitze: at(l, 0.0),
        links: at(0.0, -BREIT * l),
        rechts: at(0.0, BREIT * l),
        kerbe: at(KERBE * l, 0.0),
    }
}

/// Die Striche des „N“ im Bild (Pixel, y nach unten) um `mitte`, aufrecht
/// zum Bildschirm (Befund B, §8 07:55 und 08:20), `hoch` hoch.
pub fn n_striche(mitte: (f32, f32), hoch: f32) -> [((f32, f32), (f32, f32)); 3] {
    let (cx, cy) = mitte;
    let (h, b) = (hoch * 0.5, hoch * 0.5 * (N_BREIT / N_HOCH) as f32);
    [
        ((cx - b, cy + h), (cx - b, cy - h)),
        ((cx - b, cy - h), (cx + b, cy + h)),
        ((cx + b, cy + h), (cx + b, cy - h)),
    ]
}

/// Liegt `m` im Dreieck `a`, `b`, `c` (beliebiger Umlaufsinn)?
fn im_dreieck(m: (f64, f64), a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> bool {
    let kreuz =
        |p: (f64, f64), q: (f64, f64)| (q.0 - p.0) * (m.1 - p.1) - (q.1 - p.1) * (m.0 - p.0);
    let (x, y, z) = (kreuz(a, b), kreuz(b, c), kreuz(c, a));
    (x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0)
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

/// Länge des Pfeils am Fußpunkt `fuss` (mm): wie [`laenge`], in 3D aber
/// auf dem Bildschirm höchstens [`KAPPE_PX`] lang, damit das Bild nah am
/// Pfeil klein bleibt und schnell gemalt ist (Review 3bu-1). Greifen,
/// Pille und Bild nehmen dieselbe Länge.
pub fn laenge_bei(cam: &Camera, (w, h): (f64, f64), scale: f64, fuss: Foot) -> f64 {
    let l = laenge(cam, h, scale);
    if cam.ortho.is_some() {
        return l;
    }
    let r = cam.right();
    let a = cam.project(vec3(fuss[0], fuss[1], 0.0), w, h);
    let b = cam.project(vec3(fuss[0] + r.x * l, fuss[1] + r.y * l, r.z * l), w, h);
    match (a, b) {
        (Some(a), Some(b)) => {
            let px = (b.0 - a.0).hypot(b.1 - a.1);
            let max = KAPPE_PX * scale;
            if px > max {
                l * max / px
            } else {
                l
            }
        }
        _ => l,
    }
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
    /// Klick auf den Pfeil ohne Ziehen: schaltet den Sonnenstand (S4).
    pub klick: bool,
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
    /// Am Pfeil gedrückt (Bildpunkt), noch nicht weiter als [`ZUG_PX`]
    /// bewegt: Loslassen ist dann ein Klick ohne Schritt (§8 09:25).
    druck: Option<(f64, f64)>,
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
            self.druck = None;
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
        let l = laenge_bei(cam, (w, h), s, fuss);
        let [dx, dy] = richtung(nord);
        let p = |q: Foot| cam.project(vec3(q[0], q[1], 0.0), w, h);
        let a = p(fuss)?;
        let b = p([fuss[0] + dx * l, fuss[1] + dy * l])?;
        let d = |q: (f64, f64)| (q.0 - m.0).hypot(q.1 - m.1);
        // Der ganze Pfeilkörper greift zum Verschieben
        let g = gestalt(fuss, nord, l);
        let im_pfeil = match (p(g.spitze), p(g.links), p(g.rechts)) {
            (Some(t), Some(li), Some(re)) => im_dreieck(m, t, li, re),
            _ => false,
        };
        if d(b) < SPITZE_PX * s {
            Some(Griff::Spitze)
        } else if dist_to_segment(m, a, b) < PICK_PX * s || im_pfeil {
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
                            self.vorschau = Some((einrasten(n, mods.shift), self.fuss_aus(a)));
                            out.redraw = true;
                        }
                    }
                    out.consumed = true;
                } else if let Some(z) = self.zug {
                    // Bis zur Zug-Schwelle bleibt der Pfeil stehen
                    if let Some((a, b)) = self.druck {
                        if (x - a).hypot(y - b) < ZUG_PX * scale {
                            out.consumed = true;
                            return out;
                        }
                        self.druck = None;
                    }
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
                            self.vorschau = Some((st.nord.unwrap_or(0.0), self.fuss_aus(g)));
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
                self.druck = Some((x, y));
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
                    if self.druck.take().is_some() {
                        // Klick ohne Ziehen: kein Schritt
                        self.vorschau = None;
                        out.klick = true;
                    } else if let (Some((n, f)), Some(alt)) = (self.vorschau.take(), st.nord) {
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
            out.commit = Some((LABEL_DREHEN, n, Some(self.fuss_aus(a))));
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
                    // Zahl + Enter im Haus rastet ein wie das Aufziehen
                    let f = self.fuss_aus(a);
                    out.commit = Some((LABEL_DREHEN, n.rem_euclid(360.0), Some(f)));
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
            self.vorschau = Some((n.rem_euclid(360.0), self.fuss_aus(a)));
        }
    }

    /// Fußpunkt zum Klickpunkt `a` beim Aufziehen: im Haus gleich vor der
    /// Kante eingerastet, gezeigt wie geschrieben (§8 08:50). Die Richtung
    /// zählt weiter ab dem Klickpunkt.
    fn fuss_aus(&self, a: Foot) -> Foot {
        vor_der_kante(a, self.gebaeude)
    }

    /// Pille beim Aufziehen und Drehen: Spitze des Pfeils und Text.
    pub fn label(&self, cam: &Camera, wh: (f64, f64), scale: f64) -> Option<(Vec3, String)> {
        if self.anfang.is_none() && self.zug != Some(Zug::Drehen) {
            return None;
        }
        let (n, f) = self.vorschau?;
        // Hinter dem „N“
        let l = laenge_bei(cam, wh, scale, f) * (1.0 + N_LUFT + N_HOCH + 0.14);
        let [dx, dy] = richtung(n);
        let text = match &self.eingabe {
            Some(i) => i.text(["Nord", ""]),
            None => grad_text(n),
        };
        Some((vec3(f[0] + dx * l, f[1] + dy * l, 0.0), text))
    }

    /// Das Zeichen im Bild der Ansicht (`w` × `h` Pixel) mit Strichbreite
    /// `width` (dip); `None` ohne Pfeil oder wenn ein Teil hinter der Kamera
    /// läge. In `hot` beim Aufziehen, Ziehen und Darüberfahren.
    #[allow(clippy::too_many_arguments)]
    pub fn bild(
        &self,
        st: Stand,
        cam: &Camera,
        (w, h): (f64, f64),
        scale: f32,
        ink: [f32; 4],
        hot: [f32; 4],
        width: f32,
    ) -> Option<Bild> {
        let (n, f) = self.gezeigt(st)?;
        let heiss = self.zug.is_some() || self.anfang.is_some() || self.hover.is_some();
        let l = laenge_bei(cam, (w, h), scale as f64, f);
        let g = gestalt(f, n, l);
        let p = |q: Foot| {
            cam.project(vec3(q[0], q[1], 0.0), w, h)
                .map(|(x, y)| (x as f32, y as f32))
        };
        let pfeil = [g.spitze, g.links, g.kerbe, g.rechts, f].map(p);
        let pfeil = [pfeil[0]?, pfeil[1]?, pfeil[2]?, pfeil[3]?, pfeil[4]?];
        let [t, _, _, _, m] = pfeil;
        // Größe des „N“ wie auf dem Boden an der Spitze, aber aufrecht zum
        // Bildschirm; es steht in Richtung des Pfeils vor der Spitze, mit
        // Luft auch dann, wenn der Pfeil in 3D verkürzt erscheint
        let r = cam.right();
        let quer = p([g.spitze[0] + r.x * l, g.spitze[1] + r.y * l])?;
        let lp = (quer.0 - t.0).hypot(quer.1 - t.1);
        let (hh, hb) = ((N_HOCH * 0.5) as f32 * lp, (N_BREIT * 0.5) as f32 * lp);
        let (ux, uy) = (t.0 - m.0, t.1 - m.1);
        let len = ux.hypot(uy);
        let (ux, uy) = if len > 1e-3 {
            (ux / len, uy / len)
        } else {
            (0.0, -1.0)
        };
        let ab = N_LUFT as f32 * lp + ux.abs() * hb + uy.abs() * hh;
        Some(Bild {
            pfeil,
            n: ((t.0 + ux * ab, t.1 + uy * ab), 2.0 * hh),
            farbe: Rgba::from_f32(if heiss { hot } else { ink }),
            breite: (width * scale).max(1.0),
            ansicht: (w as f32, h as f32),
        })
    }
}

/// Strich im Bild: Anfang, Ende, Breite (Pixel).
type Strich = ((f32, f32), (f32, f32), f32);

/// Das Zeichen im Bild der Ansicht (Pixel, y nach unten), wie es gemalt
/// wird; gleich, solange sich das Bild nicht ändert.
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    /// Spitze, linke Ecke, Kerbe, rechte Ecke, Fußpunkt.
    pfeil: [(f32, f32); 5],
    /// Mitte und Höhe des „N“.
    n: ((f32, f32), f32),
    farbe: Rgba,
    breite: f32,
    ansicht: (f32, f32),
}

/// Die linke Hälfte des Pfeils: weiß, auch über dunklem Grund.
const WEISS: Rgba = Rgba(255, 255, 255, 255);

impl Bild {
    /// Gemalt: das Bild und seine linke obere Ecke in der Ansicht; `None`,
    /// wenn nichts davon in der Ansicht liegt.
    pub fn malen(&self) -> Option<(Canvas, i32, i32)> {
        let [t, li, ke, re, _] = self.pfeil;
        let ((nx, ny), nh) = self.n;
        let rand = self.breite * 2.0 + 2.0;
        let mut lo = (nx - nh, ny - nh);
        let mut hi = (nx + nh, ny + nh);
        for &(x, y) in &self.pfeil {
            (lo, hi) = ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)));
        }
        // Nur der sichtbare Teil
        let x0 = (lo.0 - rand).max(-rand).floor();
        let y0 = (lo.1 - rand).max(-rand).floor();
        let x1 = (hi.0 + rand).min(self.ansicht.0 + rand).ceil();
        let y1 = (hi.1 + rand).min(self.ansicht.1 + rand).ceil();
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let mut c = Canvas::new((x1 - x0) as usize, (y1 - y0) as usize);
        let v = |p: (f32, f32)| (p.0 - x0, p.1 - y0);
        let dreieck = |c: &mut Canvas, a: (f32, f32), b: (f32, f32), col: Rgba| {
            let (a, b, d) = (v(t), v(a), v(b));
            let mut p = Path::new();
            p.move_to(a.0, a.1)
                .line_to(b.0, b.1)
                .line_to(d.0, d.1)
                .close();
            c.fill(&p, col);
        };
        // Links weiß, rechts gefüllt (Jörns Skizze)
        dreieck(&mut c, li, ke, WEISS);
        dreieck(&mut c, ke, re, self.farbe);
        let k = self.breite;
        let mut striche: Vec<Strich> = [(t, li), (li, ke), (ke, re), (re, t), (t, ke)]
            .map(|(a, b)| (v(a), v(b), k))
            .to_vec();
        for (a, b) in n_striche(v((nx, ny)), nh) {
            striche.push((a, b, k * 1.4));
        }
        // Striche und runde Enden je in einem Pfad: alle Striche laufen im
        // selben Sinn um, alle Enden auch, nur beide zusammen höben sich auf
        let (mut p, mut q) = (Path::new(), Path::new());
        for (a, b, k) in striche {
            p.segment(a, b, k);
            let r = k * 0.5;
            for e in [a, b] {
                q.rounded_rect(e.0 - r, e.1 - r, k, k, r);
            }
        }
        c.fill(&p, self.farbe);
        c.fill(&q, self.farbe);
        Some((c, x0 as i32, y0 as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

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
        assert_eq!(tip(None, false), "Nordrichtung festlegen");
        assert_eq!(tip(None, true), "Nordrichtung festlegen");
        assert_eq!(tip(Some(12.0), true), "Sonnenstand an");
        assert_eq!(tip(Some(12.0), false), "Sonnenstand aus");
    }

    /// §8: Fehlt der Fußpunkt oder liegt er im Gebäude, steht der Pfeil
    /// vorne links daneben, das ganze Zeichen 1 m frei (08:45); ohne
    /// Gebäude am Ursprung.
    #[test]
    fn anzeige_neben_dem_gebaeude() {
        let b = Some((vec3(0.0, 0.0, 0.0), vec3(10000.0, 8000.0, 6000.0)));
        let l = 2000.0;
        let a = ersatz_abstand(l);
        assert_eq!(a, 1000.0 + (1.0 + N_LUFT + N_HOCH + N_BREIT) * l);
        assert_eq!(anzeige_fuss(None, None, l), [0.0, 0.0]);
        assert_eq!(anzeige_fuss(Some([5.0, 6.0]), None, l), [5.0, 6.0]);
        assert_eq!(anzeige_fuss(None, b, l), [-a, -a]);
        assert_eq!(anzeige_fuss(Some([0.0, 0.0]), b, l), [-a, -a]);
        assert_eq!(anzeige_fuss(Some([5000.0, 4000.0]), b, l), [-a, -a]);
        assert_eq!(anzeige_fuss(Some([12000.0, 0.0]), b, l), [12000.0, 0.0]);
        // Jeder Punkt des Zeichens liegt in n_reichweite um den Fußpunkt
        for nord in [0.0, 40.0, 90.0, 225.0, 333.0] {
            let g = gestalt([0.0, 0.0], nord, l);
            for p in [g.spitze, g.links, g.rechts, g.kerbe] {
                assert!(p[0].hypot(p[1]) <= n_reichweite(l));
            }
        }
    }

    /// Der Pfeil zeigt in die Nordrichtung: bei Nord 90° liegt die Spitze
    /// rechts vom Fußpunkt, die linke Ecke nördlich der Achse, die Kerbe
    /// auf der Achse vor der Grundlinie.
    #[test]
    fn gestalt_zeigt_nach_norden() {
        let f = [100.0, 200.0];
        let g = gestalt(f, 90.0, 1000.0);
        let nah = |a: Foot, b: Foot| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert!(nah(g.spitze, [1100.0, 200.0]));
        assert!(nah(g.links, [100.0, 200.0 + BREIT * 1000.0]));
        assert!(nah(g.rechts, [100.0, 200.0 - BREIT * 1000.0]));
        assert!(nah(g.kerbe, [100.0 + KERBE * 1000.0, 200.0]));
    }

    fn bild_bei(nord: f64, cam: &Camera) -> Bild {
        let st = Stand {
            nord: Some(nord),
            fuss: [0.0, 0.0],
            gesetzt: None,
        };
        Nordpfeil::default()
            .bild(
                st,
                cam,
                (1000.0, 800.0),
                1.0,
                [0.0, 0.0, 0.0, 1.0],
                [1.0; 4],
                1.5,
            )
            .unwrap()
    }

    /// Befund B (§8 07:55, 08:20): Das „N“ steht im Grundriss und in 3D
    /// aufrecht zum Bildschirm, im Grundriss bei jeder Nordrichtung gleich
    /// groß, und frei vor der Spitze.
    #[test]
    fn n_bleibt_aufrecht() {
        let plan = Camera::parallel(vec3(0.0, 0.0, 0.0), FRAC_PI_2, -FRAC_PI_2, 20000.0);
        let raum = Camera::looking_at(vec3(-15000.0, -22000.0, 15000.0), vec3(0.0, 0.0, 0.0), 45.0);
        for (cam, name) in [(&plan, "Grundriss"), (&raum, "3D")] {
            for nord in [0.0, 90.0, 180.0, 270.0, 33.0] {
                let b = bild_bei(nord, cam);
                let ((nx, ny), hoch) = b.n;
                let [a, _, c] = n_striche((nx, ny), hoch);
                // Senkrechte Striche: oben kleineres y
                assert_eq!(a.0 .0, a.1 .0, "{name} {nord}");
                assert!(a.1 .1 < a.0 .1 && c.1 .1 < c.0 .1, "{name} {nord}");
                if cam.ortho.is_some() {
                    assert!((hoch - (N_HOCH * LAENGE_PX) as f32).abs() < 1e-3, "{nord}");
                }
                // Kein Punkt des N näher am Fußpunkt als die Spitze
                let [t, _, _, _, m] = b.pfeil;
                let spitze = (t.0 - m.0).hypot(t.1 - m.1);
                for (p, q) in n_striche((nx, ny), hoch) {
                    for r in [p, q] {
                        assert!(
                            (r.0 - m.0).hypot(r.1 - m.1) > spitze * 1.03,
                            "{name} {nord}"
                        );
                    }
                }
            }
        }
    }

    /// Jörns Skizze (§8 08:42): links der Teilung weiß, rechts gefüllt;
    /// dreimal so groß wie zuerst (192 dip im Grundriss, 9 m in 3D).
    #[test]
    fn links_weiss_rechts_gefuellt() {
        assert_eq!((LAENGE_PX, LAENGE_3D), (192.0, 9000.0));
        let plan = Camera::parallel(vec3(0.0, 0.0, 0.0), FRAC_PI_2, -FRAC_PI_2, 20000.0);
        let b = bild_bei(0.0, &plan);
        let (c, x0, y0) = b.malen().unwrap();
        let px = c.to_rgba8();
        let farbe = |x: f32, y: f32| {
            let (i, j) = ((x - x0 as f32) as usize, (y - y0 as f32) as usize);
            let k = (j * c.width + i) * 4;
            [px[k], px[k + 1], px[k + 2], px[k + 3]]
        };
        let [t, _, _, _, m] = b.pfeil;
        let l = m.1 - t.1;
        assert!((l - 192.0).abs() < 0.01);
        // Nord 0: Spitze oben, links ist kleineres x
        let y = m.1 - 0.4 * l;
        assert_eq!(farbe(m.0 - 0.05 * l, y), [255, 255, 255, 255]);
        assert_eq!(farbe(m.0 + 0.05 * l, y), [0, 0, 0, 255]);
        // Hinter der Kerbe, zwischen den Ecken: frei
        assert_eq!(farbe(m.0, m.1 - 0.05 * l)[3], 0);
    }

    /// Review 3bu-1: Nah am Pfeil in 3D bleibt er auf dem Bildschirm höchstens
    /// [`KAPPE_PX`] lang; das Bild bleibt klein, Greifen und Pille folgen.
    #[test]
    fn nah_in_3d_gedeckelt() {
        let (w, h) = (2560.0, 1400.0);
        let nah = Camera::looking_at(vec3(-1500.0, -2500.0, 1800.0), vec3(0.0, 0.0, 0.0), 45.0);
        let fern = Camera::looking_at(vec3(-25000.0, -40000.0, 25000.0), vec3(0.0, 0.0, 0.0), 45.0);
        assert_eq!(laenge_bei(&fern, (w, h), 2.0, [0.0, 0.0]), LAENGE_3D);
        let l = laenge_bei(&nah, (w, h), 2.0, [0.0, 0.0]);
        assert!(l < LAENGE_3D * 0.5, "{l}");
        let st = Stand {
            nord: Some(30.0),
            fuss: [0.0, 0.0],
            gesetzt: None,
        };
        let b = Nordpfeil::default()
            .bild(st, &nah, (w, h), 2.0, [0.0, 0.0, 0.0, 1.0], [1.0; 4], 1.5)
            .unwrap();
        let (c, _, _) = b.malen().unwrap();
        let groesste = (KAPPE_PX * 2.0 * 1.6) as usize;
        assert!(
            c.width < groesste && c.height < groesste,
            "{} × {}",
            c.width,
            c.height
        );
    }

    /// Bildzeit nah am Pfeil (Review 3bu-1), nur zum Messen:
    /// `cargo test --release -p skizzeo bildzeit -- --ignored --nocapture`
    #[test]
    #[ignore = "misst nur"]
    fn bildzeit_nah_am_pfeil() {
        let (w, h) = (2560.0, 1400.0);
        let st = Stand {
            nord: Some(30.0),
            fuss: [0.0, 0.0],
            gesetzt: None,
        };
        for (name, eye) in [
            ("wie im Test", vec3(-15000.0, -22000.0, 15000.0)),
            ("nah", vec3(-1500.0, -2500.0, 1800.0)),
        ] {
            let mut zeit = std::time::Duration::ZERO;
            let mut groesse = (0, 0);
            for i in 0..40 {
                // Jedes Bild mit etwas anderer Kamera, wie beim Drehen
                let e = eye + vec3(i as f64 * 7.0, 0.0, 0.0);
                let cam = Camera::looking_at(e, vec3(0.0, 0.0, 0.0), 45.0);
                let t = std::time::Instant::now();
                let b = Nordpfeil::default()
                    .bild(st, &cam, (w, h), 2.0, [0.0, 0.0, 0.0, 1.0], [1.0; 4], 1.5)
                    .unwrap();
                let (c, _, _) = b.malen().unwrap();
                let px = c.to_premul_rgba8();
                zeit += t.elapsed();
                groesse = (c.width, c.height);
                assert!(!px.is_empty());
            }
            println!(
                "{name}: {} × {} Pixel, {:.2} ms je Bild",
                groesse.0,
                groesse.1,
                zeit.as_secs_f64() * 1000.0 / 40.0
            );
        }
    }

    /// Greifen: die Spitze dreht, der ganze Pfeilkörper verschiebt.
    #[test]
    fn pfeilkoerper_greift() {
        let (a, b, c) = ((0.0, 0.0), (10.0, 0.0), (0.0, 10.0));
        assert!(im_dreieck((2.0, 2.0), a, b, c));
        assert!(im_dreieck((2.0, 2.0), a, c, b));
        assert!(!im_dreieck((8.0, 8.0), a, b, c));
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
        // Nach außen auf 10 mm gerundet, nur auf der eingerasteten Achse
        let krumm = Some((vec3(-2617.5, 0.0, 0.0), vec3(10003.0, 8000.0, 0.0)));
        assert_eq!(vor_der_kante([-2000.0, 4000.0], krumm), [-3120.0, 4000.0]);
        assert_eq!(vor_der_kante([9900.0, 4000.5], krumm), [10510.0, 4000.5]);
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
            fuss: anzeige_fuss(m.north_foot(), s.bounds(), laenge(&grundriss(), H, 1.0)),
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
        let (text, pille) = n.label(&cam, (W, H), 1.0).map(|(p, t)| (t, p)).unwrap();
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
        let a = ersatz_abstand(laenge(&cam, H, 1.0));
        assert_eq!(st.fuss, [lo.x - a, lo.y - a]);
        let bild = n.bild(st, &cam, (W, H), 1.0, [1.0; 4], [1.0; 4], 1.5);
        assert!(bild.and_then(|b| b.malen()).is_some(), "Zeichen sichtbar");
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
        assert!(n.label(&cam, (W, H), 1.0).unwrap().1.starts_with("Nord"));
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
        let a = ersatz_abstand(laenge(&grundriss(), H, 1.0));
        assert_eq!(stand(&s).fuss, [lo.x - a, lo.y - a]);
        let neu = szo::write(s.model());
        assert!(neu.contains("[location] north=15\n"), "{neu}");
        assert!(s.undo());
        assert_eq!(szo::write(s.model()), alt);
    }

    /// S4 (§8 09:25): Ein Klick auf den Pfeil ohne Ziehen meldet `klick`
    /// und schreibt keinen Schritt, auch mit kleinem Zittern unter der
    /// Zug-Schwelle; erst darüber wird verschoben.
    #[test]
    fn klick_ohne_ziehen_ohne_schritt() {
        let cam = grundriss();
        let mut s = Scene::with_model(Model::with_seed(1));
        assert!(s.nordpfeil_setzen(LABEL_DREHEN, 0.0, Some([0.0, 0.0])));
        let vorher = szo::write(s.model());
        let mut n = Nordpfeil::default();
        let p = [0.0, 1500.0];
        let (x, y) = bild(&cam, p);
        assert!(ereignis(&mut n, &mut s, &cam, unten(&cam, p)).consumed);
        let zittern = Event::MouseMove {
            x: x + 2.5,
            y: y - 1.0,
            mods: M,
        };
        ereignis(&mut n, &mut s, &cam, zittern);
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, p));
        assert!(out.klick && out.commit.is_none() && out.consumed);
        assert_eq!(szo::write(s.model()), vorher, "kein Schritt");
        assert!(!n.zieht());
        // Weiter als die Schwelle: verschoben, kein Klick
        ereignis(&mut n, &mut s, &cam, unten(&cam, p));
        ereignis(&mut n, &mut s, &cam, zu(&cam, [-2000.0, 1500.0], false));
        let out = ereignis(&mut n, &mut s, &cam, oben(&cam, [-2000.0, 1500.0]));
        assert!(!out.klick);
        assert_eq!(out.commit.map(|c| c.0), Some(LABEL_SCHIEBEN));
        assert_eq!(s.model().north_foot(), Some([-2000.0, 0.0]));
    }
}

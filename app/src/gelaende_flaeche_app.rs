//! Geländeflächen in der App (G3d, Bedienbarkeit 43.1): das Werkzeug
//! „Fläche zeichnen“ in der Leiste „Gelände“, die Auswahl einer Fläche mit
//! einem Klick im Grundriss und ihre Eigenschaften im Paneel (Höhe,
//! Böschung, befestigt, löschen; dazu Andecken und Faktor des Grundstücks).
//!
//! Das Werkzeug setzt Ecken mit Klicks im Grundriss (es schaltet dorthin); ein Klick auf die
//! erste Ecke, ein zweiter Klick auf die letzte oder Enter schließt die
//! Fläche, Rücktaste oder Strg+Z nimmt die letzte Ecke zurück, Esc verwirft. Die neue
//! Fläche liegt eben auf der Höhe des Geländes in ihrer Mitte, mit der
//! Böschung nach Vorgabe; Höhe und Böschung stellt man danach im Paneel.
//! Beim Zeichnen rastet der Punkt an Ecken und Kanten von Haus, Flächen
//! und Grenzen ([`fang`], H-189a).
//!
//! Dasselbe Werkzeug zeichnet als „Grenze zeichnen“ ein Grundstück (I2):
//! Der geschlossene Umriss wird ein Flurstück im Gelände
//! ([`sk_model::Model::add_parcel`]), auch ohne Lageplan.

mod fang;

use crate::camera::Camera;
use crate::ui::{Field, FieldRow, GelaendeFeld, GelaendeKnopf, Id, Props, Section, ViewKind};
use crate::App;
use sk_math::vec3;
use sk_model::{
    HouseStrip, Model, PatchHeight, SiteSoil, TerrainPatch, PATCH_SLOPE, PATCH_SLOPE_RANGE,
    PATCH_XY_MAX, STRIP_FALL_RANGE, STRIP_WIDTH, STRIP_WIDTH_RANGE,
};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

use fang::{Fang, Ziele};

/// Raster der Ecken (mm).
const STEP: f64 = 10.0;
/// So nah (Bildpunkte) an der ersten Ecke schließt ein Klick die Fläche.
const SCHLIESSEN_PX: f64 = 10.0;
/// Schritte im Rückgängig.
pub const LABEL_NEU: &str = "Geländefläche";
pub const LABEL_AENDERN: &str = "Geländefläche ändern";
pub const LABEL_LOESCHEN: &str = "Geländefläche löschen";
pub const LABEL_HAUS: &str = "Gelände ans Haus";
/// „Gelände ans Haus“ ohne Haus mit Gründung bzw. um einen Umriss, um den
/// sich kein Streifen legen lässt.
pub const KEIN_HAUS: &str = "Gelände ans Haus braucht ein Haus mit Außenwänden und Gründung.";
pub const NICHT_UM: &str = "Um dieses Haus lässt sich kein Streifen legen: Eine Einbuchtung ist \
    schmaler als 0,40 m.";
/// Der neue Streifen ist schmaler als 1,00 m, weil eine Einbuchtung nicht
/// mehr zulässt (Bedienbarkeit zu #226).
pub const SCHMAL: &str = "Der Streifen ist {} m breit: Für 1,00 m ist eine Einbuchtung zu schmal.";
/// Beim Schritt: Der Streifen folgt dem Haus schmaler, oder gar nicht.
pub const SCHMALER: &str = "Der Streifen am Haus ist jetzt {} m breit: Breiter passt er nicht in \
    eine Einbuchtung.";
pub const STEHT: &str =
    "Der Streifen am Haus folgt dem Haus nicht, weil eine Einbuchtung schmaler \
    als 0,40 m ist; er bleibt, wie er war.";
/// Das Haus hat schon einen Streifen: er ist jetzt gewählt.
pub const SCHON: &str = "Das Haus hat schon einen Streifen; er ist gewählt.";
/// „Geländebau“ verstand niemand (Bedienbarkeit, später zu #189).
pub const LABEL_BODEN: &str = "Erdarbeiten am Grundstück";
/// Was die Statuszeile sagt, wenn sich die gezeichnete Fläche nicht bauen
/// lässt (Kanten kreuzen sich, keine Fläche).
pub const KREUZT: &str = "Die Kanten der Fläche kreuzen sich; bitte anders schließen.";
/// Enter oder Klick auf die erste Ecke bei zwei Ecken (Bedienbarkeit,
/// später zu #189: tat still nichts oder setzte die Ecke doppelt).
pub const ZU_WENIG: &str = "Eine Fläche braucht mindestens drei Ecken.";
/// Statuszeile, solange das Werkzeug an ist (Bedienbarkeit, später zu
/// #189: die Zeile war leer, Esc nirgends genannt).
pub const ZEICHNEN: &str = "Fläche zeichnen: Ecken im Grundriss klicken (mit Strg ohne Fang); die \
    erste Ecke oder Enter schließt, Rücktaste nimmt die letzte, Esc bricht ab.";
/// Alle Ecken auf einer Linie (H-189d).
pub const OHNE_INHALT: &str = "Die Ecken liegen auf einer Linie; die Fläche hat keinen Inhalt.";
/// Dieselben Sätze für „Grenze zeichnen“ (I2).
pub const GRENZE: &str = "Grenze zeichnen: Ecken im Grundriss klicken (mit Strg ohne Fang); die \
    erste Ecke oder Enter schließt, Rücktaste nimmt die letzte, Esc bricht ab.";
pub const GRENZE_ZU_WENIG: &str = "Eine Grenze braucht mindestens drei Ecken.";
pub const GRENZE_KREUZT: &str = "Die Kanten der Grenze kreuzen sich; bitte anders schließen.";
pub const GRENZE_OHNE_INHALT: &str =
    "Die Ecken liegen auf einer Linie; das Grundstück hat keine Fläche.";
pub const GRENZE_ZU_WEIT: &str =
    "Die Grenze liegt zu weit außerhalb; bitte näher am Haus zeichnen.";
/// Schritt im Rückgängig.
pub const LABEL_GRENZE: &str = "Grenze gezeichnet";
/// Die Fläche ließ sich auf der Geländehöhe in ihrer Mitte nicht anlegen
/// (H-189d).
pub const ZU_HOCH: &str =
    "Das Gelände liegt dort mehr als 100 m über oder unter ±0,00; die Fläche entsteht nicht.";
/// Ecken weiter als 100 km vom Ursprung (H-189d).
pub const ZU_WEIT: &str = "Die Fläche liegt zu weit außerhalb; bitte näher am Haus zeichnen.";
/// Hinweise im Paneel; befestigt mit dem Satz aus den Mengen
/// ([`sk_model::qto_earth::BEFESTIGT`], Bedienbarkeit 43.1).
const HINWEIS: &str = "Höhe über ±0,00. Böschung 0: senkrechter Rand.";
const HINWEIS_BEFESTIGT: &str = "Höhe über ±0,00. Böschung 0: senkrechter Rand. Aufbau der \
    befestigten Fläche wird noch nicht gerechnet, Auftrag bis OK Fläche.";
const HINWEIS_HAUS: &str = "Höhe über ±0,00 am Haus, Gefälle vom Haus weg. Der Streifen folgt \
    dem Haus. Böschung 0: senkrechter Rand.";
const HINWEIS_BODEN: &str = "Gilt für alle Geländeflächen. Eigenboden fest zu eingebaut unter 1: \
    Boden setzt sich beim Verdichten; ändert nur Eigenboden und Lieferung.";

/// Ergebnis eines Ereignisses.
#[derive(Default)]
pub struct Ausgang {
    pub redraw: bool,
    pub consumed: bool,
    /// Geschlossener Umriss (mm).
    pub commit: Option<Vec<[f64; 2]>>,
    /// Der Umriss lässt sich nicht bauen: Satz für die Statuszeile.
    pub fehler: Option<&'static str>,
}

/// Werkzeug „Fläche zeichnen“, mit `grenze` „Grenze zeichnen“.
#[derive(Clone, Debug, Default)]
pub struct Werkzeug {
    pub aktiv: bool,
    /// Zeichnet eine Grenze statt einer Fläche (I2).
    pub grenze: bool,
    /// Gesetzte Ecken (mm).
    ecken: Vec<[f64; 2]>,
    /// Bodenpunkt unter der Maus (Gummiband).
    maus: Option<[f64; 2]>,
    /// Fangziele und der Modellstand, zu dem sie gehören.
    ziele: Ziele,
    stand: Option<u64>,
    /// Woran der Punkt unter der Maus gerastet ist.
    fang: Option<([f64; 2], Fang)>,
}

impl Werkzeug {
    /// Ein- oder ausschalten; aus verwirft die gesetzten Ecken.
    pub fn set_aktiv(&mut self, on: bool) {
        self.aktiv = on;
        self.ecken.clear();
        self.maus = None;
        self.fang = None;
        if !on {
            self.ziele = Ziele::default();
            self.stand = None;
        }
    }

    /// Als „Fläche zeichnen“ oder „Grenze zeichnen“ (`grenze`) einschalten;
    /// läuft es schon so, aus. Ein Wechsel verwirft die gesetzten Ecken.
    pub fn umschalten(&mut self, grenze: bool) {
        let an = !(self.aktiv && self.grenze == grenze);
        self.set_aktiv(an);
        self.grenze = grenze;
    }

    /// Satz der Statuszeile, solange es an ist.
    pub fn zeile(&self) -> &'static str {
        if self.grenze {
            GRENZE
        } else {
            ZEICHNEN
        }
    }

    /// Fangziele zum Modellstand `rev`; `f` rechnet sie nur, wenn sich der
    /// Stand geändert hat.
    pub fn ziele_fuer(&mut self, rev: u64, f: impl FnOnce() -> Ziele) {
        if self.aktiv && self.stand != Some(rev) {
            self.ziele = f();
            self.stand = Some(rev);
        }
    }

    /// Bodenpunkt unter dem Bildpunkt: an einem Fangziel bis [`fang::FANG_PX`]
    /// Bildpunkte, sonst auf [`STEP`] gerundet. Mit gehaltener Strg-Taste
    /// ohne Fang, nur im Raster (Bedienbarkeit, später zu #198: sonst half
    /// nur Zoomen).
    #[allow(clippy::too_many_arguments)]
    fn boden(
        &mut self,
        cam: &Camera,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        scale: f64,
        mods: Modifiers,
    ) -> Option<[f64; 2]> {
        let Some(g) = cam.ground_point(x, y, w, h) else {
            self.fang = None;
            return None;
        };
        let tol = cam
            .ground_point(x + fang::FANG_PX * scale, y, w, h)
            .map_or(0.0, |n| (n.x - g.x).hypot(n.y - g.y));
        self.fang = (!mods.ctrl)
            .then(|| self.ziele.fangen([g.x, g.y], tol))
            .flatten();
        let r = |v: f64| (v / STEP).round() * STEP + 0.0;
        Some(self.fang.map_or([r(g.x), r(g.y)], |f| f.0))
    }

    pub fn ecken(&self) -> &[[f64; 2]] {
        &self.ecken
    }

    /// Ecken gesetzt: Rückgängig, Wiederherstellen und Entf warten.
    pub fn zeichnet(&self) -> bool {
        self.aktiv && !self.ecken.is_empty()
    }

    /// Der Umriss ließ sich nicht anlegen (Gelände zu hoch): die Ecken
    /// bleiben stehen wie bei jedem anderen Fehler (Review #189 H-189d).
    pub fn ecken_zurueck(&mut self, ring: Vec<[f64; 2]>) {
        if self.aktiv {
            self.ecken = ring;
        }
    }

    /// Esc: erst die gesetzten Ecken, dann das Werkzeug. `true`, wenn
    /// etwas geschlossen wurde.
    pub fn escape(&mut self) -> bool {
        if !self.ecken.is_empty() {
            self.ecken.clear();
            return true;
        }
        if self.aktiv {
            self.set_aktiv(false);
            return true;
        }
        false
    }

    /// Rücktaste und Strg+Z nehmen die letzte Ecke zurück, Enter schließt
    /// ab drei Ecken; Strg+Y, Strg+Umschalt+Z und Entf tun beim Zeichnen
    /// nichts, statt aufs Modell zu wirken (Review #189 B1). `true`, wenn
    /// verbraucht.
    pub fn key(&mut self, key: Key, mods: Modifiers, out: &mut Ausgang) -> bool {
        if !self.zeichnet() {
            return false;
        }
        match key {
            Key::Backspace => {}
            Key::Char('Z') if mods.ctrl && !mods.shift => {}
            Key::Enter => {
                self.schliessen(out);
                return true;
            }
            Key::Char('Y' | 'Z') if mods.ctrl => return true,
            Key::Delete => return true,
            _ => return false,
        }
        self.ecken.pop();
        out.redraw = true;
        true
    }

    /// Umriss fertig, wenn er drei Ecken hat und sich bauen lässt; sonst
    /// bleibt er stehen und `fehler` sagt warum.
    fn schliessen(&mut self, out: &mut Ausgang) {
        out.redraw = true;
        let g = self.grenze;
        if self.ecken.len() < 3 {
            out.fehler = Some(if g { GRENZE_ZU_WENIG } else { ZU_WENIG });
            return;
        }
        // eine Grenze muss sich bauen lassen wie eine Fläche
        let p = TerrainPatch::flat(self.ecken.clone(), 0.0);
        if !p.valid() {
            let weit = self.ecken.iter().flatten().any(|v| v.abs() > PATCH_XY_MAX);
            out.fehler = Some(match (auf_einer_linie(&self.ecken), weit, g) {
                (true, _, false) => OHNE_INHALT,
                (true, _, true) => GRENZE_OHNE_INHALT,
                (_, true, false) => ZU_WEIT,
                (_, true, true) => GRENZE_ZU_WEIT,
                (_, _, false) => KREUZT,
                (_, _, true) => GRENZE_KREUZT,
            });
            return;
        }
        out.commit = Some(std::mem::take(&mut self.ecken));
        self.maus = None;
        self.fang = None;
    }

    /// Ereignis der Ansicht (Koordinaten ohne Titelleiste).
    pub fn handle(&mut self, e: &Event, cam: &Camera, w: f64, h: f64, scale: f64) -> Ausgang {
        let mut out = Ausgang::default();
        if !self.aktiv {
            return out;
        }
        match *e {
            Event::MouseMove { x, y, mods } => {
                let vor = self.fang;
                let p = self.boden(cam, x, y, w, h, scale, mods);
                if p != self.maus || vor != self.fang {
                    out.redraw = !self.ecken.is_empty() || vor != self.fang;
                    self.maus = p;
                }
                out.consumed = true;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                out.consumed = true;
                let Some(p) = self.boden(cam, x, y, w, h, scale, mods) else {
                    return out;
                };
                // Klick auf die erste Ecke (am Bildschirm) oder noch einmal
                // auf die letzte schließt
                let am_anfang = self.ecken.first().is_some_and(|a| {
                    cam.project(vec3(a[0], a[1], 0.0), w, h)
                        .is_some_and(|q| (q.0 - x).hypot(q.1 - y) < SCHLIESSEN_PX * scale)
                });
                if self.ecken.len() >= 3 && (am_anfang || self.ecken.last() == Some(&p)) {
                    self.schliessen(&mut out);
                } else if am_anfang {
                    // nicht die erste Ecke noch einmal setzen
                    if self.ecken.len() == 2 {
                        out.fehler = Some(if self.grenze {
                            GRENZE_ZU_WENIG
                        } else {
                            ZU_WENIG
                        });
                    }
                } else if self.ecken.last() != Some(&p) {
                    self.ecken.push(p);
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => out.consumed = true,
            Event::MouseLeave => {
                // beide leeren, auch wenn ein Fang die Antwort schon gibt
                // (Review #198 B1)
                let fang = self.fang.take().is_some();
                let maus = self.maus.take().is_some();
                out.redraw = fang || (maus && !self.ecken.is_empty());
            }
            _ => {}
        }
        out
    }

    /// Gesetzte Kanten, Gummiband zur Maus und zurück zur ersten Ecke, in
    /// Höhe `z` (mm).
    pub fn linien(&self, z: f64, scale: f32, theme: &Theme) -> Vec<Helper> {
        let col = &theme.interact;
        let mut pts: Vec<[f64; 2]> = self.ecken.clone();
        if let (Some(m), false) = (self.maus, self.ecken.is_empty()) {
            pts.push(m);
        }
        let mut out = Vec::new();
        let p = |q: [f64; 2]| [q[0] as f32, q[1] as f32, z as f32 + 1.0];
        let linie = |a, b, color, dash: f32| Helper {
            a: p(a),
            b: p(b),
            color,
            width: 2.0 * scale,
            dash: dash * scale,
            pattern: sk_render::SOLID,
            occlude: false,
            round: false,
        };
        for s in pts.windows(2) {
            out.push(linie(s[0], s[1], col.draw, 0.0));
        }
        if pts.len() >= 3 {
            out.push(linie(pts[pts.len() - 1], pts[0], col.guide, 6.0));
        }
        if let Some(&a) = self.ecken.first() {
            let mut m = linie(a, a, col.track, 0.0);
            m.width = 8.0 * scale;
            out.push(m);
        }
        // Fang: ein Quadrat an der Ecke, ein runder Punkt an der Kante;
        // nur an der Größe war das kaum zu unterscheiden (Bedienbarkeit,
        // später zu #198)
        if let Some((q, f)) = self.fang {
            let mut m = linie(q, q, col.guide, 0.0);
            m.width = if f == Fang::Ecke { 12.0 } else { 10.0 } * scale;
            m.round = f == Fang::Kante;
            out.push(m);
        }
        out
    }
}

/// Liegt `p` in dem Polygon `ring`?
fn innen(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut drin = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                drin = !drin;
            }
        }
    }
    drin
}

/// Nummer der Fläche an `p` (mm); bei Überlappung die zuletzt gesetzte,
/// wie sie auch gilt. Ohne Höhenaufmaß ruhen die Flächen: nicht zu sehen,
/// nicht gerechnet, also auch nicht wählbar (Review #196 H-196a).
pub fn flaeche_bei(m: &Model, p: [f64; 2]) -> Option<usize> {
    if ruhen(m) {
        return None;
    }
    m.site_patches()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, f)| f.contains(p))
        .map(|(i, _)| i)
}

/// Kann ein Klick das Gelände wählen? Nur in 3D, wo es gezeichnet ist: im
/// Grundriss ist es unsichtbar, ein Klick neben das Haus wählt dort ab
/// (Review #212 B1). Das Gelände gehört zur Grundausstattung, also auch
/// ohne das Geländemodul, ob als ebene Platte oder mit Punkten (I1);
/// ausgeblendet ist es nicht zu treffen.
fn platte_waehlbar(view: ViewKind, ausgeblendet: bool) -> bool {
    view == ViewKind::Persp && !ausgeblendet
}

/// Trifft der Sehstrahl eine der Platten? `strahl(z)` ist sein
/// Schnittpunkt mit der Ebene in Höhe `z` (OK Platte).
fn platte_getroffen(
    platten: &[sk_model::TerrainPlate],
    strahl: impl Fn(f64) -> Option<sk_math::Vec3>,
) -> bool {
    platten
        .iter()
        .any(|p| strahl(p.top).is_some_and(|q| platte_bei(p, [q.x, q.y])))
}

/// Liegt `p` (mm, Draufsicht) auf der Platte `t`, in einem ihrer Trapeze?
fn platte_bei(t: &sk_model::TerrainPlate, p: [f64; 2]) -> bool {
    t.pieces.iter().any(|q| innen(&q.map(|e| [e.x, e.y]), p))
}

/// Ohne Höhenaufmaß ruhen die Flächen.
fn ruhen(m: &Model) -> bool {
    m.terrain().is_none_or(|t| t.points.is_empty())
}

/// Die gewählte Fläche `f`, solange es sie gibt und sie nicht ruht; nach
/// Strg+Z des Einlesens blieb sie sonst gewählt (Review #196 H-196g).
fn gewaehlte(m: &Model, f: Option<usize>) -> Option<usize> {
    f.filter(|&i| !ruhen(m) && i < m.site_patches().len())
}

/// Liegen alle Ecken (mm) höchstens 1 mm neben der Geraden durch die erste
/// und die von ihr fernste Ecke?
fn auf_einer_linie(ring: &[[f64; 2]]) -> bool {
    let a = ring[0];
    let d = |p: &[f64; 2]| [p[0] - a[0], p[1] - a[1]];
    let fern = ring
        .iter()
        .map(d)
        .max_by(|u, v| u[0].hypot(u[1]).total_cmp(&v[0].hypot(v[1])));
    let Some(f) = fern.filter(|f| f[0].hypot(f[1]) > 1.0) else {
        return true;
    };
    let l = f[0].hypot(f[1]);
    ring.iter()
        .map(d)
        .all(|u| (u[0] * f[1] - u[1] * f[0]).abs() <= l)
}

/// Punkt für die Höhe einer neuen Fläche: ihr Schwerpunkt; liegt er bei
/// einem konkaven Umriss draußen, die Mitte des breitesten Stücks der
/// Waagerechten durch ihn, das innen liegt (H-189e).
fn mitte(ring: &[[f64; 2]]) -> [f64; 2] {
    let n = ring.len();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (p, q) = (ring[i], ring[(i + 1) % n]);
        let k = p[0] * q[1] - q[0] * p[1];
        a += k;
        cx += (p[0] + q[0]) * k;
        cy += (p[1] + q[1]) * k;
    }
    if a.abs() < 1e-9 {
        let m = n.max(1) as f64;
        return ring
            .iter()
            .fold([0.0, 0.0], |s, p| [s[0] + p[0] / m, s[1] + p[1] / m]);
    }
    let c = [cx / (3.0 * a), cy / (3.0 * a)];
    if innen(ring, c) {
        return c;
    }
    // Schnitte der Waagerechten mit den Kanten, paarweise innen
    let mut xs: Vec<f64> = (0..n)
        .filter_map(|i| {
            let (p, q) = (ring[i], ring[(i + 1) % n]);
            ((p[1] > c[1]) != (q[1] > c[1]))
                .then(|| p[0] + (c[1] - p[1]) / (q[1] - p[1]) * (q[0] - p[0]))
        })
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.chunks_exact(2)
        .max_by(|u, v| (u[1] - u[0]).total_cmp(&(v[1] - v[0])))
        .map_or(c, |u| [(u[0] + u[1]) / 2.0, c[1]])
}

fn de(v: f64, stellen: usize) -> String {
    format!("{v:.stellen$}").replace('.', ",")
}

fn feld(f: GelaendeFeld, label: &'static str, value: f64, r: (f64, f64)) -> FieldRow {
    FieldRow {
        field: Field::Gelaende(f),
        label,
        value,
        min: r.0,
        max: r.1,
        zero: false,
        einheit: None,
    }
}

/// Höhe einer ebenen Fläche über ±0,00 (mm), sonst `None` (geneigt oder in
/// m ü. NHN eingegeben: nur in der Datei).
fn eben(p: &TerrainPatch) -> Option<f64> {
    match p.height {
        PatchHeight::Flat(z) if !p.abs => Some(z),
        _ => None,
    }
}

/// Neigung einer Böschung 1:`n` in Grad (0 senkrecht: 90°).
fn grad(n: f64) -> f64 {
    (1.0 / n).atan().to_degrees()
}

/// Böschung ringsum (n von 1:n, 0 senkrecht), `None`, wenn sie je Kante
/// verschieden ist.
fn boeschung(p: &TerrainPatch) -> Option<f64> {
    match p.slopes.as_slice() {
        [] => Some(PATCH_SLOPE),
        [n] => Some(*n),
        v => v.iter().all(|&n| n == v[0]).then_some(v[0]),
    }
}

/// Paneel „Eigenschaften“ der Fläche `i`.
pub(crate) fn props(m: &Model, i: usize) -> Option<Props> {
    let p = m.site_patches().get(i)?;
    let streifen = p.house.as_ref();
    let mut values = vec![
        (
            "Kategorie",
            match streifen {
                Some(_) => LABEL_HAUS.to_string(),
                None => "Geländefläche".to_string(),
            },
        ),
        ("Fläche", format!("{} m²", de(p.area() / 1e6, 2))),
    ];
    match streifen {
        Some(s) => {
            let name = m.buildings().iter().find(|(_, b)| b.guid == s.building);
            values.extend(name.map(|(_, b)| ("Haus", b.name.clone())));
        }
        None => values.push(("Ecken", p.outline.len().to_string())),
    }
    let mut fields = Vec::new();
    // mit Aufmaß daneben die Höhe in m ü. NHN (Bedienbarkeit Gelände); am
    // Streifen die Höhe am Haus
    let (hoehe, nhn) = match streifen {
        Some(_) => ("Höhe am Haus", "am Haus ü. NHN"),
        None => ("Höhe", "Höhe ü. NHN"),
    };
    if let Some((z, h0)) = eben(p).zip(m.terrain_nhn()) {
        values.push((nhn, format!("{} m", de(h0 + z / 1000.0, 2))));
    }
    match eben(p) {
        Some(z) => fields.push(feld(
            GelaendeFeld::Hoehe,
            hoehe,
            z,
            (-sk_model::PATCH_Z_MAX, sk_model::PATCH_Z_MAX),
        )),
        None => values.push(("Höhe", "geneigt, aus der Datei".to_string())),
    }
    if let Some(s) = streifen {
        fields.push(feld(
            GelaendeFeld::Breite,
            "Breite",
            s.width,
            STRIP_WIDTH_RANGE,
        ));
        fields.push(FieldRow {
            einheit: Some("%"),
            ..feld(GelaendeFeld::Gefaelle, "Gefälle", s.fall, STRIP_FALL_RANGE)
        });
    }
    match boeschung(p) {
        Some(n) => fields.push(FieldRow {
            field: Field::Gelaende(GelaendeFeld::Boeschung),
            label: "Böschung 1:",
            value: n,
            min: PATCH_SLOPE_RANGE.0,
            max: PATCH_SLOPE_RANGE.1,
            zero: true,
            einheit: Some(""),
        }),
        None => values.push(("Böschung", "je Kante, aus der Datei".to_string())),
    }
    // die andere Form daneben (VOB-Prüfung G3): Grad mit einer Stelle
    if let Some(n) = boeschung(p) {
        values.push(("Böschungswinkel", format!("{} °", de(grad(n), 1))));
    }
    let s = m.site_soil();
    let befestigt = if p.paved {
        "befestigt: ja"
    } else {
        "befestigt: nein"
    };
    Some(Props {
        values,
        sections: vec![
            Section {
                title: if streifen.is_some() {
                    LABEL_HAUS
                } else {
                    "Geländefläche"
                },
                fields,
                hint: match (streifen.is_some(), p.paved) {
                    (true, _) => HINWEIS_HAUS,
                    (false, true) => HINWEIS_BEFESTIGT,
                    (false, false) => HINWEIS,
                },
                buttons: vec![
                    (Id::Gelaende(GelaendeKnopf::Befestigt), befestigt),
                    (Id::Gelaende(GelaendeKnopf::Loeschen), "Fläche löschen"),
                ],
            },
            Section {
                title: "Grundstück",
                fields: vec![
                    feld(GelaendeFeld::Andecken, SiteSoil::FELDER[0].1, s.spread, {
                        let f = SiteSoil::FELDER[0];
                        (f.2, f.3)
                    }),
                    FieldRow {
                        field: Field::Gelaende(GelaendeFeld::Faktor),
                        label: SiteSoil::FELDER[1].1,
                        value: s.reuse,
                        min: SiteSoil::FELDER[1].2,
                        max: SiteSoil::FELDER[1].3,
                        zero: false,
                        einheit: Some(""),
                    },
                ],
                hint: HINWEIS_BODEN,
                buttons: Vec::new(),
            },
        ],
        ..Default::default()
    })
}

/// Die Fläche `i` mit Feld `f` auf `v`; `None`, wenn der Wert nicht gilt.
pub(crate) fn mit_feld(p: &TerrainPatch, f: GelaendeFeld, v: f64) -> Option<TerrainPatch> {
    let mut q = p.clone();
    match f {
        GelaendeFeld::Hoehe => q.height = PatchHeight::Flat(v),
        GelaendeFeld::Breite => {
            let s = q.house.as_mut()?;
            q.outline = HouseStrip::aussen(&s.house, v)?;
            s.width = v;
        }
        GelaendeFeld::Gefaelle => q.house.as_mut()?.fall = v,
        GelaendeFeld::Boeschung => {
            let (a, b) = PATCH_SLOPE_RANGE;
            if v != 0.0 && !(a..=b).contains(&v) {
                return None;
            }
            q.slopes = vec![v];
        }
        _ => return None,
    }
    q.valid().then_some(q)
}

impl App {
    /// Ereignis an das Werkzeug „Fläche zeichnen“; `true`, wenn es das
    /// Ereignis genommen hat. Ein geschlossener Umriss wird eine Fläche
    /// (ein Schritt) und ist dann gewählt.
    pub(crate) fn flaeche_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        if !self.platte.leiste.werkzeug.aktiv || self.ui.view != ViewKind::Plan {
            return false;
        }
        let vor = self.platte.leiste.werkzeug.ecken().len();
        let rev = self.scene.model().revision();
        let scene = &self.scene;
        let w = &mut self.platte.leiste.werkzeug;
        w.ziele_fuer(rev, || fang::ziele(scene));
        let out = w.handle(ev, &self.cam, vw, vh, sc);
        self.flaeche_ausgang(out, vor)
    }

    /// Mausbewegung an Nordpfeil und Werkzeug „Fläche zeichnen“; `true`,
    /// wenn das Werkzeug sie im Grundriss nimmt: Band, Griffe, Schnittlinie
    /// und Bauteile zeigen dann keinen Hover (H-189c).
    pub(crate) fn nord_hover(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        self.nord_handle(ev, vw, vh, sc)
            && self.platte.leiste.werkzeug.aktiv
            && self.ui.view == ViewKind::Plan
    }

    /// Tasten beim Zeichnen, nur im Grundriss (H-189g).
    pub(crate) fn flaeche_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        let mut out = Ausgang::default();
        let vor = self.platte.leiste.werkzeug.ecken().len();
        if self.ui.view != ViewKind::Plan || !self.platte.leiste.werkzeug.key(key, mods, &mut out) {
            return false;
        }
        self.flaeche_ausgang(out, vor);
        true
    }

    /// Statuszeile leer: die Anleitung des geführten Nordpfeils oder des
    /// Flächenwerkzeugs kehrt zurück, auch nach einer Meldung wie „zu wenige
    /// Ecken“ (Bedienbarkeit, später zu #204).
    pub(crate) fn zeile_zurueck(&mut self) {
        if self.nord_fuehrung.laeuft {
            self.nord_zeile();
        } else if self.platte.leiste.werkzeug.aktiv {
            self.status(
                crate::meldung::Meldung::satz(self.platte.leiste.werkzeug.zeile()),
                std::time::Duration::from_secs(3600),
            );
        }
    }

    /// Das Werkzeug ist aus: seine Zeile geht aus der Statuszeile.
    pub(crate) fn flaeche_zeile_ende(&mut self) {
        if self.platte.leiste.werkzeug.aktiv
            || !self
                .notice
                .as_ref()
                .is_some_and(|n| n.text == ZEICHNEN || n.text == GRENZE)
        {
            return;
        }
        self.notice = None;
        self.renderer
            .set_overlay(crate::OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
        self.redraw = true;
    }

    /// Esc beim Zeichnen.
    pub(crate) fn flaeche_escape(&mut self) -> bool {
        if !self.platte.leiste.werkzeug.escape() {
            return false;
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
        self.redraw = true;
        true
    }

    /// `vor`: Ecken vor dem Ereignis; die Hinweise der Leiste folgen, wenn
    /// die erste gesetzt oder die letzte genommen ist.
    fn flaeche_ausgang(&mut self, out: Ausgang, vor: usize) -> bool {
        self.redraw |= out.redraw;
        if (vor == 0) != self.platte.leiste.werkzeug.ecken().is_empty() {
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        }
        if let Some(f) = out.fehler {
            self.status(crate::meldung::Meldung::satz(f), crate::NOTICE_TIME);
        }
        match out.commit {
            Some(ring) if self.platte.leiste.werkzeug.grenze => self.grenze_anlegen(ring),
            Some(ring) => self.flaeche_anlegen(ring),
            None => {}
        }
        out.consumed
    }

    /// Neue Fläche aus dem Umriss `ring`: eben auf dem Gelände in ihrer
    /// Mitte, auf 1 cm; gewählt. Ohne Punkte legt derselbe Schritt das
    /// ebene Gelände mit Punkten an, auf dem sie liegt (I1).
    fn flaeche_anlegen(&mut self, ring: Vec<[f64; 2]>) {
        let m = self.scene.model();
        let c = mitte(&ring);
        let z = (m.terrain_z_at(m.level_building(), c[0], c[1]) / 10.0).round() * 10.0 + 0.0;
        let p = TerrainPatch::flat(ring.clone(), z);
        let mut neu = None;
        if self.scene.edit_model(LABEL_NEU, |m| {
            m.seed_flat_terrain(&ring);
            neu = m.add_site_patch(p);
            neu.is_some()
        }) {
            self.upload_model();
            self.quantity.dirty = true;
        }
        if neu.is_none() {
            self.platte.leiste.werkzeug.ecken_zurueck(ring);
            self.status(crate::meldung::Meldung::satz(ZU_HOCH), crate::NOTICE_TIME);
        }
        if let Some(i) = neu {
            self.flaeche_waehlen(Some(i));
            // Mengen zählen dort doppelt (Bedienbarkeit 43.1)
            if self.scene.model().site_patch_overlaps(i) {
                let m = crate::meldung::Meldung::satz(sk_model::qto_earth::UEBERLAPPT);
                self.status(m, crate::NOTICE_TIME);
            }
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
    }

    /// Neue Grenze aus dem Umriss `ring` (I2), ein Schritt; das Werkzeug
    /// geht aus, die Statuszeile nennt das Grundstück.
    fn grenze_anlegen(&mut self, ring: Vec<[f64; 2]>) {
        if !self.scene.edit_model(LABEL_GRENZE, |m| m.add_parcel(&ring)) {
            self.platte.leiste.werkzeug.ecken_zurueck(ring);
            let satz = crate::meldung::Meldung::satz(GRENZE_OHNE_INHALT);
            self.status(satz, crate::NOTICE_TIME);
            return;
        }
        self.upload_model();
        self.quantity.dirty = true;
        self.platte.leiste.werkzeug.set_aktiv(false);
        self.platte.leiste.neu = true;
        self.gelaende_sync();
        self.flaeche_zeile_ende();
        let qm = inhalt(&ring) / 1e6;
        let satz =
            crate::meldung::Meldung::mit("Grundstück {} m² (aus der Zeichnung)", &[&de(qm, 2)]);
        self.status(satz, crate::NOTICE_TIME);
    }

    /// Ein Werkzeug in Eingabe, etwas gezogen oder Ecken einer Fläche
    /// gesetzt: Rückgängig, Wiederherstellen und Löschen warten (Review
    /// #189 B1).
    pub(crate) fn im_zug(&self) -> bool {
        self.tool.is_active()
            || self.edit.is_dragging()
            || self.drains.is_dragging()
            || self.platte.leiste.werkzeug.zeichnet()
    }

    /// Rückgängig und Wiederherstellen (Knopf, Menü, Kürzel) beim Zeichnen:
    /// Rückgängig nimmt die letzte Ecke, Wiederherstellen tut nichts.
    /// `true`, wenn Ecken gesetzt sind; dann bleibt das Modell (H-189h).
    pub(crate) fn flaeche_zurueck(&mut self, redo: bool) -> bool {
        if !self.platte.leiste.werkzeug.zeichnet() {
            return false;
        }
        if !redo {
            let strg = Modifiers {
                ctrl: true,
                shift: false,
                alt: false,
            };
            self.flaeche_taste(Key::Char('Z'), strg);
        }
        true
    }

    /// Entf: die gewählte Fläche löschen, beim Zeichnen nichts; `false`,
    /// wenn Entf den Bauteilen gilt.
    pub(crate) fn flaeche_entf(&mut self) -> bool {
        if self.im_zug() {
            return true;
        }
        if self.picking.flaeche.is_none() || !self.picking.selected.is_empty() {
            return false;
        }
        self.flaeche_knopf(GelaendeKnopf::Loeschen);
        true
    }

    /// Nach einem Wechsel der Sicht: Nordpfeil nur, wo er geht, das
    /// Werkzeug „Fläche zeichnen“ nur im Grundriss (H-189g).
    pub(crate) fn sicht_werkzeuge(&mut self) {
        if !self.tool_allowed() {
            self.nord.set_aktiv(false);
        }
        if self.ui.view != ViewKind::Plan && self.platte.leiste.werkzeug.aktiv {
            self.platte.leiste.werkzeug.set_aktiv(false);
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        }
    }

    /// Fläche `i` wählen (oder keine); Bauteile und Platte sind dann nicht
    /// gewählt.
    pub(crate) fn flaeche_waehlen(&mut self, i: Option<usize>) {
        if i.is_some() {
            self.picking.selected.clear();
            self.picking.plate = false;
            self.sel.set(None);
        }
        if self.picking.flaeche != i {
            self.picking.flaeche = i;
            self.quantity.dirty = true;
        }
        self.redraw = true;
    }

    /// Klick ins Leere im Grundriss: die Fläche darunter wählen, daneben
    /// eine gewählte abwählen (H-189f); in 3D wählt ein Klick auf die
    /// grüne Geländeplatte die Platte ([`App::platte_klick`]).
    /// `true`, wenn der Klick einer Fläche oder der Platte galt.
    pub(crate) fn flaeche_klick(&mut self, x: f64, y: f64, vw: f64, vh: f64) -> bool {
        let frei = self.ui.frei.frei(crate::freischaltung::Modul::Gelaende);
        let g = self.cam.ground_point(x, y, vw, vh);
        let i = g
            .filter(|_| frei && self.ui.view == ViewKind::Plan && !self.scene.terrain_hidden())
            .and_then(|g| flaeche_bei(self.scene.model(), [g.x, g.y]));
        if i.is_none() && self.platte_klick(x, y, vw, vh) {
            return true;
        }
        if i.is_none() && self.picking.flaeche.is_none() {
            return false;
        }
        self.flaeche_waehlen(i);
        true
    }

    /// Klick ins Leere auf das grüne Gelände in 3D wählt es wie seine
    /// Zeile im Baum (Bedienbarkeit Durchgang 41: „nur im Baum wählbar“):
    /// ohne Punkte die Platte, mit Punkten das Netz (I1); wann das geht,
    /// sagt [`platte_waehlbar`]. `true`, wenn der Klick das Gelände traf.
    fn platte_klick(&mut self, x: f64, y: f64, vw: f64, vh: f64) -> bool {
        let m = self.scene.model();
        if !platte_waehlbar(self.ui.view, self.scene.terrain_hidden()) {
            return false;
        }
        let cam = &self.cam;
        let traf = if ruhen(m) {
            platte_getroffen(self.platte.im_bild(m), |z| cam.plane_point(x, y, vw, vh, z))
        } else {
            let (o, d) = cam.ray(x, y, vw, vh);
            self.platte
                .gelaende_im_bild(m)
                .is_some_and(|g| g.getroffen(o, d))
        };
        if traf && !self.picking.plate {
            self.platte_waehlen();
        }
        traf
    }

    /// Paneel „Eigenschaften“ der gewählten Fläche; `true`, wenn es sie
    /// zeigt. Je Modellstand einmal gebaut.
    pub(crate) fn flaeche_props(&mut self) -> bool {
        let m = self.scene.model();
        let i = gewaehlte(m, self.picking.flaeche).filter(|_| self.sel.id.is_none());
        if i.is_none() && self.picking.flaeche.is_some() {
            self.picking.flaeche = None;
        }
        let soll = i.map(|i| (i, m.revision()));
        if soll == self.platte.leiste.props {
            return soll.is_some();
        }
        let war = self.platte.leiste.props;
        self.platte.leiste.props = soll;
        match i.and_then(|i| props(m, i)) {
            Some(p) => {
                if war.map(|w| w.0) != i {
                    self.ui.reset_props_scroll();
                }
                self.ui.set_props(Some(p));
            }
            None => {
                self.ui.set_props(None);
                self.props_key = None;
            }
        }
        self.props_dirty = true;
        soll.is_some()
    }

    /// Knöpfe der gewählten Fläche und das Werkzeug.
    pub(crate) fn flaeche_knopf(&mut self, k: GelaendeKnopf) {
        match k {
            GelaendeKnopf::Flaeche | GelaendeKnopf::Grenze => {
                let w = &mut self.platte.leiste.werkzeug;
                w.umschalten(k == GelaendeKnopf::Grenze);
                let an = w.aktiv;
                let zeile = w.zeile();
                if an {
                    // gezeichnet wird im Grundriss, wo der Klick den Boden trifft
                    if self.ui.view != ViewKind::Plan {
                        self.set_view(ViewKind::Plan);
                    }
                    self.tool.set_enabled(false);
                    self.nord.set_aktiv(false);
                    self.refresh_cursor();
                    // steht, solange das Werkzeug an ist, wie beim Nordpfeil
                    self.status(
                        crate::meldung::Meldung::satz(zeile),
                        std::time::Duration::from_secs(3600),
                    );
                }
            }
            GelaendeKnopf::AmHaus => self.am_haus(),
            GelaendeKnopf::Befestigt => {
                if let Some(i) = self.picking.flaeche {
                    let p = self.scene.model().site_patches().get(i).cloned();
                    if let Some(mut p) = p {
                        p.paved = !p.paved;
                        self.flaeche_schritt(LABEL_AENDERN, |m| m.set_site_patch(i, p));
                    }
                }
            }
            GelaendeKnopf::Loeschen => {
                if let Some(i) = self.picking.flaeche {
                    if self.flaeche_schritt(LABEL_LOESCHEN, |m| m.remove_site_patch(i)) {
                        self.flaeche_waehlen(None);
                    }
                }
            }
            _ => {}
        }
        self.redraw = true;
    }

    /// Feld im Paneel der Fläche übernommen; `true`, wenn es zu ihr
    /// gehört. Ein Wert, der nicht gilt, ändert nichts.
    pub(crate) fn flaeche_feld(&mut self, f: GelaendeFeld, v: f64) -> bool {
        match f {
            GelaendeFeld::Hoehe
            | GelaendeFeld::Boeschung
            | GelaendeFeld::Breite
            | GelaendeFeld::Gefaelle => {
                let Some(i) = self.picking.flaeche else {
                    return true;
                };
                let p = self.scene.model().site_patches().get(i).cloned();
                if let Some(q) = p.and_then(|p| mit_feld(&p, f, v)) {
                    self.flaeche_schritt(LABEL_AENDERN, |m| m.set_site_patch(i, q));
                }
                true
            }
            GelaendeFeld::Andecken | GelaendeFeld::Faktor => {
                let mut s = self.scene.model().site_soil();
                if f == GelaendeFeld::Andecken {
                    s.spread = v;
                } else {
                    s.reuse = (v * 1000.0).round() / 1000.0;
                }
                self.flaeche_schritt(LABEL_BODEN, |m| m.set_site_soil(s));
                true
            }
            _ => false,
        }
    }

    /// „Gelände ans Haus“: den Streifen um das Haus der Höhenlage anlegen
    /// und wählen; hat es schon einen, ihn wählen.
    fn am_haus(&mut self) {
        let m = self.scene.model();
        let Some(b) = m.level_building() else {
            self.status(crate::meldung::Meldung::satz(KEIN_HAUS), crate::NOTICE_TIME);
            return;
        };
        if let Some(i) = m.house_strip_of(b) {
            self.flaeche_waehlen(Some(i));
            self.status(crate::meldung::Meldung::satz(SCHON), crate::NOTICE_TIME);
            return;
        }
        let Some(p) = m.house_strip(b) else {
            let satz = if m.house_ring(b).is_some() {
                NICHT_UM
            } else {
                KEIN_HAUS
            };
            self.status(crate::meldung::Meldung::satz(satz), crate::NOTICE_TIME);
            return;
        };
        let breite = p.house.as_ref().map_or(STRIP_WIDTH, |s| s.width);
        let ring = p.outline.clone();
        let mut neu = None;
        // ohne Punkte legt derselbe Schritt das ebene Gelände an (I1)
        if self.flaeche_schritt(LABEL_HAUS, |m| {
            m.seed_flat_terrain(&ring);
            neu = m.add_site_patch(p);
            neu.is_some()
        }) {
            self.flaeche_waehlen(neu);
            if neu.is_some_and(|i| self.scene.model().site_patch_overlaps(i)) {
                let m = crate::meldung::Meldung::satz(sk_model::qto_earth::UEBERLAPPT);
                self.status(m, crate::NOTICE_TIME);
            } else if breite < STRIP_WIDTH {
                let m = crate::meldung::Meldung::mit(SCHMAL, &[&de(breite / 1000.0, 2)]);
                self.status(m, crate::NOTICE_TIME);
            }
        }
        self.platte.leiste.neu = true;
        self.gelaende_sync();
    }

    /// Je Durchlauf: meldet einmal, was der Abgleich der Streifen beim
    /// letzten Schritt zu sagen hat, schmaler geworden oder stehen
    /// geblieben (Bedienbarkeit zu #226).
    pub(crate) fn streifen_pruefen(&mut self) {
        let Some(n) = self.scene.model().house_strip_note() else {
            return;
        };
        if n.serial == self.platte.streifen_gemeldet {
            return;
        }
        self.platte.streifen_gemeldet = n.serial;
        let m = match n.width {
            Some(w) => crate::meldung::Meldung::mit(SCHMALER, &[&de(w / 1000.0, 2)]),
            None => crate::meldung::Meldung::satz(STEHT),
        };
        self.status(m, crate::NOTICE_TIME);
    }

    /// Eine Änderung an den Flächen als ein Schritt; Bild und Mengen
    /// folgen.
    fn flaeche_schritt(&mut self, label: &'static str, f: impl FnOnce(&mut Model) -> bool) -> bool {
        let neu = self.scene.edit_model(label, f);
        if neu {
            self.upload_model();
            self.quantity.dirty = true;
            self.props_dirty = true;
        }
        neu
    }

    /// Hilfslinien: das Werkzeug beim Zeichnen und der Umriss der
    /// gewählten Fläche.
    pub(crate) fn flaeche_linien(&self, scale: f32) -> Vec<Helper> {
        let m = self.scene.model();
        let z = m.terrain_z();
        let mut out = self.platte.leiste.werkzeug.linien(z, scale, &self.theme);
        if let Some(p) = self.picking.flaeche.and_then(|i| m.site_patches().get(i)) {
            let h0 = m.terrain_nhn().unwrap_or(0.0);
            let n = p.outline.len();
            let q = |k: usize| {
                let [x, y] = p.outline[k % n];
                let z = p.z_at(x, y, h0);
                [x as f32, y as f32, z as f32 + 1.0]
            };
            for k in 0..n {
                out.push(Helper {
                    a: q(k),
                    b: q(k + 1),
                    color: self.theme.interact.select,
                    width: 3.0 * scale,
                    dash: 0.0,
                    pattern: sk_render::SOLID,
                    occlude: false,
                    round: false,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::Vec3;
    use std::f64::consts::FRAC_PI_2;

    const OHNE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
    };

    fn punkt(x: f64, y: f64) -> Vec3 {
        vec3(x, y, 0.0)
    }

    /// Grundriss von oben über 0..20 m, 800 × 800 Bildpunkte.
    fn kamera() -> (Camera, f64, f64) {
        let c = Camera::parallel(punkt(10000.0, 10000.0), FRAC_PI_2, -FRAC_PI_2, 12500.0);
        (c, 800.0, 800.0)
    }

    fn klick(w: &mut Werkzeug, cam: &Camera, p: Vec3) -> Ausgang {
        let (x, y) = cam.project(p, 800.0, 800.0).unwrap();
        w.handle(
            &Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: Modifiers::default(),
            },
            cam,
            800.0,
            800.0,
            1.0,
        )
    }

    /// Vier Klicks und ein Klick auf die erste Ecke: eine Fläche mit vier
    /// Ecken auf dem Raster.
    #[test]
    fn rechteck_schliesst_an_der_ersten_ecke() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [
            (2001.0, 2002.0),
            (8000.0, 2000.0),
            (8000.0, 6000.0),
            (2000.0, 6000.0),
        ] {
            let o = klick(&mut w, &cam, punkt(p.0, p.1));
            assert!(o.consumed && o.commit.is_none());
        }
        assert_eq!(w.ecken().len(), 4);
        let o = klick(&mut w, &cam, punkt(2003.0, 2004.0));
        let ring = o.commit.expect("geschlossen");
        assert_eq!(ring.len(), 4);
        for q in &ring {
            assert_eq!(q[0] % STEP, 0.0);
            assert_eq!(q[1] % STEP, 0.0);
        }
        assert!(w.ecken().is_empty() && w.aktiv);
        // ließ sich die Fläche nicht anlegen, stehen die Ecken wieder da
        // (H-189d); bei beendetem Werkzeug nicht
        w.ecken_zurueck(ring.clone());
        assert_eq!(w.ecken(), &ring[..]);
        w.set_aktiv(false);
        w.ecken_zurueck(ring);
        assert!(w.ecken().is_empty());
    }

    /// Rücktaste nimmt die letzte Ecke, Enter schließt, Esc verwirft erst
    /// die Ecken, dann das Werkzeug.
    #[test]
    fn tasten() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [
            (0.0, 0.0),
            (5000.0, 0.0),
            (5000.0, 5000.0),
            (9000.0, 9000.0),
        ] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        assert!(w.key(Key::Backspace, OHNE, &mut o));
        assert_eq!(w.ecken().len(), 3);
        assert!(w.key(Key::Enter, OHNE, &mut o));
        assert_eq!(o.commit.map(|r| r.len()), Some(3));
        klick(&mut w, &cam, punkt(0.0, 0.0));
        assert!(w.escape());
        assert!(w.aktiv && w.ecken().is_empty());
        assert!(w.escape());
        assert!(!w.aktiv);
        assert!(!w.escape());
    }

    /// Bei zwei Ecken setzt ein Klick auf die erste sie nicht noch einmal,
    /// und Enter sagt, warum nichts geschieht (Bedienbarkeit, später zu
    /// #189).
    #[test]
    fn zu_wenig_ecken() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        assert!(klick(&mut w, &cam, punkt(0.0, 0.0)).fehler.is_none());
        // gleich daneben, noch eine Ecke: still
        assert!(klick(&mut w, &cam, punkt(30.0, 0.0)).fehler.is_none());
        assert_eq!(w.ecken().len(), 1);
        klick(&mut w, &cam, punkt(5000.0, 0.0));
        let o = klick(&mut w, &cam, punkt(20.0, 10.0));
        assert_eq!(o.fehler, Some(ZU_WENIG));
        assert!(o.commit.is_none());
        assert_eq!(w.ecken().len(), 2);
        let mut o = Ausgang::default();
        assert!(w.key(Key::Enter, OHNE, &mut o));
        assert_eq!(o.fehler, Some(ZU_WENIG));
        assert_eq!(w.ecken().len(), 2);
        klick(&mut w, &cam, punkt(5000.0, 5000.0));
        let o = klick(&mut w, &cam, punkt(0.0, 0.0));
        assert_eq!(o.commit.map(|r| r.len()), Some(3));
    }

    /// Ein Umriss mit sich kreuzenden Kanten wird keine Fläche; die Ecken
    /// bleiben stehen.
    #[test]
    fn kreuzende_kanten() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        for p in [(0.0, 0.0), (6000.0, 6000.0), (6000.0, 0.0), (0.0, 6000.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert!(o.commit.is_none());
        assert_eq!(o.fehler, Some(KREUZT));
        assert_eq!(w.ecken().len(), 4);
        // drei Ecken auf einer Linie: kein Inhalt statt „kreuzen“ (H-189d)
        w.escape();
        for p in [(0.0, 0.0), (3000.0, 0.0), (6000.0, 0.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert_eq!(o.fehler, Some(OHNE_INHALT));
    }

    /// „Grenze zeichnen“ (I2): derselbe Knopf schaltet aus, „Fläche
    /// zeichnen“ wechselt und verwirft die Ecken; Statuszeile und Fehler
    /// sprechen von der Grenze, geschlossen wird wie bei einer Fläche.
    #[test]
    fn grenze_zeichnen() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.umschalten(true);
        assert!(w.aktiv && w.grenze);
        assert_eq!(w.zeile(), GRENZE);
        klick(&mut w, &cam, punkt(0.0, 0.0));
        w.umschalten(false);
        assert!(w.aktiv && !w.grenze && w.ecken().is_empty());
        assert_eq!(w.zeile(), ZEICHNEN);
        w.umschalten(true);
        for p in [(0.0, 0.0), (6000.0, 0.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert_eq!(o.fehler, Some(GRENZE_ZU_WENIG));
        klick(&mut w, &cam, punkt(0.0, 6000.0));
        klick(&mut w, &cam, punkt(6000.0, 6000.0));
        let mut o = Ausgang::default();
        w.key(Key::Enter, OHNE, &mut o);
        assert_eq!(o.fehler, Some(GRENZE_KREUZT));
        w.key(Key::Backspace, OHNE, &mut o);
        let o = klick(&mut w, &cam, punkt(0.0, 0.0));
        assert_eq!(o.commit.map(|r| r.len()), Some(3));
        w.umschalten(true);
        assert!(!w.aktiv, "derselbe Knopf schaltet aus");
    }

    /// Beim Zeichnen nimmt Strg+Z die letzte Ecke; Strg+Y, Strg+Umschalt+Z
    /// und Entf verbraucht das Werkzeug, damit sie nicht aufs Modell wirken
    /// (Review #189 B1). Ohne Ecken gehören die Tasten wieder der App.
    #[test]
    fn rueckgaengig_beim_zeichnen() {
        let (cam, _, _) = kamera();
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        let mut o = Ausgang::default();
        let strg = Modifiers { ctrl: true, ..OHNE };
        let umschalt = Modifiers {
            shift: true,
            ..strg
        };
        assert!(!w.zeichnet() && !w.key(Key::Char('Z'), strg, &mut o));
        for p in [(0.0, 0.0), (5000.0, 0.0), (5000.0, 5000.0)] {
            klick(&mut w, &cam, punkt(p.0, p.1));
        }
        assert!(w.zeichnet());
        for (k, m) in [('Y', strg), ('Z', umschalt)] {
            assert!(w.key(Key::Char(k), m, &mut o));
        }
        assert!(w.key(Key::Delete, OHNE, &mut o));
        assert!(!w.key(Key::Char('Z'), OHNE, &mut o), "Z ohne Strg");
        assert_eq!(w.ecken().len(), 3);
        assert!(w.key(Key::Char('Z'), strg, &mut o));
        assert_eq!(w.ecken().len(), 2);
        assert!(o.commit.is_none());
        w.key(Key::Char('Z'), strg, &mut o);
        w.key(Key::Char('Z'), strg, &mut o);
        assert!(!w.zeichnet() && !w.key(Key::Delete, OHNE, &mut o));
    }

    /// Klick in eine Fläche trifft die zuletzt gesetzte, außerhalb keine.
    #[test]
    fn treffer() {
        let mut m = Model::with_seed(1);
        m.begin("Flächen");
        let r = |x0: f64, x1: f64| vec![[x0, 0.0], [x1, 0.0], [x1, 4000.0], [x0, 4000.0]];
        m.add_site_patch(TerrainPatch::flat(r(0.0, 6000.0), 0.0));
        m.add_site_patch(TerrainPatch::flat(r(4000.0, 9000.0), 0.0));
        m.commit();
        // ohne Aufmaß ruhen sie
        assert_eq!(flaeche_bei(&m, [1000.0, 1000.0]), None);
        let mut text = String::new();
        for i in 0..5 {
            for j in 0..5 {
                text.push_str(&format!("{} {} {} 10.0\n", i * 5 + j + 1, j * 5, i * 5));
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-15", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        assert_eq!(flaeche_bei(&m, [1000.0, 1000.0]), Some(0));
        assert_eq!(flaeche_bei(&m, [5000.0, 1000.0]), Some(1));
        assert_eq!(flaeche_bei(&m, [9500.0, 1000.0]), None);
        // Strg+Z des Einlesens: die gewählte Fläche ruht und ist nicht mehr
        // gewählt (H-196g)
        assert_eq!(gewaehlte(&m, Some(1)), Some(1));
        assert_eq!(gewaehlte(&m, Some(2)), None);
        m.begin("zurück");
        assert!(m.set_terrain(None));
        m.commit();
        assert_eq!(gewaehlte(&m, Some(1)), None);
    }

    /// Klick auf die Platte trifft eines ihrer Trapeze, auch das mit zwei
    /// zusammenfallenden Ecken; das Loch fürs Haus und daneben nicht
    /// (Bedienbarkeit Durchgang 41).
    #[test]
    fn platte_treffen() {
        let q = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| {
            [a, b, c, d].map(|e| punkt(e[0], e[1]))
        };
        // Ring 0..10 m um ein Haus 3..7 m, unten ein Rechteck, rechts ein
        // Dreieck (zwei Ecken gleich)
        let t = sk_model::TerrainPlate {
            buildings: Vec::new(),
            top: 0.0,
            bottom: -500.0,
            pieces: vec![
                q(
                    [0.0, 0.0],
                    [10000.0, 0.0],
                    [7000.0, 3000.0],
                    [3000.0, 3000.0],
                ),
                q(
                    [7000.0, 3000.0],
                    [10000.0, 0.0],
                    [10000.0, 10000.0],
                    [10000.0, 10000.0],
                ),
            ],
            edges: Vec::new(),
        };
        assert!(platte_bei(&t, [5000.0, 1000.0]));
        assert!(platte_bei(&t, [9500.0, 5000.0]));
        assert!(!platte_bei(&t, [5000.0, 5000.0]), "Loch fürs Haus");
        assert!(!platte_bei(&t, [11000.0, 5000.0]), "daneben");
        assert!(!platte_bei(&t, [5000.0, -100.0]), "daneben");
    }

    /// Review #212 B1, H-212a: Die Platte wählt nur ein Klick in 3D; im
    /// Grundriss ist sie unsichtbar, dort wählt ein Klick neben das Haus ab
    /// (`flaeche_klick` gibt `false`, die App wählt nichts). Ausgeblendet
    /// nicht; das Modul fragt [`platte_waehlbar`] nicht ab, das Gelände
    /// gibt es in jeder Ausbaustufe, mit Höhenaufmaß wie ohne (I1).
    #[test]
    fn platte_nur_in_3d() {
        for v in [
            ViewKind::Plan,
            ViewKind::Section,
            ViewKind::Front,
            ViewKind::Back,
            ViewKind::Left,
            ViewKind::Right,
        ] {
            assert!(!platte_waehlbar(v, false), "{v:?}");
        }
        assert!(platte_waehlbar(ViewKind::Persp, false));
        assert!(!platte_waehlbar(ViewKind::Persp, true), "ausgeblendet");
        // der Strahl auf OK Platte: im Ring getroffen, im Haus und daneben
        // nicht
        let t = sk_model::TerrainPlate {
            buildings: Vec::new(),
            top: -200.0,
            bottom: -700.0,
            pieces: vec![[
                punkt(0.0, 0.0),
                punkt(10000.0, 0.0),
                punkt(10000.0, 2000.0),
                punkt(0.0, 2000.0),
            ]],
            edges: Vec::new(),
        };
        let bei = |x: f64, y: f64| move |z: f64| Some(vec3(x, y, z));
        assert!(platte_getroffen(
            std::slice::from_ref(&t),
            bei(5000.0, 1000.0)
        ));
        assert!(!platte_getroffen(
            std::slice::from_ref(&t),
            bei(5000.0, 3000.0)
        ));
        assert!(!platte_getroffen(std::slice::from_ref(&t), |_| None));
        assert!(!platte_getroffen(&[], bei(5000.0, 1000.0)));
    }

    /// Paneel: Höhe und Böschung als Felder, solange sie für die ganze
    /// Fläche gelten; Werte, die nicht gelten, ändern nichts.
    #[test]
    fn paneel_und_felder() {
        let mut m = Model::with_seed(1);
        m.begin("Fläche");
        let ring = vec![[0.0, 0.0], [5000.0, 0.0], [5000.0, 4000.0], [0.0, 4000.0]];
        m.add_site_patch(TerrainPatch::flat(ring, -600.0));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert!(p.values.contains(&("Fläche", "20,00 m²".to_string())));
        assert!(p.values.iter().all(|v| v.0 != "Höhe ü. NHN"), "ohne Aufmaß");
        let f = &p.sections[0].fields;
        assert_eq!(f[0].field, Field::Gelaende(GelaendeFeld::Hoehe));
        assert_eq!(f[0].value, -600.0);
        assert_eq!(f[1].value, PATCH_SLOPE);
        // 1:1,5 daneben in Grad (VOB-Prüfung G3)
        assert!(p
            .values
            .contains(&("Böschungswinkel", "33,7 °".to_string())));
        assert_eq!(grad(0.0), 90.0);
        assert_eq!(p.sections[1].fields[0].value, 200.0);
        // ein Name wie im Modell, Hinweise zu Faktor und befestigt (43.1 B)
        assert_eq!(
            p.sections[1].fields[1].label,
            "Eigenboden fest zu eingebaut"
        );
        assert!(p.sections[1].hint.contains("unter 1: Boden setzt sich"));
        assert_eq!(p.sections[0].hint, HINWEIS);
        assert!(HINWEIS_BEFESTIGT.contains(sk_model::qto_earth::BEFESTIGT));
        let mut b = m.site_patches()[0].clone();
        b.paved = true;
        m.begin("befestigt");
        assert!(m.set_site_patch(0, b));
        m.commit();
        assert_eq!(props(&m, 0).unwrap().sections[0].hint, HINWEIS_BEFESTIGT);
        let q = &m.site_patches()[0];
        let h = mit_feld(q, GelaendeFeld::Hoehe, 300.0).unwrap();
        assert_eq!(h.height, PatchHeight::Flat(300.0));
        assert_eq!(
            mit_feld(q, GelaendeFeld::Boeschung, 0.0).unwrap().slopes,
            vec![0.0]
        );
        assert!(mit_feld(q, GelaendeFeld::Boeschung, 0.2).is_none());
        assert!(mit_feld(q, GelaendeFeld::Boeschung, 25.0).is_none());
        // je Kante verschieden: kein Feld, ein Wert sagt es
        let mut q2 = q.clone();
        q2.slopes = vec![1.0, 2.0, 1.0, 2.0];
        m.begin("Fläche");
        assert!(m.set_site_patch(0, q2));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert_eq!(p.sections[0].fields.len(), 1);
        assert!(p
            .values
            .contains(&("Böschung", "je Kante, aus der Datei".to_string())));
        assert!(!p.values.iter().any(|v| v.0 == "Böschungswinkel"));
        assert!(props(&m, 1).is_none());
        // mit Aufmaß und ±0,00 = 24,95 m ü. NHN: −0,60 m stehen als 24,35
        let mut text = String::new();
        for i in 0..5 {
            for j in 0..5 {
                text.push_str(&format!("{} {} {} 24.5\n", i * 5 + j + 1, j * 5, i * 5));
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-15", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        assert!(m.set_terrain_nhn(24.95));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert!(p.values.contains(&("Höhe ü. NHN", "24,35 m".to_string())));
    }

    /// „Gelände ans Haus“: Das Paneel nennt den Streifen und sein Haus,
    /// Höhe am Haus, Breite und Gefälle; ein Klick trifft ihn um das Haus,
    /// nicht im Haus; Breite und Gefälle außerhalb ihrer Bereiche ändern
    /// nichts.
    #[test]
    fn streifen_am_haus() {
        let mut m = Model::with_seed(1);
        let b = m.add_building(1);
        let pts = [
            punkt(7000.0, 6000.0),
            punkt(7000.0, 14000.0),
            punkt(17000.0, 14000.0),
            punkt(17000.0, 6000.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let mut text = String::new();
        for i in 0..5 {
            for j in 0..5 {
                text.push_str(&format!("{} {} {} 24.5\n", i * 5 + j + 1, j * 5, i * 5));
            }
        }
        let (t, ..) = sk_model::Terrain::lesen(text.as_bytes(), "x.txt", "2026-10-15", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        assert!(m.set_terrain_nhn(24.95));
        m.commit();
        assert_eq!(m.level_building(), Some(b));
        let s = m.house_strip(b).unwrap();
        m.begin(LABEL_HAUS);
        assert_eq!(m.add_site_patch(s), Some(0));
        m.commit();
        let p = props(&m, 0).unwrap();
        assert!(p.values.contains(&("Kategorie", LABEL_HAUS.to_string())));
        // 12 × 10 m außen ohne das Haus 10 × 8 m
        assert!(
            p.values.contains(&("Fläche", "40,00 m²".to_string())),
            "{:?}",
            p.values
        );
        assert!(p
            .values
            .contains(&("am Haus ü. NHN", "24,80 m".to_string())));
        assert_eq!(p.sections[0].title, LABEL_HAUS);
        assert_eq!(p.sections[0].hint, HINWEIS_HAUS);
        let f: Vec<_> = p.sections[0]
            .fields
            .iter()
            .map(|f| (f.label, f.value))
            .collect();
        assert_eq!(
            f[..3],
            [
                ("Höhe am Haus", -150.0),
                ("Breite", 1000.0),
                ("Gefälle", 2.0)
            ]
        );
        assert_eq!(flaeche_bei(&m, [6500.0, 10_000.0]), Some(0));
        assert_eq!(flaeche_bei(&m, [12_000.0, 10_000.0]), None, "im Haus");
        assert_eq!(flaeche_bei(&m, [5500.0, 10_000.0]), None);
        let q = &m.site_patches()[0];
        let breiter = mit_feld(q, GelaendeFeld::Breite, 1500.0).unwrap();
        assert!(breiter.contains([5700.0, 10_000.0]));
        assert!(mit_feld(q, GelaendeFeld::Breite, 100.0).is_none());
        assert_eq!(
            mit_feld(q, GelaendeFeld::Gefaelle, 5.0)
                .unwrap()
                .house
                .unwrap()
                .fall,
            5.0
        );
        assert!(mit_feld(q, GelaendeFeld::Gefaelle, 30.0).is_none());
        // eine gezeichnete Fläche kennt Breite und Gefälle nicht
        let r = vec![[0.0, 0.0], [3000.0, 0.0], [3000.0, 3000.0], [0.0, 3000.0]];
        let g = TerrainPatch::flat(r, 0.0);
        assert!(mit_feld(&g, GelaendeFeld::Breite, 1500.0).is_none());
        // die Sätze passen zu den Grenzen des Streifens (Bedienbarkeit zu
        // #226): ein Platzhalter für die Breite, die kleinste Nische
        assert!([SCHMAL, SCHMALER]
            .iter()
            .all(|t| t.matches("{}").count() == 1));
        let min = format!("{} m", de(2.0 * STRIP_WIDTH_RANGE.0 / 1000.0, 2));
        assert!(NICHT_UM.contains(&min) && STEHT.contains(&min));
    }

    /// Mit gehaltener Strg-Taste kein Fang, nur das Raster (Bedienbarkeit,
    /// später zu #198).
    #[test]
    fn strg_ohne_fang() {
        let (cam, _, _) = kamera();
        let r = [[5000.0, 5000.0], [9003.0, 5000.0], [9003.0, 8001.0]];
        let mut w = Werkzeug::default();
        w.set_aktiv(true);
        w.ziele_fuer(1, || Ziele {
            ecken: r.to_vec(),
            kanten: vec![[r[0], r[1]], [r[1], r[2]], [r[2], r[0]]],
        });
        let strg = Modifiers { ctrl: true, ..OHNE };
        let (x, y) = cam.project(punkt(9104.0, 8096.0), 800.0, 800.0).unwrap();
        let ev = |mods| Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        w.handle(&ev(strg), &cam, 800.0, 800.0, 1.0);
        assert_eq!(w.ecken(), &[[9100.0, 8100.0]]);
        assert_eq!(w.fang, None);
        // ohne Strg wieder an der Ecke
        w.escape();
        w.handle(&ev(OHNE), &cam, 800.0, 800.0, 1.0);
        assert_eq!(w.ecken(), &[[9003.0, 8001.0]]);
    }

    /// Fang (H-189a): an der Ecke genau die Ecke, an der Kante der
    /// Lotfußpunkt, sonst das Raster; die Hilfslinien zeigen den Fang.
    #[test]
    fn fang_an_ecken_und_kanten() {
        let (cam, _, _) = kamera();
        let r = [[5000.0, 5000.0], [9003.0, 5000.0], [9003.0, 8001.0]];
        let ziele = || Ziele {
            ecken: r.to_vec(),
            kanten: vec![[r[0], r[1]], [r[1], r[2]], [r[2], r[0]]],
        };
        let mut w = Werkzeug::default();
        // aus: keine Ziele
        w.ziele_fuer(1, ziele);
        assert!(w.ziele.ecken.is_empty());
        w.set_aktiv(true);
        w.ziele_fuer(1, ziele);
        assert_eq!(w.ziele.ecken.len(), 3);
        // gleicher Stand: nicht neu gerechnet
        w.ziele_fuer(1, Ziele::default);
        assert_eq!(w.ziele.ecken.len(), 3);
        // 25 mm je Bildpunkt: 10 Bildpunkte sind 250 mm
        klick(&mut w, &cam, punkt(9100.0, 8100.0));
        assert_eq!(w.ecken(), &[[9003.0, 8001.0]]);
        assert_eq!(w.fang.map(|f| f.1), Some(Fang::Ecke));
        let l = w.linien(0.0, 1.0, &Theme::dark());
        assert!(l.iter().any(|h| h.width == 12.0 && !h.round), "Quadrat");
        klick(&mut w, &cam, punkt(7000.4, 4880.0));
        // auf der Kante, ungerundet (H-198c)
        let k = w.ecken()[1];
        assert!(k[1] == 5000.0 && (k[0] - 7000.4).abs() < 0.1, "{k:?}");
        assert_eq!(w.fang.map(|f| f.1), Some(Fang::Kante));
        let l = w.linien(0.0, 1.0, &Theme::dark());
        assert!(l.iter().any(|h| h.width == 10.0 && h.round), "Punkt");
        assert!(!l.iter().any(|h| h.width == 12.0), "kein Quadrat");
        klick(&mut w, &cam, punkt(12004.0, 12996.0));
        assert_eq!(w.ecken()[2], [12000.0, 13000.0]);
        assert_eq!(w.fang, None);
        // gerastet aus der Ansicht: Gummiband und Marke gehen beide weg
        // (Review #198 B1)
        let (x, y) = cam.project(punkt(9050.0, 8050.0), 800.0, 800.0).unwrap();
        let ev = Event::MouseMove {
            x,
            y,
            mods: Modifiers::default(),
        };
        w.handle(&ev, &cam, 800.0, 800.0, 1.0);
        assert!(w.fang.is_some() && w.maus.is_some());
        assert!(w.handle(&Event::MouseLeave, &cam, 800.0, 800.0, 1.0).redraw);
        assert!(w.fang.is_none() && w.maus.is_none());
        // neuer Stand: neu gerechnet; aus: verworfen
        w.ziele_fuer(2, Ziele::default);
        assert!(w.ziele.ecken.is_empty());
        w.set_aktiv(false);
        assert_eq!(w.stand, None);
    }

    /// Höhe am Schwerpunkt (H-189e); liegt er bei einem U außerhalb, die
    /// Mitte eines Schenkels.
    #[test]
    fn mitte_im_umriss() {
        let q = [[0.0, 0.0], [4000.0, 0.0], [4000.0, 2000.0], [0.0, 2000.0]];
        assert_eq!(mitte(&q), [2000.0, 1000.0]);
        // Dreieck: der Schwerpunkt
        let d = [[0.0, 0.0], [3000.0, 0.0], [0.0, 3000.0]];
        assert_eq!(mitte(&d), [1000.0, 1000.0]);
        let u = [
            [0.0, 0.0],
            [3000.0, 0.0],
            [3000.0, 3000.0],
            [2000.0, 3000.0],
            [2000.0, 1000.0],
            [1000.0, 1000.0],
            [1000.0, 3000.0],
            [0.0, 3000.0],
        ];
        let c = mitte(&u);
        assert!(innen(&u, c), "{c:?}");
        assert!((c[1] - 9500.0 / 7.0).abs() < 1e-6);
        // L: Mittel der Ecken läge draußen, der Schwerpunkt innen
        let l = [
            [0.0, 0.0],
            [6000.0, 0.0],
            [6000.0, 500.0],
            [500.0, 500.0],
            [500.0, 6000.0],
            [0.0, 6000.0],
        ];
        assert!(innen(&l, mitte(&l)), "{:?}", mitte(&l));
    }
}

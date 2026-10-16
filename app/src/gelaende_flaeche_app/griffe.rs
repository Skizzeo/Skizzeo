//! Grenze mit Griffen ändern (I3, gelaende/analyse-interaktives-gelaende.md
//! §4.2): Ist die Leiste „Gelände“ offen (§4.0 Regel 1, J9 A), zeigt jedes
//! Flurstück im Grundriss an den Ecken ein Quadrat und in jeder Kantenmitte
//! einen Punkt. Eine Ecke ziehen verschiebt sie mit Fang (Strg ohne), den
//! Punkt ziehen fügt dort eine Ecke ein und zieht sie gleich mit, ein Klick
//! fügt sie nur ein. Eine Kante quer ziehen versetzt sie parallel, die
//! Nachbarkanten behalten ihre Richtung. Entf löscht die Ecke unter der
//! Maus; eine Grenze behält drei. Was unter der Maus liegt, sagt die
//! Statuszeile, beim Ziehen die Fläche des Grundstücks. Loslassen ist ein
//! Schritt, Esc bricht ab.

use super::fang::{self, Ziele};
use super::STEP;
use crate::camera::Camera;
use crate::meldung::Meldung;
use crate::ui::ViewKind;
use crate::App;
use sk_math::vec3;
use sk_platform::{Event, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// So nah (Bildpunkte) greift die Maus eine Ecke oder einen Punkt, eine
/// Kante halb so nah.
const GREIF_PX: f64 = 8.0;
/// Größe der Quadrate und Punkte (Bildpunkte bei 96 dpi).
const ECKE_PX: f32 = 8.0;
const PLUS_PX: f32 = 6.0;

/// Statuszeile über einem Griff (§4.0 Regel 2).
pub const ECKE: &str =
    "Ecke der Grenze: ziehen verschiebt sie (mit Strg ohne Fang), Entf löscht sie.";
pub const PLUS: &str = "Neue Ecke: klicken fügt sie ein, ziehen fügt sie ein und verschiebt sie.";
pub const KANTE: &str = "Kante der Grenze: quer ziehen verschiebt sie parallel.";
/// Entf an einem Dreieck.
pub const DREI: &str = "Eine Grenze behält mindestens drei Ecken.";
/// Die gezogene Grenze ließe sich nicht bauen (Schleife, keine Fläche).
pub const GEHT_NICHT: &str =
    "So kreuzt sich die Grenze oder hat keine Fläche; sie bleibt, wie sie war.";
/// Schritte im Rückgängig (§4.0 Regel 5).
pub const LABEL_ECKE: &str = "Grenze: Ecke verschoben";
pub const LABEL_EINGEFUEGT: &str = "Grenze: Ecke eingefügt";
pub const LABEL_KANTE: &str = "Grenze: Kante verschoben";
pub const LABEL_GELOESCHT: &str = "Grenze: Ecke gelöscht";

/// Ein Griff am Flurstück `.0`: Ecke, Punkt in der Kantenmitte oder Kante
/// `.1` (von Ecke `.1` zu `.1 + 1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Griff {
    Ecke(usize, usize),
    Plus(usize, usize),
    Kante(usize, usize),
}

/// Ein Zug am Griff: Bodenpunkt beim Greifen, der Ring dabei (nach dem
/// Einfügen), Ziele für den Fang und ob sich etwas geändert hat.
#[derive(Clone, Debug)]
struct Zug {
    griff: Griff,
    start: [f64; 2],
    ring: Vec<[f64; 2]>,
    ziele: Ziele,
    geaendert: bool,
}

/// Griffe der Grenze im Grundriss.
#[derive(Clone, Debug, Default)]
pub struct Griffe {
    hover: Option<Griff>,
    zug: Option<Zug>,
}

impl Griffe {
    /// Steht die Maus auf einem Griff oder wird gezogen?
    pub fn busy(&self) -> bool {
        self.hover.is_some() || self.zug.is_some()
    }

    pub fn zieht(&self) -> bool {
        self.zug.is_some()
    }
}

/// Griff unter dem Bildpunkt: Ecken vor Punkten vor Kanten.
pub fn treffen(
    ringe: &[Vec<[f64; 2]>],
    cam: &Camera,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
) -> Option<Griff> {
    let r = GREIF_PX * scale;
    let bild = |p: [f64; 2]| cam.project(vec3(p[0], p[1], 0.0), w, h);
    let abstand = |p: [f64; 2]| bild(p).map(|q| (q.0 - x).hypot(q.1 - y));
    let naechster = |f: &dyn Fn(usize, usize) -> Option<f64>| {
        let mut best: Option<(f64, usize, usize)> = None;
        for (i, ring) in ringe.iter().enumerate() {
            for k in 0..ring.len() {
                if let Some(d) = f(i, k).filter(|d| *d <= r) {
                    if best.is_none_or(|b| d < b.0) {
                        best = Some((d, i, k));
                    }
                }
            }
        }
        best.map(|b| (b.1, b.2))
    };
    let mitte = |i: usize, k: usize| {
        let ring = &ringe[i];
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
    };
    if let Some((i, k)) = naechster(&|i, k| abstand(ringe[i][k])) {
        return Some(Griff::Ecke(i, k));
    }
    if let Some((i, k)) = naechster(&|i, k| abstand(mitte(i, k))) {
        return Some(Griff::Plus(i, k));
    }
    naechster(&|i, k| {
        let ring = &ringe[i];
        let (a, b) = (bild(ring[k])?, bild(ring[(k + 1) % ring.len()])?);
        let e = (b.0 - a.0, b.1 - a.1);
        let l2 = e.0 * e.0 + e.1 * e.1;
        let t = if l2 < 1e-9 {
            0.0
        } else {
            (((x - a.0) * e.0 + (y - a.1) * e.1) / l2).clamp(0.0, 1.0)
        };
        let d = (a.0 + e.0 * t - x).hypot(a.1 + e.1 * t - y);
        Some(d * 2.0)
    })
    .map(|(i, k)| Griff::Kante(i, k))
}

/// Der Ring beim Zug an `griff` mit dem Bodenpunkt `p` (mm): die Ecke auf
/// `p`, die Kante um ihren Abstand quer, auf [`STEP`] gerundet.
fn gezogen(griff: Griff, start: [f64; 2], ring: &[[f64; 2]], p: [f64; 2]) -> Option<Vec<[f64; 2]>> {
    match griff {
        Griff::Ecke(_, k) => {
            let mut r = ring.to_vec();
            *r.get_mut(k)? = p;
            Some(r)
        }
        Griff::Kante(_, k) => {
            let n = ring.len();
            let (a, b) = (ring[k], ring[(k + 1) % n]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let l = dx.hypot(dy);
            if l < 1e-9 {
                return None;
            }
            let s = ((p[0] - start[0]) * -dy + (p[1] - start[1]) * dx) / l;
            let s = (s / STEP).round() * STEP;
            sk_model::offset_ring_edge(ring, k, s)
        }
        // eingefügt wird beim Greifen, gezogen als Ecke
        Griff::Plus(..) => None,
    }
}

/// Bodenpunkt unter dem Bildpunkt: am Fangziel bis [`fang::FANG_PX`], sonst
/// im Raster; mit Strg nur im Raster.
#[allow(clippy::too_many_arguments)]
fn boden(
    cam: &Camera,
    ziele: &Ziele,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
    mods: Modifiers,
) -> Option<[f64; 2]> {
    let g = cam.ground_point(x, y, w, h)?;
    if !mods.ctrl {
        let tol = cam
            .ground_point(x + fang::FANG_PX * scale, y, w, h)
            .map_or(0.0, |n| (n.x - g.x).hypot(n.y - g.y));
        if let Some((q, _)) = ziele.fangen([g.x, g.y], tol) {
            return Some(q);
        }
    }
    let r = |v: f64| (v / STEP).round() * STEP + 0.0;
    Some([r(g.x), r(g.y)])
}

/// Fläche eines Rings (m²), für die Statuszeile.
fn qm(ring: &[[f64; 2]]) -> f64 {
    sk_model::TerrainPatch::flat(ring.to_vec(), 0.0).area() / 1e6
}

/// Satz der Statuszeile zum Griff.
fn satz(g: Griff) -> &'static str {
    match g {
        Griff::Ecke(..) => ECKE,
        Griff::Plus(..) => PLUS,
        Griff::Kante(..) => KANTE,
    }
}

impl App {
    /// Sind die Griffe der Grenze an? Im Grundriss bei offener Leiste mit
    /// Modul, ohne Werkzeug, mit sichtbarem Gelände.
    fn grenz_griffe_an(&self) -> bool {
        let l = &self.platte.leiste;
        self.ui.view == ViewKind::Plan
            && l.offen
            && !l.werkzeug.aktiv
            && !self.tool.enabled
            && self.ui.frei.frei(crate::freischaltung::Modul::Gelaende)
            && !self.scene.terrain_hidden()
    }

    /// Ereignis an die Griffe der Grenze; `true`, wenn sie es nehmen.
    pub(crate) fn grenz_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        if !self.grenz_griffe_an() {
            if self.platte.griffe.zieht() {
                self.grenz_escape();
            }
            if self.platte.griffe.hover.take().is_some() {
                self.grenz_zeile(None);
            }
            return false;
        }
        match *ev {
            Event::MouseMove { x, y, mods } => {
                if self.platte.griffe.zug.is_some() {
                    self.grenz_ziehen(x, y, vw, vh, sc, mods);
                    return true;
                }
                let ringe = self.scene.model().parcel_rings();
                let g = treffen(&ringe, &self.cam, x, y, vw, vh, sc);
                if g != self.platte.griffe.hover {
                    self.platte.griffe.hover = g;
                    self.grenz_zeile(g);
                    self.redraw = true;
                }
                g.is_some()
            }
            Event::MouseLeave => {
                if self.platte.griffe.zug.is_none() && self.platte.griffe.hover.take().is_some() {
                    self.grenz_zeile(None);
                    self.redraw = true;
                }
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let ringe = self.scene.model().parcel_rings();
                let Some(g) = treffen(&ringe, &self.cam, x, y, vw, vh, sc) else {
                    return false;
                };
                let Some(start) = self.cam.ground_point(x, y, vw, vh).map(|p| [p.x, p.y]) else {
                    return false;
                };
                self.grenz_greifen(g, start, &ringe);
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                let Some(z) = self.platte.griffe.zug.take() else {
                    return false;
                };
                if z.geaendert {
                    self.scene.commit();
                    self.upload_model();
                    self.quantity.dirty = true;
                } else {
                    self.scene.rollback();
                }
                self.platte.leiste.neu = true;
                self.gelaende_sync();
                self.redraw = true;
                true
            }
            _ => false,
        }
    }

    /// Greifen: Schritt öffnen; am Punkt der Kantenmitte gleich die Ecke
    /// einfügen und als Ecke weiterziehen.
    fn grenz_greifen(&mut self, g: Griff, start: [f64; 2], ringe: &[Vec<[f64; 2]>]) {
        let (i, k) = match g {
            Griff::Ecke(i, k) | Griff::Plus(i, k) | Griff::Kante(i, k) => (i, k),
        };
        let Some(ring) = ringe.get(i) else {
            return;
        };
        let ziele = fang::ziele(&self.scene);
        let label = match g {
            Griff::Ecke(..) => LABEL_ECKE,
            Griff::Plus(..) => LABEL_EINGEFUEGT,
            Griff::Kante(..) => LABEL_KANTE,
        };
        self.scene.begin(label);
        let mut ring = ring.clone();
        let mut griff = g;
        let mut geaendert = false;
        if let Griff::Plus(..) = g {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            ring.insert(k + 1, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
            geaendert = self.scene.grenze_ziehen(i, &ring);
            griff = Griff::Ecke(i, k + 1);
            self.upload_model();
        }
        self.platte.griffe.hover = Some(griff);
        self.platte.griffe.zug = Some(Zug {
            griff,
            start,
            ring,
            ziele,
            geaendert,
        });
        self.redraw = true;
    }

    /// Ziehen: der Ring aus dem Stand beim Greifen, live im Modell; die
    /// Statuszeile nennt die Fläche.
    fn grenz_ziehen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64, mods: Modifiers) {
        let Some(z) = self.platte.griffe.zug.as_ref() else {
            return;
        };
        let p = match z.griff {
            Griff::Ecke(..) => boden(&self.cam, &z.ziele, x, y, vw, vh, sc, mods),
            _ => self.cam.ground_point(x, y, vw, vh).map(|p| [p.x, p.y]),
        };
        let Some(r) = p.and_then(|p| gezogen(z.griff, z.start, &z.ring, p)) else {
            return;
        };
        let (Griff::Ecke(i, _) | Griff::Kante(i, _) | Griff::Plus(i, _)) = z.griff;
        // eine Schleife zeigt die Zeile; das Modell bleibt beim letzten
        // guten Stand
        if !sk_model::parcel_ring_ok(&r) {
            self.status(Meldung::satz(GEHT_NICHT), crate::NOTICE_TIME);
            return;
        }
        if self.scene.grenze_ziehen(i, &r) {
            if let Some(z) = self.platte.griffe.zug.as_mut() {
                z.geaendert = true;
            }
            self.upload_model();
        }
        let t = format!("Grundstück {} m²", super::de(qm(&r), 2));
        self.status(
            Meldung::mit("{}", &[&t]),
            std::time::Duration::from_secs(3600),
        );
    }

    /// Esc beim Ziehen: alles wie vorher. `true`, wenn gezogen wurde.
    pub(crate) fn grenz_escape(&mut self) -> bool {
        if self.platte.griffe.zug.take().is_none() {
            return false;
        }
        self.scene.rollback();
        self.upload_model();
        self.grenz_zeile(self.platte.griffe.hover);
        self.redraw = true;
        true
    }

    /// Entf über einer Ecke löscht sie; `true`, wenn Entf der Grenze galt.
    pub(crate) fn grenz_entf(&mut self) -> bool {
        let Some(Griff::Ecke(i, k)) = self.platte.griffe.hover.filter(|_| self.grenz_griffe_an())
        else {
            return false;
        };
        let Some(mut ring) = self.scene.model().parcel_rings().get(i).cloned() else {
            return false;
        };
        if ring.len() <= 3 || k >= ring.len() {
            self.status(Meldung::satz(DREI), crate::NOTICE_TIME);
            return true;
        }
        ring.remove(k);
        if self
            .scene
            .edit_model(LABEL_GELOESCHT, |m| m.set_parcel_ring(i, &ring))
        {
            self.upload_model();
            self.quantity.dirty = true;
            self.platte.griffe.hover = None;
            self.grenz_zeile(None);
            self.platte.leiste.neu = true;
            self.gelaende_sync();
        } else {
            self.status(Meldung::satz(GEHT_NICHT), crate::NOTICE_TIME);
        }
        self.redraw = true;
        true
    }

    /// Statuszeile zum Griff unter der Maus; ohne Griff geht ein Satz der
    /// Griffe wieder aus der Zeile.
    fn grenz_zeile(&mut self, g: Option<Griff>) {
        match g {
            Some(g) => self.status(Meldung::satz(satz(g)), std::time::Duration::from_secs(3600)),
            None => {
                let unsere = [ECKE, PLUS, KANTE];
                if self
                    .notice
                    .as_ref()
                    .is_some_and(|n| unsere.iter().any(|s| n.text == *s))
                {
                    self.notice = None;
                    self.renderer
                        .set_overlay(crate::OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                    self.redraw = true;
                }
            }
        }
    }

    /// Quadrate an den Ecken, Punkte in den Kantenmitten; der Griff unter
    /// der Maus und der gezogene in der Farbe des Ziehens, die Kante dazu
    /// als Linie.
    pub(crate) fn grenz_linien(&self, z: f64, scale: f32, theme: &Theme) -> Vec<Helper> {
        if !self.grenz_griffe_an() {
            return Vec::new();
        }
        let ringe = self.scene.model().parcel_rings();
        let heiss = self.platte.griffe.hover;
        let col = &theme.interact;
        let p = |q: [f64; 2]| [q[0] as f32, q[1] as f32, z as f32 + 2.0];
        let punkt = |q: [f64; 2], px: f32, color, round| Helper {
            a: p(q),
            b: p(q),
            color,
            width: px * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: false,
            round,
        };
        let mut out = Vec::new();
        for (i, ring) in ringe.iter().enumerate() {
            let n = ring.len();
            for k in 0..n {
                let (a, b) = (ring[k], ring[(k + 1) % n]);
                if heiss == Some(Griff::Kante(i, k)) {
                    out.push(Helper {
                        a: p(a),
                        b: p(b),
                        color: col.drag,
                        width: 3.0 * scale,
                        dash: 0.0,
                        pattern: sk_render::SOLID,
                        occlude: false,
                        round: false,
                    });
                }
                let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                let an = heiss == Some(Griff::Plus(i, k));
                out.push(punkt(
                    m,
                    PLUS_PX * if an { 1.4 } else { 1.0 },
                    if an { col.drag } else { col.guide },
                    true,
                ));
            }
            for (k, &q) in ring.iter().enumerate() {
                let an = heiss == Some(Griff::Ecke(i, k));
                out.push(punkt(
                    q,
                    ECKE_PX * if an { 1.4 } else { 1.0 },
                    if an { col.drag } else { col.select },
                    false,
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    /// Grundriss von oben über 0..20 m, 800 × 800 Bildpunkte.
    fn kamera() -> Camera {
        Camera::parallel(vec3(10000.0, 10000.0, 0.0), FRAC_PI_2, -FRAC_PI_2, 12500.0)
    }

    fn rechteck() -> Vec<Vec<[f64; 2]>> {
        vec![vec![
            [2000.0, 2000.0],
            [18000.0, 2000.0],
            [18000.0, 12000.0],
            [2000.0, 12000.0],
        ]]
    }

    fn bild(cam: &Camera, x: f64, y: f64) -> (f64, f64) {
        cam.project(vec3(x, y, 0.0), 800.0, 800.0).unwrap()
    }

    /// Ecke vor Punkt vor Kante; daneben nichts.
    #[test]
    fn griff_unter_der_maus() {
        let cam = kamera();
        let r = rechteck();
        let t = |x: f64, y: f64| {
            let (bx, by) = bild(&cam, x, y);
            treffen(&r, &cam, bx, by, 800.0, 800.0, 1.0)
        };
        assert_eq!(t(2050.0, 2050.0), Some(Griff::Ecke(0, 0)));
        assert_eq!(t(10000.0, 2050.0), Some(Griff::Plus(0, 0)));
        assert_eq!(t(18030.0, 5000.0), Some(Griff::Kante(0, 1)));
        assert_eq!(t(10000.0, 7000.0), None);
        // eine Kante greift nur halb so weit wie ein Punkt
        assert_eq!(t(18000.0 + 150.0, 5000.0), None);
    }

    /// Ein Zug in der Szene: Greifen öffnet den Schritt, jeder Stand geht
    /// live ins Modell, eine Schleife nicht; Loslassen ist ein Schritt, den
    /// Rückgängig ganz zurücknimmt.
    #[test]
    fn zug_ist_ein_schritt() {
        let mut m = sk_model::Model::with_seed(1);
        m.begin("Grenze");
        assert!(m.add_parcel(&rechteck()[0]));
        m.commit();
        let mut s = crate::scene::Scene::with_model(m);
        let r = s.model().parcel_rings()[0].clone();
        assert!(!s.grenze_ziehen(0, &r), "ohne offenen Schritt nichts");
        s.begin(LABEL_ECKE);
        for x in [19000.0, 20000.0] {
            let e = gezogen(Griff::Ecke(0, 2), [0.0, 0.0], &r, [x, 13000.0]).unwrap();
            assert!(s.grenze_ziehen(0, &e));
        }
        let schleife = gezogen(Griff::Ecke(0, 2), [0.0, 0.0], &r, [10000.0, -3000.0]).unwrap();
        assert!(!sk_model::parcel_ring_ok(&schleife));
        assert!(!s.grenze_ziehen(0, &schleife));
        s.commit();
        assert_eq!(s.model().parcel_rings()[0][2], [20000.0, 13000.0]);
        assert!(s.undo());
        assert_eq!(s.model().parcel_rings()[0], r);
    }

    /// Ecke folgt dem Punkt; Kante quer auf 1 cm, längs ändert nichts.
    #[test]
    fn ring_beim_ziehen() {
        let r = &rechteck()[0];
        let e = gezogen(Griff::Ecke(0, 2), [0.0, 0.0], r, [19000.0, 13000.0]).unwrap();
        assert_eq!(e[2], [19000.0, 13000.0]);
        assert_eq!(e[1], r[1]);
        // rechte Kante (1 → 2) zeigt nach +y, links ist −x: nach rechts
        // ziehen ist negativ und schiebt sie nach außen
        let k = gezogen(Griff::Kante(0, 1), [18000.0, 5000.0], r, [19234.0, 9000.0]).unwrap();
        assert_eq!(k[1], [19230.0, 2000.0]);
        assert_eq!(k[2], [19230.0, 12000.0]);
        assert!(gezogen(Griff::Plus(0, 1), [0.0, 0.0], r, [0.0, 0.0]).is_none());
    }
}

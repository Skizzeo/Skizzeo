//! Grenze mit Griffen ändern (I3, gelaende/analyse-interaktives-gelaende.md
//! §4.2): Ist die Leiste „Gelände“ offen (§4.0 Regel 1, J9 A), zeigt jedes
//! Flurstück im Grundriss und in 3D an den Ecken ein Quadrat und in jeder
//! Kantenmitte einen Punkt (I3b, Jörn 17.10.: in 3D auf dem Gelände). Eine
//! Ecke ziehen verschiebt sie mit Fang (Strg ohne), den Punkt ziehen fügt
//! dort eine Ecke ein und zieht sie gleich mit, ein Klick fügt sie nur ein.
//! Eine Kante quer ziehen versetzt sie parallel, die Nachbarkanten behalten
//! ihre Richtung. In 3D wird in der Ebene auf der Höhe des Griffs gezogen.
//! Ein Klick wählt eine Ecke, Esc oder ein Klick daneben wählt sie ab. Entf
//! löscht die gewählte Ecke, ohne Wahl die unter der Maus; eine Grenze
//! behält drei. Was unter der Maus liegt, sagt die
//! Statuszeile, beim Ziehen die Fläche des Grundstücks und die Längen der
//! Kanten am Griff. Gezogen wird nur im Bild; erst Loslassen schreibt die
//! Grenze ins Modell, als ein Schritt (Review B-233a: sonst bauten Netz und
//! Objektbaum bei jeder Mausbewegung neu). Esc bricht ab.

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
/// Nach dem Klick auf eine Ecke.
pub const GEWAEHLT: &str =
    "Ecke gewählt: Entf löscht sie, ziehen verschiebt sie, Esc wählt sie ab.";
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

/// Ein Zug am Griff: Bodenpunkt beim Greifen auf der Höhe `z` des Griffs
/// (in ihrer Ebene wird gezogen), der Ring dabei (nach dem Einfügen), Ziele
/// für den Fang und der Ring, den Loslassen schreibt (`None`: nichts
/// geändert).
#[derive(Clone, Debug)]
struct Zug {
    griff: Griff,
    start: [f64; 2],
    z: f64,
    ring: Vec<[f64; 2]>,
    ziele: Ziele,
    label: &'static str,
    neu: Option<Vec<[f64; 2]>>,
}

/// Griffe der Grenze im Grundriss und in 3D.
#[derive(Clone, Debug, Default)]
pub struct Griffe {
    hover: Option<Griff>,
    zug: Option<Zug>,
    /// Die gewählte Ecke: Flurstück, Ecke und wo sie lag. Ändert sich die
    /// Grenze anders (Rückgängig, eine Kante gezogen), gilt die Wahl nicht
    /// mehr.
    gewaehlt: Option<(usize, usize, [f64; 2])>,
}

impl Griffe {
    /// Steht die Maus auf einem Griff oder wird gezogen?
    pub fn busy(&self) -> bool {
        self.hover.is_some() || self.zug.is_some()
    }

    pub fn zieht(&self) -> bool {
        self.zug.is_some()
    }

    /// Die Ecke, die Entf löscht: die unter der Maus, wie es die
    /// Statuszeile dort ansagt, sonst die gewählte (Bedienbarkeit U-253a).
    fn entf_ziel(&self, ringe: &[Vec<[f64; 2]>]) -> Option<(usize, usize)> {
        match self.hover {
            Some(Griff::Ecke(i, k)) => Some((i, k)),
            _ => self.wahl(ringe),
        }
    }

    /// Die gewählte Ecke, wenn sie noch so in `ringe` liegt.
    fn wahl(&self, ringe: &[Vec<[f64; 2]>]) -> Option<(usize, usize)> {
        let (i, k, p) = self.gewaehlt?;
        (ringe.get(i)?.get(k)? == &p).then_some((i, k))
    }

    /// Nichts mehr gewählt; `true`, wenn etwas gewählt war.
    pub fn abwaehlen(&mut self) -> bool {
        self.gewaehlt.take().is_some()
    }
}

/// Griff unter dem Bildpunkt: Ecken vor Punkten vor Kanten. `hoehe` gibt
/// die Höhe der Grenze an einer Stelle (mm).
#[allow(clippy::too_many_arguments)]
pub fn treffen(
    ringe: &[Vec<[f64; 2]>],
    hoehe: &dyn Fn([f64; 2]) -> f64,
    cam: &Camera,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
) -> Option<Griff> {
    let r = GREIF_PX * scale;
    let bild = |p: [f64; 2]| cam.project(vec3(p[0], p[1], hoehe(p)), w, h);
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

/// Punkt unter dem Bildpunkt in der Ebene auf der Höhe `z`: am Fangziel
/// bis [`fang::FANG_PX`], sonst im Raster; mit Strg nur im Raster.
#[allow(clippy::too_many_arguments)]
fn boden(
    cam: &Camera,
    ziele: &Ziele,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    z: f64,
    scale: f64,
    mods: Modifiers,
) -> Option<[f64; 2]> {
    let g = cam.plane_point(x, y, w, h, z)?;
    if !mods.ctrl {
        let tol = cam
            .plane_point(x + fang::FANG_PX * scale, y, w, h, z)
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

/// Statuszeile beim Ziehen: Fläche und die Kanten, die sich ändern (an
/// der Ecke beide Nachbarkanten, an der Kante sie selbst).
fn zug_zeile(g: Griff, ring: &[[f64; 2]]) -> String {
    let n = ring.len();
    let kante = |k: usize| {
        let (a, b) = (ring[k % n], ring[(k + 1) % n]);
        super::de((b[0] - a[0]).hypot(b[1] - a[1]) / 1000.0, 2)
    };
    let f = super::de(qm(ring), 2);
    match g {
        Griff::Ecke(_, j) | Griff::Plus(_, j) => format!(
            "Grundstück {f} m², Kanten {} m und {} m",
            kante(j + n - 1),
            kante(j)
        ),
        Griff::Kante(_, k) => format!("Grundstück {f} m², Kante {} m", kante(k)),
    }
}

/// Wo ein Griff sitzt: die Ecke, die Mitte des Punkts oder der Kante.
fn ort(g: Griff, ringe: &[Vec<[f64; 2]>]) -> Option<[f64; 2]> {
    let (Griff::Ecke(i, k) | Griff::Plus(i, k) | Griff::Kante(i, k)) = g;
    let ring = ringe.get(i)?;
    let a = *ring.get(k)?;
    if let Griff::Ecke(..) = g {
        return Some(a);
    }
    let b = ring[(k + 1) % ring.len()];
    Some([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])
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
    /// Sind die Griffe der Grenze an? Im Grundriss und in 3D (I3b) bei
    /// offener Leiste mit Modul, ohne Werkzeug, mit sichtbarem Gelände.
    pub(super) fn grenz_griffe_an(&self) -> bool {
        let l = &self.platte.leiste;
        matches!(self.ui.view, ViewKind::Plan | ViewKind::Persp)
            && l.offen
            && !l.werkzeug.aktiv
            && !self.tool.enabled
            && self.ui.frei.frei(crate::freischaltung::Modul::Gelaende)
            && !self.scene.terrain_hidden()
    }

    /// Höhe der Grenze an einer Stelle (mm): in 3D auf dem Gelände wie ihre
    /// Linie dort, im Grundriss auf OK Gelände.
    fn grenz_hoehe(&self) -> impl Fn([f64; 2]) -> f64 + '_ {
        let m = self.scene.model();
        let drei_d = self.ui.view == ViewKind::Persp;
        let (b, z) = (m.level_building(), m.terrain_z());
        move |p| {
            if drei_d {
                m.terrain_z_at(b, p[0], p[1])
            } else {
                z
            }
        }
    }

    /// Griff unter dem Bildpunkt.
    fn grenz_treffen(
        &self,
        ringe: &[Vec<[f64; 2]>],
        x: f64,
        y: f64,
        vw: f64,
        vh: f64,
        sc: f64,
    ) -> Option<Griff> {
        treffen(ringe, &self.grenz_hoehe(), &self.cam, x, y, vw, vh, sc)
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
            self.platte.griffe.abwaehlen();
            return false;
        }
        match *ev {
            Event::MouseMove { x, y, mods } => {
                if self.platte.griffe.zug.is_some() {
                    self.grenz_ziehen(x, y, vw, vh, sc, mods);
                    return true;
                }
                let ringe = self.scene.model().parcel_rings();
                let g = self.grenz_treffen(&ringe, x, y, vw, vh, sc);
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
                let Some(g) = self.grenz_treffen(&ringe, x, y, vw, vh, sc) else {
                    // ein Klick daneben wählt die Ecke ab
                    if self.platte.griffe.abwaehlen() {
                        self.grenz_zeile(None);
                        self.redraw = true;
                    }
                    return false;
                };
                let Some(z) = ort(g, &ringe).map(|p| self.grenz_hoehe()(p)) else {
                    return false;
                };
                let Some(start) = self.cam.plane_point(x, y, vw, vh, z).map(|p| [p.x, p.y]) else {
                    return false;
                };
                self.grenz_greifen(g, start, z, &ringe);
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                let Some(z) = self.platte.griffe.zug.take() else {
                    return false;
                };
                let (Griff::Ecke(i, _) | Griff::Kante(i, _) | Griff::Plus(i, _)) = z.griff;
                let mut ok = true;
                if let Some(r) = &z.neu {
                    ok = self.scene.edit_model(z.label, |m| m.set_parcel_ring(i, r));
                    if ok {
                        self.upload_model();
                        self.quantity.dirty = true;
                    } else {
                        // der Ring springt zurück; die Statuszeile sagt warum (H-233b)
                        self.status(Meldung::satz(GEHT_NICHT), crate::NOTICE_TIME);
                    }
                }
                // die gezogene oder angeklickte Ecke bleibt gewählt (I3b)
                if let (true, Griff::Ecke(_, k)) = (ok, z.griff) {
                    let p = z.neu.as_ref().unwrap_or(&z.ring).get(k).copied();
                    self.platte.griffe.gewaehlt = p.map(|p| (i, k, p));
                    if z.neu.is_none() {
                        self.status(Meldung::satz(GEWAEHLT), crate::NOTICE_TIME);
                    }
                }
                self.platte.leiste.neu = true;
                self.gelaende_sync();
                self.redraw = true;
                true
            }
            _ => false,
        }
    }

    /// Greifen: am Punkt der Kantenmitte gleich die Ecke einfügen (im Bild)
    /// und als Ecke weiterziehen.
    fn grenz_greifen(&mut self, g: Griff, start: [f64; 2], z: f64, ringe: &[Vec<[f64; 2]>]) {
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
        let mut ring = ring.clone();
        let mut griff = g;
        let mut neu = None;
        if let Griff::Plus(..) = g {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            ring.insert(k + 1, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]);
            neu = Some(ring.clone());
            griff = Griff::Ecke(i, k + 1);
        }
        self.platte.griffe.hover = Some(griff);
        self.platte.griffe.zug = Some(Zug {
            griff,
            start,
            z,
            ring,
            ziele,
            label,
            neu,
        });
        self.redraw = true;
    }

    /// Ziehen: der Ring aus dem Stand beim Greifen, nur im Bild; die
    /// Statuszeile nennt die Fläche und die Kanten am Griff.
    fn grenz_ziehen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64, mods: Modifiers) {
        let Some(z) = self.platte.griffe.zug.as_ref() else {
            return;
        };
        let p = match z.griff {
            Griff::Ecke(..) => boden(&self.cam, &z.ziele, x, y, vw, vh, z.z, sc, mods),
            _ => self.cam.plane_point(x, y, vw, vh, z.z).map(|p| [p.x, p.y]),
        };
        let Some(r) = p.and_then(|p| gezogen(z.griff, z.start, &z.ring, p)) else {
            return;
        };
        let griff = z.griff;
        // eine Schleife zeigt die Zeile; das Bild bleibt beim letzten guten
        // Stand
        if !sk_model::parcel_ring_ok(&r) {
            self.status(Meldung::satz(GEHT_NICHT), crate::NOTICE_TIME);
            return;
        }
        let t = zug_zeile(griff, &r);
        if let Some(z) = self.platte.griffe.zug.as_mut() {
            z.neu = (r != z.ring || z.label == LABEL_EINGEFUEGT).then_some(r);
        }
        self.redraw = true;
        self.status(
            Meldung::mit("{}", &[&t]),
            std::time::Duration::from_secs(3600),
        );
    }

    /// Esc beim Ziehen: alles wie vorher; sonst geht die Wahl der Ecke.
    /// `true`, wenn gezogen wurde oder eine Ecke gewählt war.
    pub(crate) fn grenz_escape(&mut self) -> bool {
        if self.platte.griffe.zug.take().is_none() {
            if !self.platte.griffe.abwaehlen() {
                return false;
            }
            self.grenz_zeile(None);
            self.redraw = true;
            return true;
        }
        self.grenz_zeile(self.platte.griffe.hover);
        self.redraw = true;
        true
    }

    /// Entf löscht die Ecke unter der Maus, sonst die gewählte
    /// ([`Griffe::entf_ziel`]); `true`, wenn Entf der Grenze galt.
    pub(crate) fn grenz_entf(&mut self) -> bool {
        if !self.grenz_griffe_an() {
            return false;
        }
        let ringe = self.scene.model().parcel_rings();
        let Some((i, k)) = self.platte.griffe.entf_ziel(&ringe) else {
            return false;
        };
        let Some(mut ring) = ringe.get(i).cloned() else {
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
            self.platte.griffe.abwaehlen();
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
                let unsere = [ECKE, PLUS, KANTE, GEWAEHLT];
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

    /// Quadrate an den Ecken, Punkte in den Kantenmitten, in 3D auf dem
    /// Gelände; der Griff unter der Maus, die gewählte Ecke und der
    /// gezogene Griff in der Farbe des Ziehens, die Kante dazu als Linie.
    /// Beim Ziehen steht der gezogene Ring an seiner neuen Stelle, ganz als
    /// Linie (das Modell folgt erst beim Loslassen).
    pub(crate) fn grenz_linien(&self, scale: f32, theme: &Theme) -> Vec<Helper> {
        if !self.grenz_griffe_an() {
            return Vec::new();
        }
        let mut ringe = self.scene.model().parcel_rings();
        let wahl = self.platte.griffe.wahl(&ringe);
        let zug = self.platte.griffe.zug.as_ref();
        let gezogen = zug.map(|z| {
            let (Griff::Ecke(i, _) | Griff::Kante(i, _) | Griff::Plus(i, _)) = z.griff;
            (i, z.neu.as_ref().unwrap_or(&z.ring).clone())
        });
        if let Some((i, r)) = &gezogen {
            if let Some(alt) = ringe.get_mut(*i) {
                *alt = r.clone();
            }
        }
        let heiss = self.platte.griffe.hover;
        let col = &theme.interact;
        let hoehe = self.grenz_hoehe();
        let p = |q: [f64; 2]| [q[0] as f32, q[1] as f32, hoehe(q) as f32 + 2.0];
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
                if gezogen.as_ref().is_some_and(|g| g.0 == i) || heiss == Some(Griff::Kante(i, k)) {
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
                let gewaehlt = wahl == Some((i, k)) && gezogen.is_none();
                let gross = if gewaehlt {
                    1.7
                } else if an {
                    1.4
                } else {
                    1.0
                };
                out.push(punkt(
                    q,
                    ECKE_PX * gross,
                    if an || gewaehlt { col.drag } else { col.select },
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
            treffen(&r, &|_| 0.0, &cam, bx, by, 800.0, 800.0, 1.0)
        };
        assert_eq!(t(2050.0, 2050.0), Some(Griff::Ecke(0, 0)));
        assert_eq!(t(10000.0, 2050.0), Some(Griff::Plus(0, 0)));
        assert_eq!(t(18030.0, 5000.0), Some(Griff::Kante(0, 1)));
        assert_eq!(t(10000.0, 7000.0), None);
        // eine Kante greift nur halb so weit wie ein Punkt
        assert_eq!(t(18000.0 + 150.0, 5000.0), None);
    }

    /// In 3D liegen die Griffe auf der Höhe der Grenze (I3b): Die Ecke
    /// greift, wo sie auf 1,50 m im Bild steht, nicht auf 0. Gezogen wird in
    /// der Ebene auf ihrer Höhe, der Punkt unter der Maus bleibt unter ihr.
    #[test]
    fn griffe_in_3d_auf_der_hoehe() {
        let cam = Camera::looking_at(
            vec3(10000.0, -20000.0, 15000.0),
            vec3(10000.0, 7000.0, 0.0),
            45.0,
        );
        let r = rechteck();
        let hoch = |_: [f64; 2]| 1500.0;
        let an = |p: [f64; 2], z: f64| cam.project(vec3(p[0], p[1], z), 800.0, 800.0).unwrap();
        let t = |q: (f64, f64)| treffen(&r, &hoch, &cam, q.0, q.1, 800.0, 800.0, 1.0);
        let oben = an([2000.0, 2000.0], 1500.0);
        let unten = an([2000.0, 2000.0], 0.0);
        assert!((oben.0 - unten.0).hypot(oben.1 - unten.1) > 3.0 * GREIF_PX);
        assert_eq!(t(oben), Some(Griff::Ecke(0, 0)));
        assert_ne!(t(unten), Some(Griff::Ecke(0, 0)));
        assert_eq!(t(an([10000.0, 12000.0], 1500.0)), Some(Griff::Plus(0, 2)));
        let strg = Modifiers {
            ctrl: true,
            shift: false,
            alt: false,
        };
        let q = an([5432.0, 7654.0], 1500.0);
        let b = boden(
            &cam,
            &Ziele::default(),
            q.0,
            q.1,
            800.0,
            800.0,
            1500.0,
            1.0,
            strg,
        );
        assert_eq!(b, Some([5430.0, 7650.0]));
        // auf 0 genommen läge der Punkt weit daneben
        let b0 = boden(
            &cam,
            &Ziele::default(),
            q.0,
            q.1,
            800.0,
            800.0,
            0.0,
            1.0,
            strg,
        )
        .unwrap();
        assert!((b0[1] - 7650.0).abs() > 1000.0, "{b0:?}");
    }

    /// Die gewählte Ecke gilt, solange sie dort liegt, wo sie gewählt wurde.
    #[test]
    fn wahl_folgt_der_grenze() {
        let mut r = rechteck();
        let mut g = Griffe {
            gewaehlt: Some((0, 2, r[0][2])),
            ..Default::default()
        };
        assert_eq!(g.wahl(&r), Some((0, 2)));
        // Entf gilt der Ecke unter der Maus, sonst der gewählten (U-253a)
        assert_eq!(g.entf_ziel(&r), Some((0, 2)));
        g.hover = Some(Griff::Ecke(0, 1));
        assert_eq!(g.entf_ziel(&r), Some((0, 1)));
        g.hover = Some(Griff::Kante(0, 1));
        assert_eq!(g.entf_ziel(&r), Some((0, 2)));
        g.hover = None;
        r[0][2] = [19000.0, 13000.0];
        assert_eq!(g.wahl(&r), None, "anders geändert");
        assert_eq!(g.wahl(&[]), None);
        assert!(g.abwaehlen() && !g.abwaehlen());
        assert_eq!(
            ort(Griff::Kante(0, 1), &rechteck()),
            Some([18000.0, 7000.0])
        );
        assert_eq!(ort(Griff::Ecke(0, 3), &rechteck()), Some([2000.0, 12000.0]));
    }

    /// Ein Zug ändert das Modell nicht (Review B-233a), nur das Bild;
    /// eine Schleife bleibt beim letzten guten Stand. Loslassen schreibt den
    /// letzten Ring als einen Schritt, den Rückgängig ganz zurücknimmt.
    #[test]
    fn zug_ist_ein_schritt() {
        let mut m = sk_model::Model::with_seed(1);
        m.begin("Grenze");
        assert!(m.add_parcel(&rechteck()[0]));
        m.commit();
        let mut s = crate::scene::Scene::with_model(m);
        let r = s.model().parcel_rings()[0].clone();
        let rev = s.model().revision();
        let mut letzter = None;
        for p in [[19000.0, 13000.0], [20000.0, 13000.0], [10000.0, -3000.0]] {
            let e = gezogen(Griff::Ecke(0, 2), [0.0, 0.0], &r, p).unwrap();
            if sk_model::parcel_ring_ok(&e) {
                letzter = Some(e);
            }
        }
        assert_eq!(s.model().revision(), rev, "beim Ziehen bleibt das Modell");
        let e = letzter.unwrap();
        assert_eq!(e[2], [20000.0, 13000.0], "die Schleife zählt nicht");
        assert!(s.edit_model(LABEL_ECKE, |m| m.set_parcel_ring(0, &e)));
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
        assert_eq!(
            zug_zeile(Griff::Ecke(0, 0), r),
            "Grundstück 160,00 m², Kanten 10,00 m und 16,00 m"
        );
        assert_eq!(
            zug_zeile(Griff::Kante(0, 1), &k),
            "Grundstück 172,30 m², Kante 10,00 m"
        );
    }
}

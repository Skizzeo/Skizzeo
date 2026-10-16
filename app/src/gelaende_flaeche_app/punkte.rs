//! Höhenpunkte im Grundriss (I4, gelaende/analyse-interaktives-gelaende.md
//! §4.3, §4.3a): Bei offener Leiste „Gelände“ (§4.0 Regel 1) greift die
//! Maus die Punkte des Geländes. Ein Klick wählt einen Punkt, Strg+Klick
//! nimmt weitere dazu oder heraus. Einen Punkt ziehen verschiebt ihn und
//! die übrigen gewählten in der Lage, mit Fang an Haus, Grenze und Flächen
//! (Alt ohne, §5 J10). Am gewählten Punkt steht ein Höhengriff: nach oben
//! ziehen hebt, nach unten senkt, ein Bildpunkt 1 cm, mit Umschalt 10 cm.
//! Strg beim Ziehen oder Loslassen macht eine Kopie (immer ein eigener
//! Punkt), Strg+D kopiert die gewählten um 1,00 m versetzt. Entf löscht die
//! gewählten. Ein Doppelklick aufs Gelände setzt einen Punkt in der Höhe,
//! die das Gelände dort hat; so springt beim Setzen nichts.
//!
//! Gezogen wird ohne das Modell zu ändern; Loslassen ist ein Schritt. Esc
//! bricht den Zug ab, ohne Zug hebt es die Wahl auf.
//!
//! In 3D (I4c) geht dasselbe, solange das Hilfsnetz die Punkte als Nadeln
//! zeigt: Am ersten gewählten Punkt steht ein senkrechter Pfeil mit Kugel,
//! die Maus zählt dann auf der senkrechten Ebene durch den Punkt, die zur
//! Kamera schaut; der Punkt hebt sich also mit der Maus. Ein Quadrat am Fuß
//! zeigt, dass der Punkt selbst waagerecht gleitet, in seiner Höhe.

use super::fang::{self, Ziele};
use super::STEP;
use crate::camera::Camera;
use crate::meldung::Meldung;
use crate::ui::ViewKind;
use crate::App;
use sk_math::{vec3, Vec3};
use sk_model::TerrainSpot;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;
use std::time::{Duration, Instant};

/// So nah (Bildpunkte) greift die Maus einen Punkt oder den Höhengriff.
const GREIF_PX: f64 = 7.0;
/// Größe der Punkte (Bildpunkte bei 96 dpi).
const PUNKT_PX: f32 = 7.0;
/// Der Höhengriff sitzt so weit rechts über dem Punkt (Bildpunkte).
const GRIFF_DX: f64 = 18.0;
const GRIFF_DY: f64 = -18.0;
/// Länge des Pfeils am Höhengriff (Bildpunkte).
const PFEIL_PX: f64 = 14.0;
/// Höhe je Bildpunkt (mm), mit Umschalt zehnmal so viel.
const MM_JE_PX: f64 = 10.0;
/// Doppelklick: so schnell und so nah.
const DOPPEL: Duration = Duration::from_millis(450);
const DOPPEL_PX: f64 = 5.0;
/// Strg+D versetzt um so viel (mm).
const VERSATZ: f64 = 1000.0;
/// In 3D: Länge des Höhenpfeils und halbe Seite des Quadrats am Fuß
/// (Bildpunkte).
const PFEIL_3D_PX: f64 = 56.0;
const QUADRAT_PX: f64 = 10.0;

/// Statuszeile (§4.0 Regel 2).
pub const PUNKT: &str =
    "Höhenpunkt {}: {}. Klicken wählt, ziehen verschiebt (Strg kopiert, Alt ohne Fang).";
pub const HOEHE: &str = "Höhe ziehen: nach oben hebt, 1 Bildpunkt 1 cm, mit Umschalt 10 cm.";
/// Messpunkte des Vermessers tragen ein Schloss (I5, §5 J2 C).
pub const MESSPUNKT: &str =
    "Punkte des Vermessers sind gesperrt: „Schloss offen“ in der Leiste gibt sie \
     frei, Strg ziehen setzt eine eigene Kopie, Entf nimmt sie aus dem Gelände.";
pub const DREI: &str = "Das Gelände behält mindestens drei Punkte.";
pub const BELEGT: &str = "Dort liegt schon ein Höhenpunkt; nichts geändert.";
/// Schritte im Rückgängig (§4.0 Regel 5).
pub const LABEL_GESETZT: &str = "Höhenpunkt gesetzt";
pub const LABEL_LAGE: &str = "Höhenpunkte verschoben";
pub const LABEL_HOEHE: &str = "Höhe geändert";
pub const LABEL_KOPIE: &str = "Höhenpunkte kopiert";
pub const LABEL_GELOESCHT: &str = "Höhenpunkte gelöscht";
pub const LABEL_GEMESSEN: &str = "Messpunkte wie gemessen";

/// Was die Maus greift.
#[derive(Clone, Debug, PartialEq)]
pub enum Ziel {
    /// Ein Punkt (Nummer).
    Punkt(String),
    /// Der Höhengriff der Wahl.
    Hoehe,
}

/// Ein Zug: in der Lage oder in der Höhe.
#[derive(Clone, Debug)]
struct Zug {
    hoehe: bool,
    /// Bildpunkt und Bodenpunkt beim Greifen.
    px: (f64, f64),
    start: [f64; 2],
    /// Der gegriffene Punkt (Lage) für den Fang.
    griff: [f64; 2],
    /// Die gezogenen Punkte: Index, Lage und Höhe beim Greifen.
    alt: Vec<(usize, [f64; 3])>,
    /// Lage und Höhe jetzt.
    neu: Vec<[f64; 3]>,
    ziele: Ziele,
    kopie: bool,
    /// Wurde wirklich gezogen (mehr als ein paar Bildpunkte)?
    bewegt: bool,
    /// Unter den gezogenen ist ein Punkt des Vermessers: nur kopieren.
    fest: bool,
    /// Höhe der waagerechten Ebene, auf der die Maus die Lage zählt: im
    /// Grundriss 0, in 3D die des gegriffenen Punkts.
    ebene: f64,
    /// In 3D am Höhenpfeil: Punkt und Normale der senkrechten Ebene zur
    /// Kamera und die Höhe der Maus darauf beim Greifen.
    senkrecht: Option<(Vec3, Vec3, f64)>,
    /// Mit Strg auf einen schon gewählten Punkt geklickt: ohne Zug geht
    /// er aus der Wahl.
    raus: bool,
    nr: String,
}

/// Höhenpunkte im Grundriss.
#[derive(Clone, Debug, Default)]
pub struct Punkte {
    /// Die gewählten Punkte (Nummern).
    pub(crate) gewaehlt: Vec<String>,
    hover: Option<Ziel>,
    zug: Option<Zug>,
    /// Der letzte Klick ins Leere, für den Doppelklick.
    klick: Option<(Instant, f64, f64)>,
    /// Punkte zum Stand des Modells.
    merker: Option<(u64, Spots)>,
}

/// Die Punkte mit Nummer, geteilt bis zur nächsten Änderung.
type Spots = std::sync::Arc<Vec<(String, TerrainSpot)>>;

impl Punkte {
    pub fn busy(&self) -> bool {
        self.hover.is_some() || self.zug.is_some()
    }

    pub fn zieht(&self) -> bool {
        self.zug.is_some()
    }
}

/// Der Punkt unter dem Bildpunkt (der nächste bis [`GREIF_PX`]); `z` ist
/// die Höhe der Zeichenebene im Grundriss, `None` in 3D (jeder Punkt in
/// seiner Höhe).
fn treffen<'a>(
    spots: impl IntoIterator<Item = &'a (String, TerrainSpot)>,
    cam: &Camera,
    z: Option<f64>,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<&'a (String, TerrainSpot)> {
    let r = GREIF_PX * scale;
    let mut best: Option<(f64, &(String, TerrainSpot))> = None;
    for s in spots {
        let zz = z.unwrap_or(s.1.p[2]);
        let Some(q) = cam.project(vec3(s.1.p[0], s.1.p[1], zz), w, h) else {
            continue;
        };
        let d = (q.0 - x).hypot(q.1 - y);
        if d <= r && best.is_none_or(|b| d < b.0) {
            best = Some((d, s));
        }
    }
    best.map(|b| b.1)
}

/// Lage beim Ziehen: der gegriffene Punkt am Fangziel (Alt ohne), sonst im
/// Raster; alle gewählten um dasselbe.
fn lage(z: &Zug, p: [f64; 2], tol: f64, mods: Modifiers) -> [f64; 2] {
    let ziel = [
        z.griff[0] + p[0] - z.start[0],
        z.griff[1] + p[1] - z.start[1],
    ];
    let q = (!mods.alt)
        .then(|| z.ziele.fangen(ziel, tol).map(|f| f.0))
        .flatten()
        .unwrap_or_else(|| ziel.map(|v| (v / STEP).round() * STEP + 0.0));
    [q[0] - z.griff[0], q[1] - z.griff[1]]
}

/// Höhe beim Ziehen am Griff: Bildpunkte nach oben, auf cm (mit Umschalt
/// dm) gerundet.
fn hub(dy_px: f64, scale: f64, shift: bool) -> f64 {
    let je = if shift { MM_JE_PX * 10.0 } else { MM_JE_PX };
    let d = -dy_px / scale.max(1e-9) * je;
    (d / je).round() * je + 0.0
}

/// Höhe des Strahls `o + t·d` auf der senkrechten Ebene durch `p` mit der
/// (waagerechten) Normale `n`; `None`, wenn er sie nicht vor sich trifft.
fn senkrecht_z(o: Vec3, d: Vec3, p: Vec3, n: Vec3) -> Option<f64> {
    let dn = d.dot(n);
    if dn.abs() < 1e-6 {
        return None;
    }
    let t = (p - o).dot(n) / dn;
    (t > 0.0).then(|| (o + d * t).z)
}

/// In 3D gezogene Höhe auf cm (mit Umschalt dm) gerundet.
fn hub_3d(dz: f64, shift: bool) -> f64 {
    let je = if shift { MM_JE_PX * 10.0 } else { MM_JE_PX };
    (dz / je).round() * je + 0.0
}

/// Abstand eines Bildpunkts von der Strecke `a`–`b` im Bild.
fn abstand(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

/// Weltlänge (mm) eines Bildpunkts quer zur Blickrichtung am Punkt `a`.
fn mm_je_px(cam: &Camera, a: Vec3, vw: f64, vh: f64) -> Option<f64> {
    let q0 = cam.project(a, vw, vh)?;
    let q1 = cam.project(a + cam.right() * 1000.0, vw, vh)?;
    let px = (q1.0 - q0.0).hypot(q1.1 - q0.1);
    (px > 1e-9).then(|| 1000.0 / px)
}

/// In 3D die Spitze des Höhenpfeils über dem Punkt `a`.
fn pfeil_spitze(cam: &Camera, a: Vec3, vw: f64, vh: f64, sc: f64) -> Option<Vec3> {
    let l = mm_je_px(cam, a, vw, vh)? * PFEIL_3D_PX * sc;
    Some(a + vec3(0.0, 0.0, l))
}

/// In 3D am Punkt `a` der senkrechte Pfeil mit Kugel (Höhe, Farbe `.0`)
/// und das waagerechte Quadrat am Fuß (Lage, `.1`); `view`: Breite, Höhe
/// und Maßstab des Bildes.
pub(crate) fn gizmo_linien(
    cam: &Camera,
    a: Vec3,
    (vw, vh, sc): (f64, f64, f64),
    (pfeil, fuss): ([f32; 4], [f32; 4]),
    scale: f32,
) -> Vec<Helper> {
    let (Some(b), Some(mm)) = (pfeil_spitze(cam, a, vw, vh, sc), mm_je_px(cam, a, vw, vh)) else {
        return Vec::new();
    };
    let f = |v: Vec3| [v.x as f32, v.y as f32, v.z as f32];
    let strich = |a: Vec3, b: Vec3, color, w: f32, round| Helper {
        a: f(a),
        b: f(b),
        color,
        width: w * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round,
    };
    let px = mm * sc;
    let quer = cam.right() * (px * 4.0);
    let unter = b - vec3(0.0, 0.0, px * 8.0);
    let mut out = vec![
        strich(a, b, pfeil, 2.0, false),
        strich(b, unter + quer, pfeil, 2.0, false),
        strich(b, unter - quer, pfeil, 2.0, false),
        strich(b, b, pfeil, 9.0, true),
    ];
    let h = px * QUADRAT_PX;
    let e = [
        vec3(-h, -h, 0.0),
        vec3(h, -h, 0.0),
        vec3(h, h, 0.0),
        vec3(-h, h, 0.0),
    ];
    for i in 0..4 {
        out.push(strich(a + e[i], a + e[(i + 1) % 4], fuss, 1.5, false));
    }
    out
}

/// Meter mit Komma und zwei Stellen, mit Vorzeichen.
fn m_vz(mm: f64) -> String {
    let v = mm / 1000.0;
    let t = super::de(v.abs(), 2);
    if v < -5e-4 {
        format!("−{t}")
    } else {
        format!("+{t}")
    }
}

impl App {
    /// Sind die Höhenpunkte greifbar? Im Grundriss wie die Griffe der
    /// Grenze, in 3D, solange das Hilfsnetz sie zeigt (I4c).
    pub(super) fn punkte_an(&self) -> bool {
        if self.drei_d() {
            return self.netz_an() && !self.platte.leiste.werkzeug.aktiv && !self.tool.enabled;
        }
        self.grenz_griffe_an()
    }

    fn drei_d(&self) -> bool {
        self.ui.view == ViewKind::Persp
    }

    /// In 3D der Höhenpfeil am ersten gewählten Punkt: Fuß und Spitze.
    fn pfeil_3d(&self, vw: f64, vh: f64, sc: f64) -> Option<(Vec3, Vec3)> {
        let erster = self.platte.punkte.gewaehlt.first()?;
        let spots = self.spots();
        let s = spots.iter().find(|s| s.0 == *erster)?;
        let a = vec3(s.1.p[0], s.1.p[1], s.1.p[2]);
        Some((a, pfeil_spitze(&self.cam, a, vw, vh, sc)?))
    }

    /// In 3D die senkrechte Ebene durch den Punkt `p`, die zur Kamera
    /// schaut, und die Höhe der Maus darauf. Blickt die Kamera senkrecht
    /// hinab, zählen die Bildpunkte wie im Grundriss (`None`).
    fn senkrechte(
        &self,
        x: f64,
        y: f64,
        vw: f64,
        vh: f64,
        p: [f64; 3],
    ) -> Option<(Vec3, Vec3, f64)> {
        let f = self.cam.forward();
        let n = vec3(f.x, f.y, 0.0);
        if n.length() < 0.05 {
            return None;
        }
        let (n, p) = (n.normalized(), vec3(p[0], p[1], p[2]));
        let (o, d) = self.cam.ray(x, y, vw, vh);
        Some((p, n, senkrecht_z(o, d, p, n)?))
    }

    /// Die Punkte im Gelände mit Nummer: gemerkt, wenn der Stand des
    /// Modells passt, sonst neu.
    pub(super) fn spots(&self) -> Spots {
        let m = self.scene.model();
        if let Some((r, s)) = &self.platte.punkte.merker {
            if *r == m.revision() {
                return s.clone();
            }
        }
        let nr = |i: usize| {
            m.terrain()
                .and_then(|t| t.points.get(i))
                .map_or_else(String::new, |p| p.nr.clone())
        };
        std::sync::Arc::new(
            m.terrain_spots()
                .into_iter()
                .map(|s| (nr(s.i), s))
                .collect(),
        )
    }

    /// Die Punkte zum jetzigen Stand merken.
    fn spots_merken(&mut self) {
        let rev = self.scene.model().revision();
        if self
            .platte
            .punkte
            .merker
            .as_ref()
            .is_none_or(|m| m.0 != rev)
        {
            let s = self.spots();
            self.platte.punkte.merker = Some((rev, s));
        }
    }

    /// Höhe für die Zeile: über NHN, wenn das Gelände vermessen ist, dazu
    /// über ±0,00.
    fn punkt_hoehe(&self, z: f64) -> String {
        let m = self.scene.model();
        match (m.terrain_nhn(), m.terrain().is_some_and(|t| !t.assumed())) {
            (Some(h0), true) => {
                format!("{} m ü. NHN ({} m)", super::de(h0 + z / 1000.0, 2), m_vz(z))
            }
            _ => format!("{} m über ±0,00", m_vz(z)),
        }
    }

    /// Bildpunkt des Höhengriffs am ersten gewählten Punkt (in 3D die
    /// Kugel an der Spitze des Pfeils).
    fn hoehen_griff(&self, vw: f64, vh: f64, sc: f64) -> Option<(f64, f64)> {
        if self.drei_d() {
            let (_, b) = self.pfeil_3d(vw, vh, sc)?;
            return self.cam.project(b, vw, vh);
        }
        let erster = self.platte.punkte.gewaehlt.first()?.clone();
        let spots = self.spots();
        let s = spots.iter().find(|s| s.0 == erster)?;
        let z = self.scene.model().terrain_z();
        let q = self.cam.project(vec3(s.1.p[0], s.1.p[1], z), vw, vh)?;
        Some((q.0 + GRIFF_DX * sc, q.1 + GRIFF_DY * sc))
    }

    /// Ereignis an die Höhenpunkte; `true`, wenn sie es nehmen.
    pub(crate) fn punkte_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        if !self.punkte_an() || self.platte.griffe.zieht() {
            if self.platte.punkte.zug.take().is_some() || self.platte.punkte.hover.take().is_some()
            {
                self.punkte_zeile(None);
                self.redraw = true;
            }
            return false;
        }
        self.spots_merken();
        match *ev {
            Event::MouseMove { x, y, mods } => {
                if self.platte.punkte.zug.is_some() {
                    self.punkte_ziehen(x, y, vw, vh, sc, mods);
                    return true;
                }
                let ziel = self.punkte_ziel(x, y, vw, vh, sc);
                if ziel != self.platte.punkte.hover {
                    self.platte.punkte.hover = ziel.clone();
                    self.punkte_zeile(ziel.as_ref());
                    self.redraw = true;
                }
                ziel.is_some()
            }
            Event::MouseLeave => {
                if self.platte.punkte.zug.is_none() && self.platte.punkte.hover.take().is_some() {
                    self.punkte_zeile(None);
                    self.redraw = true;
                }
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                let ziel = self.punkte_ziel(x, y, vw, vh, sc);
                let out = match ziel {
                    Some(z) => {
                        self.platte.punkte.klick = None;
                        self.punkte_greifen(z, x, y, vw, vh, mods)
                    }
                    None => self.punkte_ins_leere(x, y, vw, vh),
                };
                // die Leiste zeigt das Schloss der Wahl (I5)
                self.gelaende_sync();
                out
            }
            Event::MouseUp {
                button: MouseButton::Left,
                mods,
                ..
            } => {
                let Some(z) = self.platte.punkte.zug.take() else {
                    return false;
                };
                self.punkte_loslassen(z, mods);
                self.gelaende_sync();
                true
            }
            _ => false,
        }
    }

    /// Was unter dem Bildpunkt liegt: der Höhengriff vor den Punkten.
    fn punkte_ziel(&self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) -> Option<Ziel> {
        if let Some(g) = self.hoehen_griff(vw, vh, sc) {
            if (g.0 - x).hypot(g.1 - y) <= GREIF_PX * sc * 1.5 {
                return Some(Ziel::Hoehe);
            }
        }
        let spots = self.spots();
        if !self.drei_d() {
            let z = self.scene.model().terrain_z();
            return treffen(spots.iter(), &self.cam, Some(z), (x, y), vw, vh, sc)
                .map(|s| Ziel::Punkt(s.0.clone()));
        }
        // in 3D greift auch der Schaft des Pfeils, nicht aber sein Fuß: der
        // ist der Punkt selbst
        if let Some((a, b)) = self.pfeil_3d(vw, vh, sc) {
            if let (Some(qa), Some(qb)) = (self.cam.project(a, vw, vh), self.cam.project(b, vw, vh))
            {
                let r = GREIF_PX * sc;
                if abstand((x, y), qa, qb) <= r * 0.7 && (qa.0 - x).hypot(qa.1 - y) > r {
                    return Some(Ziel::Hoehe);
                }
            }
        }
        treffen(self.nadeln(&spots), &self.cam, None, (x, y), vw, vh, sc)
            .map(|s| Ziel::Punkt(s.0.clone()))
    }

    /// Klick neben die Punkte: die Wahl geht; der zweite schnell danach
    /// setzt einen Punkt. `true`, wenn er gesetzt ist.
    fn punkte_ins_leere(&mut self, x: f64, y: f64, vw: f64, vh: f64) -> bool {
        let jetzt = Instant::now();
        let doppel = self.platte.punkte.klick.is_some_and(|(t, px, py)| {
            jetzt.duration_since(t) <= DOPPEL && (px - x).hypot(py - y) <= DOPPEL_PX
        });
        if !self.platte.punkte.gewaehlt.is_empty() {
            self.platte.punkte.gewaehlt.clear();
            self.redraw = true;
        }
        if !doppel {
            self.platte.punkte.klick = Some((jetzt, x, y));
            return false;
        }
        self.platte.punkte.klick = None;
        let Some((px, py)) = self.boden_unter(x, y, vw, vh) else {
            return false;
        };
        let m = self.scene.model();
        // unter dem Haus ist kein Gelände
        let im_haus = m
            .terrain_patches(0.0)
            .iter()
            .any(|(b, _)| (b[0]..=b[2]).contains(&px) && (b[1]..=b[3]).contains(&py));
        if im_haus {
            return false;
        }
        let z = (m.terrain_z_at(m.level_building(), px, py) / 10.0).round() * 10.0;
        let mut neu = Vec::new();
        let ok = self.scene.edit_model(LABEL_GESETZT, |m| {
            neu = m.add_terrain_points(&[[px, py, z]]);
            !neu.is_empty()
        });
        if ok {
            let nr = self
                .scene
                .model()
                .terrain()
                .and_then(|t| t.points.get(neu[0]))
                .map(|p| p.nr.clone());
            self.platte.punkte.gewaehlt = nr.into_iter().collect();
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
        }
        true
    }

    /// Die Stelle im Gelände unter dem Bildpunkt, im Raster. In 3D trifft
    /// der Strahl das Gelände (die Höhe in ein paar Schritten genähert) und
    /// nicht vorher ein Bauteil.
    fn boden_unter(&self, x: f64, y: f64, vw: f64, vh: f64) -> Option<(f64, f64)> {
        let r = |v: f64| (v / STEP).round() * STEP + 0.0;
        if !self.drei_d() {
            let g = self.cam.ground_point(x, y, vw, vh)?;
            return Some((r(g.x), r(g.y)));
        }
        let m = self.scene.model();
        let lvl = m.level_building();
        let mut g = self.cam.plane_point(x, y, vw, vh, m.terrain_z())?;
        for _ in 0..6 {
            let z = m.terrain_z_at(lvl, g.x, g.y);
            let n = self.cam.plane_point(x, y, vw, vh, z)?;
            let fertig = (n.x - g.x).hypot(n.y - g.y) < 1.0;
            g = n;
            if fertig {
                break;
            }
        }
        let (o, d) = self.cam.ray(x, y, vw, vh);
        let weit = (g - o).length() / d.length().max(1e-12);
        if self.scene.raycast(o, d).is_some_and(|(t, _)| t < weit) {
            return None;
        }
        Some((r(g.x), r(g.y)))
    }

    /// Greifen: am Punkt wählen (Strg: dazu) und den Zug vorbereiten; am
    /// Höhengriff die Höhe der Wahl.
    fn punkte_greifen(
        &mut self,
        ziel: Ziel,
        x: f64,
        y: f64,
        vw: f64,
        vh: f64,
        mods: Modifiers,
    ) -> bool {
        let (hoehe, nr, raus) = match ziel {
            Ziel::Hoehe => (true, self.platte.punkte.gewaehlt[0].clone(), false),
            Ziel::Punkt(nr) => {
                let w = &mut self.platte.punkte.gewaehlt;
                let drin = w.contains(&nr);
                if !drin {
                    if !mods.ctrl {
                        w.clear();
                    }
                    w.push(nr.clone());
                }
                (false, nr, drin && mods.ctrl)
            }
        };
        let spots = self.spots();
        let gp = spots.iter().find(|s| s.0 == nr).map(|s| s.1.p);
        let ebene = match gp {
            Some(p) if self.drei_d() => p[2],
            _ => 0.0,
        };
        let senkrecht = gp
            .filter(|_| hoehe && self.drei_d())
            .and_then(|p| self.senkrechte(x, y, vw, vh, p));
        let Some(start) = self
            .cam
            .plane_point(x, y, vw, vh, ebene)
            .map(|p| [p.x, p.y])
        else {
            self.redraw = true;
            return false;
        };
        let w = &self.platte.punkte.gewaehlt;
        let alt: Vec<(usize, [f64; 3])> = spots
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| (s.1.i, s.1.p))
            .collect();
        let griff = gp.map_or(start, |p| [p[0], p[1]]);
        let neu = alt.iter().map(|a| a.1).collect();
        // ein Messpunkt mit geschlossenem Schloss lässt sich nur kopieren
        let offen = &self.platte.leiste.offen_nr;
        let fest = spots
            .iter()
            .any(|s| w.contains(&s.0) && !s.1.hand && !offen.contains(&s.0));
        self.platte.punkte.zug = Some(Zug {
            hoehe,
            px: (x, y),
            start,
            griff,
            alt,
            neu,
            ziele: fang::ziele(&self.scene),
            kopie: mods.ctrl,
            bewegt: false,
            fest,
            ebene,
            senkrecht,
            raus,
            nr,
        });
        self.redraw = true;
        true
    }

    /// Ziehen: nur die Vorschau, das Modell bleibt bis zum Loslassen.
    fn punkte_ziehen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64, mods: Modifiers) {
        let ebene = self.platte.punkte.zug.as_ref().map_or(0.0, |z| z.ebene);
        let tol = {
            let a = self.cam.plane_point(x, y, vw, vh, ebene);
            let b = self
                .cam
                .plane_point(x + fang::FANG_PX * sc, y, vw, vh, ebene);
            a.zip(b).map_or(0.0, |(a, b)| (b.x - a.x).hypot(b.y - a.y))
        };
        let boden = self
            .cam
            .plane_point(x, y, vw, vh, ebene)
            .map(|p| [p.x, p.y]);
        let strahl = self.cam.ray(x, y, vw, vh);
        let Some(z) = self.platte.punkte.zug.as_mut() else {
            return;
        };
        if !z.bewegt && (x - z.px.0).hypot(y - z.px.1) < 3.0 * sc {
            return;
        }
        z.kopie |= mods.ctrl;
        if z.fest && !z.kopie {
            self.status(Meldung::satz(MESSPUNKT), crate::NOTICE_TIME);
            return;
        }
        let Some(z) = self.platte.punkte.zug.as_mut() else {
            return;
        };
        z.bewegt = true;
        let text = if z.hoehe {
            let d = match z.senkrecht {
                Some((p, n, z0)) => match senkrecht_z(strahl.0, strahl.1, p, n) {
                    Some(zz) => hub_3d(zz - z0, mods.shift),
                    None => return,
                },
                None => hub(y - z.px.1, sc, mods.shift),
            };
            z.neu = z.alt.iter().map(|a| [a.1[0], a.1[1], a.1[2] + d]).collect();
            let erster = z.neu[0][2];
            (erster, format!("Höhe {}", m_vz(d)))
        } else {
            let Some(p) = boden else {
                return;
            };
            let d = lage(z, p, tol, mods);
            z.neu = z
                .alt
                .iter()
                .map(|a| [a.1[0] + d[0], a.1[1] + d[1], a.1[2]])
                .collect();
            let l = d[0].hypot(d[1]) / 1000.0;
            (z.neu[0][2], format!("verschoben um {} m", super::de(l, 2)))
        };
        let kopie = if z.kopie { " · Kopie" } else { "" };
        let nr = z.nr.clone();
        let h = self.punkt_hoehe(text.0);
        let zeile = format!("Höhenpunkt {nr}: {h} · {}{kopie}", text.1);
        self.status(Meldung::mit("{}", &[&zeile]), Duration::from_secs(3600));
        self.redraw = true;
    }

    /// Loslassen: ein Schritt (verschieben oder kopieren); ein Klick ohne
    /// Zug mit Strg nimmt den Punkt aus der Wahl, wenn er schon drin war.
    fn punkte_loslassen(&mut self, z: Zug, mods: Modifiers) {
        if !z.bewegt {
            if z.raus {
                self.platte.punkte.gewaehlt.retain(|n| *n != z.nr);
            }
            self.punkte_zeile(self.platte.punkte.hover.clone().as_ref());
            self.redraw = true;
            return;
        }
        let kopie = z.kopie || mods.ctrl;
        let ok = if kopie {
            let mut neu = Vec::new();
            let ok = self.scene.edit_model(LABEL_KOPIE, |m| {
                neu = m.add_terrain_points(&z.neu);
                !neu.is_empty()
            });
            if ok {
                self.punkte_waehlen(&neu);
            }
            ok
        } else {
            let paare: Vec<(usize, [f64; 3])> = z
                .alt
                .iter()
                .map(|a| a.0)
                .zip(z.neu.iter().copied())
                .collect();
            let label = if z.hoehe { LABEL_HOEHE } else { LABEL_LAGE };
            self.scene
                .edit_model(label, |m| m.move_terrain_points(&paare))
        };
        if ok {
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
            self.redraw = true;
        }
    }

    /// Die Punkte mit diesen Indizes wählen (nach Kopie oder Setzen).
    fn punkte_waehlen(&mut self, idx: &[usize]) {
        let m = self.scene.model();
        let Some(t) = m.terrain() else {
            return;
        };
        self.platte.punkte.gewaehlt = idx
            .iter()
            .filter_map(|&i| t.points.get(i).map(|p| p.nr.clone()))
            .collect();
    }

    /// Nach einem Schritt: Bild, Mengen und Leiste nachziehen.
    fn punkte_fertig(&mut self) {
        self.upload_model();
        self.quantity.dirty = true;
        self.platte.leiste.neu = true;
        self.gelaende_sync();
        self.punkte_zeile(None);
        self.redraw = true;
    }

    /// Esc: den Zug abbrechen, sonst die Wahl aufheben. `true`, wenn Esc
    /// den Punkten galt.
    pub(crate) fn punkte_escape(&mut self) -> bool {
        if self.platte.punkte.zug.take().is_some() {
            self.punkte_zeile(self.platte.punkte.hover.clone().as_ref());
            self.redraw = true;
            return true;
        }
        if self.platte.punkte.gewaehlt.is_empty() || !self.punkte_an() {
            return false;
        }
        self.platte.punkte.gewaehlt.clear();
        self.gelaende_sync();
        self.redraw = true;
        true
    }

    /// „Gemessen zurück“: die gewählten geänderten Messpunkte auf die
    /// gemessenen Werte (I5), ein Schritt.
    pub(crate) fn punkte_gemessen(&mut self) {
        let w = self.platte.punkte.gewaehlt.clone();
        let idx: Vec<usize> = self
            .spots()
            .iter()
            .filter(|s| w.contains(&s.0) && s.1.geaendert)
            .map(|s| s.1.i)
            .collect();
        if self
            .scene
            .edit_model(LABEL_GEMESSEN, |m| m.reset_terrain_points(&idx))
        {
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
        }
    }

    /// Entf löscht die gewählten Punkte; `true`, wenn Entf ihnen galt.
    pub(crate) fn punkte_entf(&mut self) -> bool {
        if self.platte.punkte.gewaehlt.is_empty() || !self.punkte_an() {
            return false;
        }
        let w = self.platte.punkte.gewaehlt.clone();
        let idx: Vec<usize> = self
            .spots()
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| s.1.i)
            .collect();
        if self
            .scene
            .edit_model(LABEL_GELOESCHT, |m| m.remove_terrain_points(&idx))
        {
            self.platte.punkte.gewaehlt.clear();
            self.platte.punkte.hover = None;
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(DREI), crate::NOTICE_TIME);
        }
        true
    }

    /// Strg+D: die gewählten Punkte um 1,00 m versetzt kopieren und die
    /// Kopien wählen. `true`, wenn die Taste den Punkten galt.
    pub(crate) fn punkte_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        if !(key == Key::Char('D') && mods.ctrl && !mods.alt)
            || self.platte.punkte.gewaehlt.is_empty()
            || !self.punkte_an()
        {
            return false;
        }
        let w = self.platte.punkte.gewaehlt.clone();
        let neu: Vec<[f64; 3]> = self
            .spots()
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| [s.1.p[0] + VERSATZ, s.1.p[1], s.1.p[2]])
            .collect();
        let mut idx = Vec::new();
        if self.scene.edit_model(LABEL_KOPIE, |m| {
            idx = m.add_terrain_points(&neu);
            !idx.is_empty()
        }) {
            self.punkte_waehlen(&idx);
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
        }
        true
    }

    /// Statuszeile zum Ziel unter der Maus; ohne Ziel geht ein Satz der
    /// Punkte wieder aus der Zeile.
    fn punkte_zeile(&mut self, ziel: Option<&Ziel>) {
        match ziel {
            Some(Ziel::Hoehe) => self.status(Meldung::satz(HOEHE), Duration::from_secs(3600)),
            Some(Ziel::Punkt(nr)) => {
                let spots = self.spots();
                let Some(s) = spots.iter().find(|s| s.0 == *nr) else {
                    return;
                };
                let h = self.punkt_hoehe(s.1.p[2]);
                self.status(Meldung::mit(PUNKT, &[nr, &h]), Duration::from_secs(3600));
            }
            None => {
                let unsere = |t: &str| t == HOEHE || t.starts_with("Höhenpunkt ");
                if self.notice.as_ref().is_some_and(|n| unsere(&n.text)) {
                    self.notice = None;
                    self.renderer
                        .set_overlay(crate::OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                    self.redraw = true;
                }
            }
        }
    }

    /// Die gewählten Punkte und der unter der Maus, der Höhengriff an der
    /// Wahl, beim Ziehen die Punkte an ihrer neuen Stelle. Im Grundriss auf
    /// der Zeichenebene `z`, in 3D jeder Punkt in seiner Höhe.
    pub(crate) fn punkte_linien(&self, z: f64, scale: f32, theme: &Theme) -> Vec<Helper> {
        if !self.punkte_an() {
            return Vec::new();
        }
        let drei_d = self.drei_d();
        let col = &theme.interact;
        let p = |q: [f64; 3]| {
            let h = if drei_d { q[2] } else { z };
            [q[0] as f32, q[1] as f32, h as f32 + 3.0]
        };
        let punkt = |q: [f64; 3], px: f32, color, round| Helper {
            a: p(q),
            b: p(q),
            color,
            width: px * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: false,
            round,
        };
        let linie = |a: [f64; 3], b: [f64; 3], color, w: f32| Helper {
            a: p(a),
            b: p(b),
            color,
            width: w * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: false,
            round: false,
        };
        let mut out = Vec::new();
        let spots = self.spots();
        let w = &self.platte.punkte.gewaehlt;
        let heiss = match &self.platte.punkte.hover {
            Some(Ziel::Punkt(nr)) => Some(nr.clone()),
            _ => None,
        };
        for s in spots.iter() {
            let an = heiss.as_ref() == Some(&s.0);
            if w.contains(&s.0) || an {
                let farbe = if an { col.drag } else { col.select };
                out.push(punkt(s.1.p, PUNKT_PX * 1.3, farbe, true));
            }
        }
        let zug = self.platte.punkte.zug.as_ref();
        if let Some(zug) = zug.filter(|z| z.bewegt) {
            for (a, n) in zug.alt.iter().zip(&zug.neu) {
                let anders = if drei_d {
                    a.1 != *n
                } else {
                    a.1[..2] != n[..2]
                };
                if anders {
                    out.push(linie(a.1, *n, col.guide, 1.0));
                }
                out.push(punkt(*n, PUNKT_PX * 1.3, col.drag, true));
            }
        }
        let (vw, vh, sc) = self.view_size();
        let an = self.platte.punkte.hover == Some(Ziel::Hoehe) || zug.is_some_and(|z| z.hoehe);
        let farbe = if an { col.drag } else { col.select };
        if drei_d {
            let lage =
                zug.is_some_and(|z| !z.hoehe) || heiss.is_some_and(|h| w.first() == Some(&h));
            let fuss = if lage { col.drag } else { col.select };
            out.extend(self.gizmo_3d(vw, vh, sc, (farbe, fuss), scale));
            return out;
        }
        // Höhengriff: senkrechter Pfeil rechts über dem ersten gewählten
        if let Some(g) = self.hoehen_griff(vw, vh, sc) {
            let welt = |x: f64, y: f64| self.cam.ground_point(x, y, vw, vh).map(|v| [v.x, v.y, z]);
            let l = PFEIL_PX * sc;
            if let (Some(a), Some(b), Some(c), Some(d)) = (
                welt(g.0, g.1 + l * 0.5),
                welt(g.0, g.1 - l * 0.5),
                welt(g.0 - l * 0.3, g.1 - l * 0.15),
                welt(g.0 + l * 0.3, g.1 - l * 0.15),
            ) {
                out.push(linie(a, b, farbe, 2.0));
                out.push(linie(b, c, farbe, 2.0));
                out.push(linie(b, d, farbe, 2.0));
                out.push(punkt(a, 5.0, farbe, true));
            }
        }
        out
    }

    /// In 3D am ersten gewählten Punkt Pfeil und Quadrat
    /// ([`gizmo_linien`]).
    fn gizmo_3d(
        &self,
        vw: f64,
        vh: f64,
        sc: f64,
        farben: ([f32; 4], [f32; 4]),
        scale: f32,
    ) -> Vec<Helper> {
        let Some((a, _)) = self.pfeil_3d(vw, vh, sc) else {
            return Vec::new();
        };
        gizmo_linien(&self.cam, a, (vw, vh, sc), farben, scale)
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

    fn spot(nr: &str, i: usize, x: f64, y: f64) -> (String, TerrainSpot) {
        (
            nr.into(),
            TerrainSpot {
                i,
                p: [x, y, 0.0],
                hand: true,
                geaendert: false,
            },
        )
    }

    /// Der nächste Punkt bis 7 Bildpunkte, daneben keiner.
    #[test]
    fn punkt_unter_der_maus() {
        let cam = kamera();
        let s = [spot("H1", 0, 5000.0, 5000.0), spot("H2", 1, 5200.0, 5000.0)];
        let t = |x: f64, y: f64| {
            let q = cam.project(vec3(x, y, 0.0), 800.0, 800.0).unwrap();
            treffen(&s, &cam, Some(0.0), q, 800.0, 800.0, 1.0).map(|s| s.0.clone())
        };
        assert_eq!(t(5050.0, 5000.0).as_deref(), Some("H1"));
        assert_eq!(t(5150.0, 5000.0).as_deref(), Some("H2"));
        assert_eq!(t(6000.0, 5000.0), None);
    }

    /// Setzen, Höhe ändern, kopieren und löschen sind je ein Schritt der
    /// Szene, den Rückgängig ganz zurücknimmt.
    #[test]
    fn schritte_der_szene() {
        let mut m = sk_model::Model::with_seed(1);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let mut s = crate::scene::Scene::with_model(m);
        let mut neu = Vec::new();
        assert!(s.edit_model(LABEL_GESETZT, |m| {
            neu = m.add_terrain_points(&[[15000.0, 4000.0, -300.0]]);
            !neu.is_empty()
        }));
        let i = neu[0];
        let vorher = s.model().terrain_spots();
        assert!(s.edit_model(LABEL_HOEHE, |m| m
            .move_terrain_points(&[(i, [15000.0, 4000.0, 200.0])])));
        let k = s.edit_model(LABEL_KOPIE, |m| {
            !m.add_terrain_points(&[[16000.0, 4000.0, 200.0]]).is_empty()
        });
        assert!(k);
        assert_eq!(s.model().terrain_spots().len(), vorher.len() + 1);
        assert!(s.undo() && s.undo());
        assert_eq!(s.model().terrain_spots(), vorher);
    }

    /// In 3D: Punkte in ihrer Höhe greifen; der Höhenpfeil zählt auf der
    /// senkrechten Ebene zur Kamera, die Maus und der Punkt bleiben
    /// beieinander.
    #[test]
    fn hoehe_in_3d() {
        let (w, h) = (800.0, 600.0);
        let cam = Camera::looking_at(
            vec3(-20000.0, -15000.0, 12000.0),
            vec3(5000.0, 5000.0, 0.0),
            45.0,
        );
        let mut hoch = spot("H1", 0, 5000.0, 5000.0);
        hoch.1.p[2] = 2500.0;
        let s = [hoch, spot("H2", 1, 5000.0, 5000.0)];
        // H1 steht 2,50 m über H2: in 3D jeder in seiner Höhe getroffen
        let q = cam.project(vec3(5000.0, 5000.0, 2500.0), w, h).unwrap();
        let t = treffen(&s, &cam, None, q, w, h, 1.0).map(|s| s.0.as_str());
        assert_eq!(t, Some("H1"));
        let t = treffen(&s, &cam, Some(0.0), q, w, h, 1.0).map(|s| s.0.as_str());
        assert_eq!(t, None, "auf der Zeichenebene liegt dort keiner");
        // Maus auf der Höhe +0,73 m über dem Punkt: der Strahl trifft die
        // senkrechte Ebene dort
        let f = cam.forward();
        let n = vec3(f.x, f.y, 0.0).normalized();
        let p = vec3(5000.0, 5000.0, 0.0);
        let z = |zz: f64| {
            let q = cam.project(vec3(5000.0, 5000.0, zz), w, h).unwrap();
            let (o, d) = cam.ray(q.0, q.1, w, h);
            senkrecht_z(o, d, p, n).unwrap()
        };
        assert!((z(730.0) - 730.0).abs() < 1e-6);
        assert_eq!(hub_3d(z(734.0) - z(0.0), false), 730.0);
        assert_eq!(hub_3d(z(734.0) - z(0.0), true), 700.0);
        // ein Strahl parallel zur Ebene trifft sie nicht
        assert_eq!(senkrecht_z(p - n * 10.0, cam.right(), p, n), None);
        assert_eq!(abstand((5.0, 3.0), (0.0, 0.0), (10.0, 0.0)), 3.0);
        assert_eq!(abstand((-4.0, 3.0), (0.0, 0.0), (10.0, 0.0)), 5.0);
    }

    #[test]
    fn hub_in_cm() {
        assert_eq!(hub(-10.0, 1.0, false), 100.0);
        assert_eq!(hub(4.0, 1.0, false), -40.0);
        assert_eq!(hub(-3.0, 2.0, false), 20.0, "auf cm gerundet");
        assert_eq!(
            hub(-10.0, 1.0, true),
            1000.0,
            "mit Umschalt 10 cm je Bildpunkt"
        );
        assert_eq!(m_vz(-320.0), "−0,32");
        assert_eq!(m_vz(0.0), "+0,00");
    }

    /// Lage beim Ziehen: im Raster, mit Alt ohne Fang.
    #[test]
    fn lage_im_raster() {
        let z = Zug {
            hoehe: false,
            px: (0.0, 0.0),
            start: [1000.0, 1000.0],
            griff: [1000.0, 1000.0],
            alt: vec![(0, [1000.0, 1000.0, 0.0])],
            neu: vec![],
            ziele: Ziele::default(),
            kopie: false,
            bewegt: true,
            fest: false,
            ebene: 0.0,
            senkrecht: None,
            raus: false,
            nr: "H1".into(),
        };
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        let d = lage(&z, [1234.0, 987.0], 50.0, alt);
        assert_eq!(
            d,
            [
                (1234.0f64 / STEP).round() * STEP - 1000.0,
                (987.0f64 / STEP).round() * STEP - 1000.0
            ]
        );
    }
}

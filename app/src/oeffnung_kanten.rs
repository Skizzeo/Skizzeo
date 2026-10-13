//! Öffnung von außen ändern (Paket O7, Jörn 13.10. 01:57): In 3D und den
//! vier Ansichten zeigt eine Öffnung unter der Maus ihre Kanten als
//! violettes Band, ein Klick in sie wählt sie. Die Kante bei der Maus
//! greift: seitlich ändert sie die Breite wie der Griff im Grundriss
//! ([`super::breite_edit::BreiteZug::greifen_aussen`], mit Raster, Spur und
//! Breitenschloss), oben und unten die Höhe ([`HoeheZug`]).
//!
//! Die Höhe fängt an Brüstung, Sturz und OK Rohdecke des eigenen Geschosses
//! und der Geschosse direkt darüber und darunter und bindet dort: So wird
//! aus einem Fenster eines über zwei Geschosse, und es wandert mit, wenn
//! die Linie im Geschossmanager gezogen wird. Frei steht die Kante im
//! Raster von 10 mm als Maß über OK Rohdecke des Geschosses, in dem sie
//! liegt. Das Schloss neben Ober- und Unterkante zeigt die Bindung; ein
//! Klick löst sie (die Höhe bleibt) oder bindet die Kante an die Linie, auf
//! der sie gerade steht. Teil von [`crate::wall_edit`].

use super::ende_edit::{Outcome, GRIFF_PX};
use crate::camera::Camera;
use crate::link_view::{Schloss, Schlossart};
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::meldung::Meldung;
use crate::scene::kanten::VOR_WAND;
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::oeffnung::{FaceLine, HeightLine, HeightRef, Opening, MIN_OPENING};
use sk_model::{ElementId, LevelEdge, LevelKind, LevelRef, Model, StoreyId};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Öffnungshöhe geändert";
/// Feldname der Eingabe.
pub const LABEL: &str = "Höhe";
/// Raster der freien Höhe (mm).
const RASTER: f64 = 10.0;
/// Fangabstand an den Höhenlinien (Pixel, 96 dpi), wie beim Aufziehen.
const FANG_PX: f64 = 10.0;
/// So weit um ihre Kanten bleibt die Öffnung unter der Maus (Pixel): Die
/// Maus erreicht die Schlösser daneben.
const HALTEN_PX: f64 = 60.0;
/// Bänder so weit vor der Fläche (mm), damit die Wand sie nicht verdeckt.
pub const VOR: f64 = 5.0;
/// Band wie am Wandfuß: unter der Maus mit Schatten, sonst dünn (dip).
const BAND: f32 = 4.0;
const BAND_SCHATTEN: f32 = 6.0;
const BAND_RUHE: f32 = 2.0;
/// Überstand der gebundenen Höhenlinie neben der Öffnung (mm).
const UEBER: f64 = 300.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Kante einer Öffnung von außen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kante {
    /// Seitenkante am Pfadpunkt (erster oder letzter).
    Ende(usize),
    Unten,
    Oben,
}

/// Öffnung unter der Maus, der Schenkel, auf den die Maus zeigt, und die
/// Kante bei der Maus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Treffer {
    pub id: ElementId,
    pub schenkel: usize,
    pub kante: Option<Kante>,
}

/// Schenkel auf der Außenfläche (flach) mit dem Band (UK, OK absolut).
pub type Schenkel = (Vec<(Vec3, Vec3)>, f64, f64);

/// Die Schenkel der Öffnung `o`.
pub fn schenkel(m: &Model, o: &Opening) -> Option<Schenkel> {
    let (z0, z1) = m.opening_band(o)?;
    if !(2..=3).contains(&o.path.len()) {
        return None;
    }
    let legs = o.path.windows(2).map(|w| (flat(w[0]), flat(w[1])));
    Some((legs.collect(), z0, z1))
}

/// Millimeter je Bildpunkt bei `p`.
fn mm_je_px(cam: &Camera, p: Vec3, w: f64, h: f64) -> Option<f64> {
    let a = cam.project(p, w, h)?;
    let b = cam.project(p + vec3(0.0, 0.0, 100.0), w, h)?;
    let n = (a.0 - b.0).hypot(a.1 - b.1);
    (n > 1e-6).then(|| 100.0 / n)
}

/// Senkrechte Ebene durch den Schenkel `a`–`b`: Richtung und Normale.
fn ebene(a: Vec3, b: Vec3) -> Option<(Vec3, Vec3, f64)> {
    let len = (b - a).length();
    (len >= 1.0).then(|| {
        let d = (b - a) * (1.0 / len);
        (d, vec3(-d.y, d.x, 0.0), len)
    })
}

/// Öffnung, deren Außenfläche der Strahl unter der Maus (Ansichtspixel
/// `x`, `y`) trifft, auch knapp neben ihren Kanten; die vorderste, wenn
/// nichts davor liegt. `halten` bleibt bis [`HALTEN_PX`] um ihre Kanten
/// getroffen.
pub fn treffer(
    scene: &Scene,
    cam: &Camera,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
    halten: Option<ElementId>,
) -> Option<Treffer> {
    let (o, d) = cam.ray(x, y, w, h);
    let mut best: Option<(f64, Treffer)> = None;
    // Nur die Schenkel in den Feldern, die der Strahl kreuzt (W43 B1)
    let (_, wand) = scene.kanten_strahl(o, d, |s| {
        if !scene.pickable(s.id) {
            return None;
        }
        let (dir, nrm, len) = ebene(s.a, s.b)?;
        let dn = d.dot(nrm);
        if dn.abs() < 1e-9 {
            return None;
        }
        let t = (s.a - o).dot(nrm) / dn;
        if t <= 0.0 || best.is_some_and(|b| b.0 <= t) {
            return None;
        }
        let p = o + d * t;
        let mm = mm_je_px(cam, p, w, h)?;
        let tol = GRIFF_PX * scale * mm;
        let rand = if halten == Some(s.id) {
            HALTEN_PX * scale * mm
        } else {
            tol
        };
        let (u, z, z0, z1) = ((p - s.a).dot(dir), p.z, s.z0, s.z1);
        if u < -rand || u > len + rand || z < z0 - rand || z > z1 + rand {
            return None;
        }
        // Die nächste Kante innerhalb des Greifabstands
        let in_u = u >= -tol && u <= len + tol;
        let in_z = z >= z0 - tol && z <= z1 + tol;
        let mut kanten = Vec::new();
        if s.k == 0 && in_z {
            kanten.push((u.abs(), Kante::Ende(0)));
        }
        if s.k + 1 == s.n && in_z {
            kanten.push(((u - len).abs(), Kante::Ende(s.n)));
        }
        if in_u {
            kanten.push(((z - z0).abs(), Kante::Unten));
            kanten.push(((z1 - z).abs(), Kante::Oben));
        }
        let kante = kanten
            .into_iter()
            .filter(|x| x.0 <= tol)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|x| x.1);
        let tr = Treffer {
            id: s.id,
            schenkel: s.k,
            kante,
        };
        best = Some((t, tr));
        Some(t)
    });
    let (t, tr) = best?;
    // Durch das Loch geht der Strahl weiter; knapp daneben trifft er die
    // Wand in der Fläche selbst. Davor darf nichts liegen.
    wand.is_none_or(|ts| ts >= t - VOR_WAND).then_some(tr)
}

/// Ein Band von `a` nach `b`: unter der Maus mit Schatten, sonst dünn.
pub fn band(
    a: Vec3,
    b: Vec3,
    heiss: bool,
    scale: f32,
    occlude: bool,
    theme: &Theme,
) -> Vec<Helper> {
    let col = &theme.interact;
    let line = |color, width: f32| Helper {
        a: [a.x as f32, a.y as f32, a.z as f32],
        b: [b.x as f32, b.y as f32, b.z as f32],
        color,
        width: width * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude,
        round: false,
    };
    if heiss {
        vec![
            line(col.shadow_band, BAND_SCHATTEN),
            line(col.drag_hot, BAND),
        ]
    } else {
        vec![line(col.drag, BAND_RUHE)]
    }
}

/// Zum Auge hin: so liegen die Bänder vor der Fläche.
fn zum_auge(cam: &Camera, nrm: Vec3) -> Vec3 {
    if nrm.dot(cam.forward()) > 0.0 {
        nrm * -VOR
    } else {
        nrm * VOR
    }
}

/// Die Kanten der Öffnung unter der Maus als Band, die Kante bei der Maus
/// hervorgehoben.
pub fn baender(
    scene: &Scene,
    cam: &Camera,
    t: Treffer,
    scale: f32,
    occlude: bool,
    theme: &Theme,
) -> Vec<Helper> {
    let m = scene.model();
    let Some((legs, z0, z1)) = m.opening(t.id).and_then(|o| schenkel(m, o)) else {
        return Vec::new();
    };
    let n = legs.len();
    let mut out = Vec::new();
    for (k, &(a, b)) in legs.iter().enumerate() {
        let Some((_, nrm, _)) = ebene(a, b) else {
            continue;
        };
        let v = zum_auge(cam, nrm);
        let at = |p: Vec3, z: f64| vec3(p.x, p.y, z) + v;
        let mut kante = |p: Vec3, q: Vec3, welche: Kante| {
            let heiss = t.kante == Some(welche);
            out.extend(band(p, q, heiss, scale, occlude, theme));
        };
        kante(at(a, z0), at(b, z0), Kante::Unten);
        kante(at(a, z1), at(b, z1), Kante::Oben);
        if k == 0 {
            kante(at(a, z0), at(a, z1), Kante::Ende(0));
        }
        if k + 1 == n {
            kante(at(b, z0), at(b, z1), Kante::Ende(n));
        }
    }
    out
}

/// Geschosse des Gebäudes von `st` von unten nach oben, ohne Gründung und
/// Flachdach, mit ihrer Unterkante.
fn geschosse(m: &Model, st: StoreyId) -> Vec<(StoreyId, f64)> {
    let b = m.storey(st).and_then(|s| s.building);
    m.levels_in(b)
        .into_iter()
        .filter_map(|id| {
            let s = m.storey(id)?;
            (s.kind == LevelKind::Storey).then_some((id, s.elevation))
        })
        .collect()
}

/// Höhenlinien, an denen eine Kante der Öffnung fängt: Brüstung, Sturz und
/// OK Rohdecke des Geschosses `st` und der Geschosse direkt darüber und
/// darunter.
pub fn linien(m: &Model, st: StoreyId) -> Vec<FaceLine> {
    let alle = geschosse(m, st);
    let Some(i) = alle.iter().position(|g| g.0 == st) else {
        return Vec::new();
    };
    let von = i.saturating_sub(1);
    let bis = (i + 2).min(alle.len());
    let mut out = Vec::new();
    for &(s, e) in &alle[von..bis] {
        out.push(FaceLine {
            z: e,
            bind: HeightRef::Level(LevelRef::bottom(s)),
        });
        out.extend(m.face_lines(s));
    }
    out
}

/// Name der Bindung für die Pille: „Sturz OG“, „OK Rohdecke EG“; frei
/// `None`.
pub fn bindung_name(m: &Model, r: HeightRef) -> Option<String> {
    let kurz = |s: StoreyId| m.storey(s).map_or(String::new(), |x| x.short.clone());
    match r {
        HeightRef::Line(s, l) => {
            let was = match l {
                HeightLine::Sill => "Brüstung",
                HeightLine::Lintel => "Sturz",
                HeightLine::Door => "Innentürlinie",
            };
            Some(format!("{was} {}", kurz(s)))
        }
        HeightRef::Level(l) if l.edge == LevelEdge::Bottom && l.offset == 0.0 => {
            Some(format!("OK Rohdecke {}", kurz(l.storey)))
        }
        HeightRef::Level(_) => None,
    }
}

/// Freie Höhe `z` (genau) als Maß über OK Rohdecke des Geschosses, in dem
/// sie liegt. Genau auf OK Rohdecke wäre das die Bindung selbst (W43 B3):
/// Dort zählt sie vom Geschoss darunter, im untersten von dessen Oberkante.
fn frei_bei(m: &Model, st: StoreyId, z: f64) -> HeightRef {
    let alle = geschosse(m, st);
    let i = alle
        .iter()
        .rposition(|g| g.1 <= z + 1e-6)
        .unwrap_or_default();
    let (s, e) = alle
        .get(i)
        .copied()
        .unwrap_or((st, m.storey(st).map_or(0.0, |s| s.elevation)));
    let r = |storey, edge, offset| {
        HeightRef::Level(LevelRef {
            storey,
            edge,
            offset,
        })
    };
    if (z - e).abs() > 1e-6 {
        return r(s, LevelEdge::Bottom, z - e);
    }
    match i.checked_sub(1).and_then(|j| alle.get(j)) {
        Some(&(u, eu)) => r(u, LevelEdge::Bottom, z - eu),
        None => r(s, LevelEdge::Top, z - m.storey(s).map_or(e, |x| x.top())),
    }
}

/// Die Öffnung `id` mit gelöster bzw. gebundener Ober- (`oben`) oder
/// Unterkante: gebunden wird sie frei auf ihrer Höhe; frei bindet sie an
/// die Höhenlinie, auf der sie steht. `None`, wenn es keine solche gibt.
pub fn umbinden(m: &Model, id: ElementId, oben: bool) -> Option<Opening> {
    let mut o = m.opening(id)?.clone();
    let st = m.element(id)?.storey;
    let r = if oben { o.top } else { o.bottom };
    let z = m.height_z(r)?;
    let neu = if bindung_name(m, r).is_some() {
        frei_bei(m, st, z)
    } else {
        linien(m, st)
            .into_iter()
            .find(|l| (l.z - z).abs() < 0.5)?
            .bind
    };
    if oben {
        o.top = neu;
    } else {
        o.bottom = neu;
    }
    Some(o)
}

/// Schlösser an der Öffnung unter der Maus: das Breitenschloss je Ende mit
/// Partner darüber oder darunter, neben der Seitenkante; die Höhenbindung
/// an Ober- und Unterkante, wenn sie gebunden ist oder auf einer Linie
/// steht.
pub fn schloesser(scene: &Scene, t: Treffer) -> Vec<Schloss> {
    let m = scene.model();
    let Some(o) = m.opening(t.id) else {
        return Vec::new();
    };
    let Some((legs, z0, z1)) = schenkel(m, o) else {
        return Vec::new();
    };
    let Some(st) = m.element(t.id).map(|e| e.storey) else {
        return Vec::new();
    };
    let mitte = (z0 + z1) / 2.0;
    let mut out = Vec::new();
    let n = o.path.len();
    for (k, nb) in [(0, 1), (n - 1, n - 2)] {
        let partner = super::breite_edit::partner(m, t.id, k);
        if partner.is_empty() {
            continue;
        }
        let locks = m.end_locks(t.id, k);
        let linked = partner
            .iter()
            .all(|&(pid, pk)| locks.iter().any(|l| l.partner == pid && l.end == pk));
        let (p, q) = (flat(o.path[k]), flat(o.path[nb]));
        out.push(Schloss {
            id: t.id,
            art: Schlossart::Ende(k),
            p: vec3(p.x, p.y, mitte),
            out: (p - q).normalized(),
            linked,
        });
    }
    let (a, b) = legs[t.schenkel.min(legs.len() - 1)];
    let mid = (a + b) * 0.5;
    let linien = linien(m, st);
    for (oben, r, z) in [(false, o.bottom, z0), (true, o.top, z1)] {
        let gebunden = bindung_name(m, r).is_some();
        if !gebunden && !linien.iter().any(|l| (l.z - z).abs() < 0.5) {
            continue;
        }
        out.push(Schloss {
            id: t.id,
            art: Schlossart::Hoehe(oben),
            p: vec3(mid.x, mid.y, z),
            out: vec3(0.0, 0.0, if oben { 1.0 } else { -1.0 }),
            linked: gebunden,
        });
    }
    out
}

/// Laufendes Ziehen an Ober- oder Unterkante einer Öffnung.
pub struct HoeheZug {
    pub id: ElementId,
    oben: bool,
    original: Opening,
    /// Ebene des gegriffenen Schenkels: Punkt, Richtung, Normale.
    a: Vec3,
    nrm: Vec3,
    /// Höhenlinien, an denen die Kante fängt ([`linien`]).
    linien: Vec<FaceLine>,
    /// Geschoss der Öffnung.
    st: StoreyId,
    /// Höhe der anderen Kante.
    gegen: f64,
    /// Fangabstand (mm).
    tol: f64,
    /// Ein Zug am Pfad (er wird beim Ziehen live gezeichnet).
    run: Option<sk_model::RunId>,
    geaendert: bool,
    /// Getippte Höhe; solange sie offen ist, ruht die Maus.
    pub input: Option<MeasureInput>,
    maus: Option<(f64, f64)>,
}

impl HoeheZug {
    /// Greift Ober- (`oben`) oder Unterkante der Öffnung `t.id` am Schenkel
    /// `t.schenkel` und öffnet den Schritt. Der Fangabstand kommt aus der
    /// Maus `(x, y)`.
    #[allow(clippy::too_many_arguments)]
    pub fn greifen(
        scene: &mut Scene,
        t: Treffer,
        oben: bool,
        cam: &Camera,
        (x, y): (f64, f64),
        w: f64,
        h: f64,
        scale: f64,
    ) -> Option<HoeheZug> {
        let m = scene.model();
        let original = m.opening(t.id)?.clone();
        let st = m.element(t.id)?.storey;
        let (legs, z0, z1) = schenkel(m, &original)?;
        let (a, b) = *legs.get(t.schenkel)?;
        let (_, nrm, _) = ebene(a, b)?;
        let (o, d) = cam.ray(x, y, w, h);
        let dn = d.dot(nrm);
        let p = if dn.abs() > 1e-9 {
            o + d * ((a - o).dot(nrm) / dn)
        } else {
            a
        };
        let tol = FANG_PX * scale * mm_je_px(cam, p, w, h).unwrap_or(0.0);
        let koerper = |r| scene.koerper(r);
        let run =
            super::breite_edit::flaeche_bei(m, &koerper, st, original.path[0]).map(|f| f.0.run);
        let linien = linien(m, st);
        scene.begin(SCHRITT);
        Some(HoeheZug {
            id: t.id,
            oben,
            original,
            a,
            nrm,
            linien,
            st,
            gegen: if oben { z0 } else { z1 },
            tol,
            run,
            geaendert: false,
            input: None,
            maus: None,
        })
    }

    /// Zug am Pfad, live gezeichnet ([`Scene::set_opening`] rechnet alle
    /// betroffenen neu).
    pub fn run(&self) -> Option<sk_model::RunId> {
        self.run
    }

    /// Hält die Kante bei `z` die Mindesthöhe zur anderen?
    fn hoch_genug(&self, z: f64) -> bool {
        if self.oben {
            z >= self.gegen + MIN_OPENING - 1e-6
        } else {
            z <= self.gegen - MIN_OPENING + 1e-6
        }
    }

    /// Bindung der Kante für die Maushöhe `z`: die nächste Höhenlinie im
    /// Fangabstand, sonst frei im Raster, immer mindestens [`MIN_OPENING`]
    /// von der anderen Kante.
    fn bindung(&self, m: &Model, z: f64) -> HeightRef {
        let fang = self
            .linien
            .iter()
            .filter(|l| (l.z - z).abs() <= self.tol && self.hoch_genug(l.z))
            .min_by(|a, b| (a.z - z).abs().total_cmp(&(b.z - z).abs()));
        if let Some(l) = fang {
            return l.bind;
        }
        let grenze = if self.oben {
            z.max(self.gegen + MIN_OPENING)
        } else {
            z.min(self.gegen - MIN_OPENING)
        };
        let HeightRef::Level(mut l) = frei_bei(m, self.st, grenze) else {
            return frei_bei(m, self.st, grenze);
        };
        l.offset = (l.offset / RASTER).round() * RASTER;
        let z_neu = m.level_z(l).unwrap_or(grenze);
        if !self.hoch_genug(z_neu) {
            l.offset += if self.oben { RASTER } else { -RASTER };
        }
        HeightRef::Level(l)
    }

    /// Setzt die Kante auf `r`; `None`, wenn das Modell ablehnt (der letzte
    /// Stand bleibt), sonst ob sich etwas geändert hat.
    fn setzen(&mut self, scene: &mut Scene, r: HeightRef) -> Option<bool> {
        let mut o = self.original.clone();
        if self.oben {
            o.top = r;
        } else {
            o.bottom = r;
        }
        if scene.model().opening(self.id) == Some(&o) {
            return Some(false);
        }
        if !scene.set_opening(self.id, o) {
            return None;
        }
        self.geaendert = true;
        Some(true)
    }

    /// Höhe unter der Maus auf der Ebene des Schenkels.
    fn maus_z(&self, cam: &Camera, w: f64, h: f64) -> Option<f64> {
        let (x, y) = self.maus?;
        let (o, d) = cam.ray(x, y, w, h);
        let dn = d.dot(self.nrm);
        (dn.abs() > 1e-9).then(|| (o + d * ((self.a - o).dot(self.nrm) / dn)).z)
    }

    fn zur_maus(&mut self, scene: &mut Scene, cam: &Camera, w: f64, h: f64) -> bool {
        let Some(z) = self.maus_z(cam, w, h) else {
            return false;
        };
        let r = self.bindung(scene.model(), z);
        self.setzen(scene, r) == Some(true)
    }

    /// Getippte Höhe der Öffnung: die gezogene Kante genau dort, frei.
    /// Passt sie nicht, wird die Eingabe rot und Enter gilt nicht.
    fn getippt(&mut self, scene: &mut Scene) -> bool {
        let Some(Some(Ok(v))) = self.input.as_ref().map(|i| i.value(0)) else {
            return false;
        };
        let fehler = if v < MIN_OPENING - 1e-6 {
            Some(Meldung::satz("Eine Öffnung ist mindestens 10 cm hoch"))
        } else {
            let z = if self.oben {
                self.gegen + v
            } else {
                self.gegen - v
            };
            let r = frei_bei(scene.model(), self.st, z);
            match self.setzen(scene, r) {
                Some(geaendert) => return geaendert,
                None => Some(Meldung::satz("So hoch passt die Öffnung nicht in die Wand")),
            }
        };
        if let Some(i) = self.input.as_mut() {
            i.error = fehler;
        }
        false
    }

    fn schliessen(&mut self, scene: &mut Scene, out: &mut Outcome) {
        let gilt = self.input.as_ref().is_none_or(|i| i.error.is_none());
        let zurueck = !gilt || scene.model().opening(self.id) == Some(&self.original);
        if zurueck {
            scene.rollback();
        } else {
            scene.commit();
        }
        out.changed = self.geaendert && zurueck;
        out.fertig = true;
        out.redraw = true;
    }

    /// Maus, Tasten und Esc beim Ziehen; alle gehören dem Ziehen.
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
    ) -> Outcome {
        let mut out = Outcome::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                self.maus = Some((x, y));
                if self.input.is_none() && self.zur_maus(scene, cam, w, h) {
                    out.changed = true;
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => self.schliessen(scene, &mut out),
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if self.input.is_none() => {
                scene.rollback();
                out.changed = self.geaendert;
                out.fertig = true;
                out.redraw = true;
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => {
                out.redraw = true;
                let Some(i) = self.input.as_mut() else {
                    if let Key::Char(ch) = key {
                        if opens(ch, MeasureKind::Length, mods) {
                            let mut i = MeasureInput::new(MeasureKind::Length, None);
                            i.push(ch);
                            self.input = Some(i);
                            out.changed = self.getippt(scene);
                        }
                    }
                    return out;
                };
                match i.key(key, mods) {
                    InputOutcome::Enter => self.schliessen(scene, &mut out),
                    InputOutcome::Changed => out.changed = self.getippt(scene),
                    InputOutcome::Emptied | InputOutcome::Escape | InputOutcome::EnterEmpty => {
                        self.input = None;
                        out.changed = self.zur_maus(scene, cam, w, h);
                    }
                    InputOutcome::Ignored | InputOutcome::Refused => {}
                }
            }
            _ => {}
        }
        out
    }

    /// Pille am Cursor: die Eingabe, sonst Höhe und Bindung der Kante.
    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        if let Some(i) = &self.input {
            return Some((i.text([LABEL, ""]), true));
        }
        let m = scene.model();
        let o = m.opening(self.id)?;
        let (z0, z1) = m.opening_band(o)?;
        let hoehe = crate::ui::live_length_text(z1 - z0);
        let r = if self.oben { o.top } else { o.bottom };
        Some(match bindung_name(m, r) {
            Some(b) => (format!("{LABEL} {hoehe} · {b}"), false),
            None => (format!("{LABEL} {hoehe}"), false),
        })
    }

    /// Die gezogene Kante als Band; gebunden dazu die Höhenlinie über die
    /// Breite der Öffnung hinaus.
    pub fn helpers(
        &self,
        scene: &Scene,
        cam: &Camera,
        scale: f32,
        occlude: bool,
        theme: &Theme,
    ) -> Vec<Helper> {
        let m = scene.model();
        let Some(o) = m.opening(self.id) else {
            return Vec::new();
        };
        let Some((legs, z0, z1)) = schenkel(m, o) else {
            return Vec::new();
        };
        let (z, r) = if self.oben {
            (z1, o.top)
        } else {
            (z0, o.bottom)
        };
        let gebunden = bindung_name(m, r).is_some();
        let mut out = Vec::new();
        for (a, b) in legs {
            let Some((dir, nrm, _)) = ebene(a, b) else {
                continue;
            };
            let v = zum_auge(cam, nrm);
            let at = |p: Vec3| vec3(p.x, p.y, z) + v;
            out.extend(band(at(a), at(b), true, scale, occlude, theme));
            if gebunden {
                let ueber = dir * UEBER;
                out.push(Helper {
                    a: f32s(at(a) - ueber),
                    b: f32s(at(b) + ueber),
                    color: theme.interact.track,
                    width: theme.size.link_line * scale,
                    dash: theme.size.link_dash * scale,
                    pattern: sk_render::SOLID,
                    occlude,
                    round: false,
                });
            }
        }
        out
    }
}

fn f32s(p: Vec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

#[cfg(all(test, target_os = "linux"))]
#[path = "oeffnung_kanten_istbilder.rs"]
mod istbilder;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::wall_edit::breite_edit::BreiteZug;
    use sk_model::oeffnung::OpeningKind;
    use sk_model::RunId;

    pub const W: f64 = 1200.0;
    pub const H: f64 = 800.0;

    /// Zweigeschossiges Haus 10 × 8 m mit einem Fenster in der Südwand des
    /// EG von 6,5 bis 7,5 m, Brüstung bis Sturz; Kamera von Süden.
    pub fn haus() -> (Scene, Camera, ElementId, RunId, RunId) {
        let mut m = Model::with_seed(415);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        let st = m.run(eg).unwrap().storey;
        let seg = (0..4)
            .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap();
        let f = m.outer_face(eg, seg).unwrap();
        let (a, b) = (
            f.u_of(vec3(6500.0, 0.0, 0.0)),
            f.u_of(vec3(7500.0, 0.0, 0.0)),
        );
        let o = Opening {
            kind: OpeningKind::Window,
            path: vec![f.at(a, 0.0), f.at(b, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        };
        let id = m.add_opening(o).unwrap();
        let cam = Camera::looking_at(
            vec3(7000.0, -16000.0, 3500.0),
            vec3(7000.0, 0.0, 3000.0),
            45.0,
        );
        (Scene::with_model(m), cam, id, eg, og)
    }

    /// Punkt der Außenfläche der Öffnung bei `x` (mm) in Höhe `z`, im Bild.
    pub fn bei(s: &Scene, cam: &Camera, id: ElementId, x: f64, z: f64) -> (f64, f64) {
        let o = s.model().opening(id).unwrap();
        let y = o.path[0].y;
        cam.project(vec3(x, y, z), W, H).unwrap()
    }

    fn band(s: &Scene, id: ElementId) -> (f64, f64) {
        let m = s.model();
        m.opening_band(m.opening(id).unwrap()).unwrap()
    }

    /// Mitten in der Öffnung trifft der Strahl sie ohne Kante, an den
    /// Rändern die Kante; daneben nichts. Ein Klick wählt sie.
    #[test]
    fn treffer_und_kanten() {
        let (mut s, cam, id, ..) = haus();
        let (z0, z1) = band(&s, id);
        let mitte = (z0 + z1) / 2.0;
        let t = |s: &Scene, x, z| treffer(s, &cam, bei(s, &cam, id, x, z), W, H, 1.0, None);
        assert_eq!(
            t(&s, 7000.0, mitte).map(|t| (t.id, t.kante)),
            Some((id, None))
        );
        let o = s.model().opening(id).unwrap().clone();
        let links = usize::from(o.path[1].x < o.path[0].x);
        assert_eq!(
            t(&s, 6505.0, mitte).and_then(|t| t.kante),
            Some(Kante::Ende(links))
        );
        assert_eq!(
            t(&s, 7000.0, z1 - 5.0).and_then(|t| t.kante),
            Some(Kante::Oben)
        );
        assert_eq!(
            t(&s, 7000.0, z0 + 5.0).and_then(|t| t.kante),
            Some(Kante::Unten)
        );
        assert_eq!(t(&s, 3000.0, mitte), None);
        // Gehalten bleibt sie noch ein Stück daneben (Schlösser erreichbar)
        let p = bei(&s, &cam, id, 7000.0, z1 + 300.0);
        assert!(treffer(&s, &cam, p, W, H, 1.0, None).is_none());
        assert!(treffer(&s, &cam, p, W, H, 1.0, Some(id)).is_some());
        let (x, y) = bei(&s, &cam, id, 7000.0, mitte);
        let pick =
            crate::selection::pick_at(&mut s, &cam, crate::ui::ViewKind::Persp, None, x, y, W, H);
        assert_eq!(pick, Some(id));
    }

    /// Oberkante über das Wandband greifen und bis an den Sturz des OG
    /// ziehen: Sie bindet dort, das Fenster geht über zwei Geschosse, ein
    /// Schritt. Das Schloss löst die Bindung bei gleicher Höhe und bindet
    /// sie wieder.
    #[test]
    fn hoehe_ueber_zwei_geschosse() {
        let (mut s, cam, id, _, og) = haus();
        let (z0, z1) = band(&s, id);
        let st_og = s.model().run(og).unwrap().storey;
        let sturz_og = HeightRef::Line(st_og, HeightLine::Lintel);
        let z_og = s.model().height_z(sturz_og).unwrap();
        let mut e = crate::wall_edit::WallEdit::default();
        let mods = Default::default();
        let (x, y) = bei(&s, &cam, id, 7000.0, z1 - 5.0);
        e.handle(
            &Event::MouseMove { x, y, mods },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        assert!(
            !e.aussen_schloesser(&s).is_empty(),
            "Schlösser an Ober- und Unterkante"
        );
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        assert!(e.handle(&down, &mut s, &cam, W, H, 1.0, true).consumed);
        assert!(e.hoehe.is_some());
        assert_eq!(e.thema(), crate::help::Topic::Openings);
        let (x, y) = bei(&s, &cam, id, 7000.0, z_og + 30.0);
        e.handle(
            &Event::MouseMove { x, y, mods },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        assert_eq!(s.model().opening(id).unwrap().top, sturz_og);
        let pille = e.pill(&s).unwrap().0;
        assert!(
            pille.starts_with("Höhe ") && pille.ends_with("Sturz OG"),
            "{pille}"
        );
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        e.handle(&up, &mut s, &cam, W, H, 1.0, true);
        assert!(e.hoehe.is_none());
        assert_eq!(s.undo_label(), Some(SCHRITT));
        assert_eq!(band(&s, id), (z0, z_og));
        // Das Loch geht durch beide Wände
        assert!(s
            .model()
            .opening_fault(s.model().opening(id).unwrap(), Some(id))
            .is_none());
        // Lösen: frei auf derselben Höhe im OG, dann wieder gebunden
        let frei = umbinden(s.model(), id, true).unwrap();
        assert!(matches!(frei.top, HeightRef::Level(l) if l.storey == st_og));
        assert!(s.opening_schritt(id, frei, "Höhe gelöst"));
        assert_eq!(band(&s, id).1, z_og);
        let t = Treffer {
            id,
            schenkel: 0,
            kante: None,
        };
        let oben = schloesser(&s, t)
            .into_iter()
            .find(|x| x.art == Schlossart::Hoehe(true))
            .unwrap();
        assert!(!oben.linked, "frei auf der Linie: Schloss offen");
        let wieder = umbinden(s.model(), id, true).unwrap();
        assert_eq!(wieder.top, sturz_og);
    }

    /// Frei gezogen steht die Unterkante im Raster über OK Rohdecke; Zahl +
    /// Das Schloss an OK Rohdecke löst die Kante und bindet sie wieder, im
    /// untersten Geschoss wie darüber (Review W43 B3).
    #[test]
    fn schloss_an_rohdecke() {
        let (mut s, cam, id, eg, og) = haus();
        for run in [eg, og] {
            let st = s.model().run(run).unwrap().storey;
            let unten = HeightRef::Level(LevelRef::bottom(st));
            let mut o = s.model().opening(id).unwrap().clone();
            (o.bottom, o.top) = (unten, HeightRef::Line(st, HeightLine::Lintel));
            assert!(s.opening_schritt(id, o, "Höhe gebunden"));
            let z = s.model().height_z(unten).unwrap();
            let t = treffer(
                &s,
                &cam,
                bei(&s, &cam, id, 7000.0, z + 5.0),
                W,
                H,
                1.0,
                None,
            );
            let t = t.expect("Unterkante von außen");
            let schloss = |s: &Scene| {
                schloesser(s, t)
                    .into_iter()
                    .find(|c| c.art == Schlossart::Hoehe(false))
                    .map(|c| c.linked)
            };
            assert_eq!(schloss(&s), Some(true));
            let frei = umbinden(s.model(), id, false).unwrap();
            assert_eq!(bindung_name(s.model(), frei.bottom), None);
            assert!((s.model().height_z(frei.bottom).unwrap() - z).abs() < 1e-6);
            assert!(s.opening_schritt(id, frei, "Höhe gelöst"));
            assert_eq!(schloss(&s), Some(false));
            let wieder = umbinden(s.model(), id, false).unwrap();
            assert_eq!(wieder.bottom, unten);
        }
    }

    /// Enter setzt die Höhe genau, zu niedrig gilt Enter nicht.
    #[test]
    fn hoehe_frei_und_getippt() {
        let (mut s, cam, id, eg, _) = haus();
        let (z0, z1) = band(&s, id);
        let st = s.model().run(eg).unwrap().storey;
        let elev = s.model().storey(st).unwrap().elevation;
        let t = treffer(
            &s,
            &cam,
            bei(&s, &cam, id, 7000.0, z0 + 5.0),
            W,
            H,
            1.0,
            None,
        )
        .unwrap();
        assert_eq!(t.kante, Some(Kante::Unten));
        let p = bei(&s, &cam, id, 7000.0, z0 + 5.0);
        let mut z = HoeheZug::greifen(&mut s, t, false, &cam, p, W, H, 1.0).unwrap();
        let mods = Default::default();
        let (x, y) = bei(&s, &cam, id, 7000.0, z0 - 333.0);
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let o = s.model().opening(id).unwrap();
        let HeightRef::Level(l) = o.bottom else {
            panic!("frei: {:?}", o.bottom);
        };
        assert_eq!(l.storey, st);
        assert_eq!(l.offset % RASTER, 0.0);
        assert!((elev + l.offset - (z0 - 333.0)).abs() <= RASTER / 2.0 + 1e-6);
        let taste = |z: &mut HoeheZug, s: &mut Scene, key| {
            let e = Event::Key {
                key,
                down: true,
                mods,
                repeat: false,
            };
            z.handle(&e, s, &cam, W, H)
        };
        taste(&mut z, &mut s, Key::Char('0'));
        taste(&mut z, &mut s, Key::Char(','));
        taste(&mut z, &mut s, Key::Char('0'));
        taste(&mut z, &mut s, Key::Char('5'));
        assert!(z.input.as_ref().unwrap().error.is_some());
        assert!(!taste(&mut z, &mut s, Key::Enter).fertig);
        taste(&mut z, &mut s, Key::Escape);
        for ch in "1,5".chars() {
            taste(&mut z, &mut s, Key::Char(ch));
        }
        assert_eq!(z.pill(&s), Some(("Höhe 1,5 m".into(), true)));
        assert!(taste(&mut z, &mut s, Key::Enter).fertig);
        assert!((band(&s, id).0 - (z1 - 1500.0)).abs() < 1e-6);
        assert_eq!(band(&s, id).1, z1);
    }

    /// Die Seitenkante von außen ändert die Breite im Raster, wie der Griff
    /// im Grundriss.
    #[test]
    fn breite_von_aussen() {
        let (mut s, cam, id, ..) = haus();
        let (z0, z1) = band(&s, id);
        let mitte = (z0 + z1) / 2.0;
        let o = s.model().opening(id).unwrap().clone();
        let rechts = usize::from(o.path[1].x > o.path[0].x);
        let mut z = BreiteZug::greifen_aussen(&mut s, id, rechts).unwrap();
        let mods = Default::default();
        let (x, y) = bei(&s, &cam, id, 8003.0, mitte);
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let b = s.model().opening(id).unwrap().width();
        assert!((b - 1500.0).abs() < 1e-6, "{b}");
        assert_eq!(z.pill(&s), Some(("Breite 1,50 m".into(), false)));
    }
}

//! Öffnung von außen ändern (Paket O7, Jörn 13.10. 01:57): In 3D und den
//! vier Ansichten zeigt eine Öffnung unter der Maus ihre Kanten als
//! violettes Band, ein Klick in sie wählt sie. Die Kante bei der Maus
//! greift: seitlich ändert sie die Breite wie der Griff im Grundriss
//! ([`super::breite_edit::BreiteZug::greifen_aussen`], mit Raster, Spur und
//! Breitenschloss), oben und unten die Höhe ([`HoeheZug`]).
//!
//! Die Höhe fängt an Brüstung, Sturz und OK Rohdecke des eigenen Geschosses
//! und des Geschosses darüber und bindet dort; die Kante wandert mit, wenn
//! die Linie im Geschossmanager gezogen wird. So geht ein Fenster über zwei
//! Geschosse, die Decke dazwischen wird ausgespart; wo das Modell sie nicht
//! aussparen kann, durch die Decke unter ihrem Geschoss und durch eine
//! weitere darüber geht die Kante nicht ([`decke_grenzen`]). Frei steht die
//! Kante im Raster von 10 mm als Maß über OK Rohdecke des Geschosses, in dem sie
//! liegt. Das Schloss neben Ober- und Unterkante zeigt die Bindung; ein
//! Klick löst sie (die Höhe bleibt) oder bindet die Kante an die Linie, auf
//! der sie gerade steht. Eine getippte Zahl ist oben die Höhe der Öffnung,
//! unten die Brüstung über OK Rohdecke; gleich nach dem Loslassen ersetzt
//! sie mit Enter den Schritt ([`HoeheNach`], Bedienbarkeit 40.7). Teil von
//! [`crate::wall_edit`].

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
use sk_model::{ElementId, FloorSlab, LevelEdge, LevelKind, LevelRef, Model, RunId, StoreyId};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Öffnungshöhe geändert";
/// Feldname der Eingabe an der Oberkante: die Höhe der Öffnung.
pub const LABEL: &str = "Höhe";
/// Feldname an der Unterkante: die Brüstung über OK Rohdecke
/// (Bedienbarkeit 40.7).
pub const LABEL_UNTEN: &str = "Brüstung";
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

/// Millimeter je Bildpunkt bei `p`, senkrecht gemessen (Fang beim Ziehen
/// der Höhe).
fn mm_je_px(cam: &Camera, p: Vec3, w: f64, h: f64) -> Option<f64> {
    let a = cam.project(p, w, h)?;
    let b = cam.project(p + vec3(0.0, 0.0, 100.0), w, h)?;
    let n = (a.0 - b.0).hypot(a.1 - b.1);
    (n > 1e-6).then(|| 100.0 / n)
}

/// Millimeter je Bildpunkt quer zum Blick in der Tiefe von `p`: so weit
/// liegt dort ein Punkt vom Blickstrahl, der im Bild einen Bildpunkt neben
/// der Maus liegt, in jeder Richtung gleich. Von oben wird eine senkrechte
/// Strecke beliebig kurz, ihr Maß taugt dafür nicht (Review B1 zu #151).
fn mm_quer(cam: &Camera, p: Vec3, h: f64) -> f64 {
    let s = match cam.ortho {
        Some(half) => half,
        None => (p - cam.eye).dot(cam.forward()).max(cam.near()) * (cam.fov_y * 0.5).tan(),
    };
    2.0 * s / h
}

/// Wie weit (mm) die Maus an einem Punkt neben dem Blickstrahl greift:
/// [`GRIFF_PX`] Bildpunkte in seiner Tiefe.
pub(crate) fn griff(cam: &Camera, h: f64, scale: f64) -> impl Fn(Vec3) -> f64 + '_ {
    move |p| GRIFF_PX * scale * mm_quer(cam, p, h)
}

/// Abstand (Bildpunkte) der Maus `m` vom Bild der Strecke `a`–`b`, so weit
/// sie vor der Nahebene liegt (Review H1 zu #151); `None`, wenn nichts
/// davon.
fn strecke_px(cam: &Camera, w: f64, h: f64, m: (f64, f64), a: Vec3, b: Vec3) -> Option<f64> {
    let tiefe = |p: Vec3| (p - cam.eye).dot(cam.forward());
    let n = cam.near() * 1.001;
    let (ta, tb) = (tiefe(a), tiefe(b));
    let (a, b) = match (ta > n, tb > n) {
        (true, true) => (a, b),
        (false, false) => return None,
        (true, false) => (a, a + (b - a) * ((ta - n) / (ta - tb))),
        (false, true) => (b + (a - b) * ((tb - n) / (tb - ta)), b),
    };
    let (a, b) = (cam.project(a, w, h)?, cam.project(b, w, h)?);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t =
        (((m.0 - a.0) * dx + (m.1 - a.1) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0.0, 1.0);
    Some((m.0 - a.0 - t * dx).hypot(m.1 - a.1 - t * dy))
}

/// Abstand (Bildpunkte) der Maus `m` vom Bild der vier Seiten des Bands
/// auf dem Schenkel: unten, oben, am Anfang, am Ende.
fn seiten_px(
    cam: &Camera,
    w: f64,
    h: f64,
    m: (f64, f64),
    s: &crate::scene::kanten::Schenkel,
) -> [Option<f64>; 4] {
    let at = |e: Vec3, z: f64| vec3(e.x, e.y, z);
    let seite = |a, b| strecke_px(cam, w, h, m, a, b);
    [
        seite(at(s.a, s.z0), at(s.b, s.z0)),
        seite(at(s.a, s.z1), at(s.b, s.z1)),
        seite(at(s.a, s.z0), at(s.a, s.z1)),
        seite(at(s.b, s.z0), at(s.b, s.z1)),
    ]
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
    // Nur die Schenkel in den Zellen, die der Strahl kreuzt, und so weit
    // daneben, wie die Maus greift; die gehaltene Öffnung immer (W43 B1,
    // Review H1 zu #147)
    let (_, wand) = scene.kanten_strahl(o, d, griff(cam, h, scale), halten, |s| {
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
        // In der Öffnung trifft er; daneben zählt nur der Abstand im Bild,
        // in der Ebene gemessen täuscht er: schräg von oben liegt der
        // Schnitt unter dem Boden, von oben entartet das Maß (perf_kanten
        // und Review B1 zu #151)
        let p = o + d * t;
        let (u, z) = ((p - s.a).dot(dir), p.z);
        let innen = (0.0..=len).contains(&u) && (s.z0..=s.z1).contains(&z);
        let px = if halten == Some(s.id) {
            HALTEN_PX
        } else {
            GRIFF_PX
        } * scale;
        // Liegt schon die Kugel um das Band weiter vom Strahl, als die Maus
        // in ihrer größten Tiefe greift, liegt es im Bild weiter weg
        let mitte = vec3(
            (s.a.x + s.b.x) * 0.5,
            (s.a.y + s.b.y) * 0.5,
            (s.z0 + s.z1) * 0.5,
        );
        let r = 0.5 * len.hypot(s.z1 - s.z0);
        let v = mitte - o;
        let quer = (v - d * v.dot(d)).length();
        if !innen && quer - r > px * mm_quer(cam, mitte + cam.forward() * r, h) {
            debug_assert!(seiten_px(cam, w, h, (x, y), s)
                .iter()
                .flatten()
                .all(|&a| a > px));
            return None;
        }
        let seiten = seiten_px(cam, w, h, (x, y), s);
        let nah = seiten
            .iter()
            .flatten()
            .fold(f64::INFINITY, |a, &b| a.min(b));
        if !innen && nah > px {
            return None;
        }
        // Die nächste Kante bis [`GRIFF_PX`] im Bild
        let [unten, oben, anfang, ende] = seiten;
        let mut kanten = vec![(unten, Kante::Unten), (oben, Kante::Oben)];
        if s.k == 0 {
            kanten.push((anfang, Kante::Ende(0)));
        }
        if s.k + 1 == s.n {
            kanten.push((ende, Kante::Ende(s.n)));
        }
        let kante = kanten
            .into_iter()
            .filter_map(|(a, k)| Some((a.filter(|&a| a <= GRIFF_PX * scale)?, k)))
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

/// Wandzug der Außenfläche, auf der die Öffnung `id` steht.
pub fn zug(scene: &Scene, id: ElementId) -> Option<RunId> {
    let m = scene.model();
    let (o, st) = (m.opening(id)?, m.element(id)?.storey);
    let koerper = |r| scene.koerper(r);
    super::breite_edit::flaeche_bei(m, &koerper, st, *o.path.first()?).map(|f| f.0.run)
}

/// Spielraum einer Kante der Öffnung `id` am Zug `run`: von OK der Decke
/// darunter bis UK der Decke darüber. Spart das Modell die Decke darüber
/// für die Öffnung aus ([`Model::spart_aus`], Jörn 13.10.: „Aussparen“),
/// reicht er bis UK der nächsten Decke; bei einem Vorsprung der Wand
/// darüber oder ohne Kern am Deckenrand nicht (Geometrie). Dass die Kante
/// nicht durch die Decke unter ihrem Geschoss geht, ist eine Vorgabe der
/// Bedienung: Ein Fenster über zwei Geschosse wird vom unteren aus nach
/// oben aufgezogen. Das Modell spart auch eine Decke unter dem Geschoss der
/// Öffnung aus, wenn ihr Loch hineinreicht.
pub fn decke_grenzen(scene: &Scene, run: Option<RunId>, id: ElementId) -> (f64, f64) {
    let m = scene.model();
    let band = |r: RunId| mit_decke(scene, r, |f| f.band());
    let uk = |r: RunId| band(r).map(|b| b.0);
    // Ohne gültige Decke darüber bleibt die eigene die Grenze (Review W48 H9)
    let oben = match run {
        Some(r) if mit_decke(scene, r, |f| m.spart_aus_bei(r, id, f)) == Some(true) => m
            .runs_above(r)
            .first()
            .and_then(|&u| uk(u))
            .or_else(|| uk(r)),
        _ => run.and_then(uk),
    }
    .unwrap_or(f64::INFINITY);
    let unten = run
        .and_then(|r| m.run_below(r))
        .and_then(band)
        .map_or(f64::NEG_INFINITY, |b| b.1);
    (unten, oben)
}

/// Warum eine freie Kante an der Decke stehen bleibt, für die Pille
/// (Bedienbarkeit 41.3): unten, weil ein Fenster über zwei Geschosse vom
/// unteren Geschoss aus aufgezogen wird; oben, weil darüber kein Geschoss
/// steht oder die Wand darüber vorspringt. `None`, wenn die Grenze schon die
/// nächste Decke ist oder kein Grund bekannt ist.
pub fn decke_grund(scene: &Scene, run: Option<RunId>, id: ElementId, oben: bool) -> Option<String> {
    let m = scene.model();
    let r = run?;
    if !oben {
        let unter = m.run(m.run_below(r)?)?.storey;
        return Some(format!(
            "vom {} aus nach oben ziehen",
            m.storey(unter)?.short
        ));
    }
    let (spart, buendig) = mit_decke(scene, r, |f| (m.spart_aus_bei(r, id, f), f.buendig.clone()))?;
    if spart {
        None
    } else if buendig.is_empty() {
        Some("kein Geschoss darüber".into())
    } else if buendig.contains(&false) {
        Some("Wand darüber springt vor".into())
    } else {
        None
    }
}

/// `f` zur Decke über `run`: aus dem Cache der Szene, sonst gebaut (Review
/// W48 B3). `None` ohne Decke.
fn mit_decke<T>(scene: &Scene, run: RunId, f: impl FnOnce(&FloorSlab) -> T) -> Option<T> {
    if let Some(fs) = scene.floor(run) {
        return Some(f(fs));
    }
    let m = scene.model();
    m.floor_of(run)?;
    m.floor(run)?.ok().map(|fs| f(&fs))
}

/// Höhenlinien, an denen eine Kante der Öffnung fängt: Brüstung, Sturz und
/// OK Rohdecke des Geschosses `st` und der Geschosse direkt darüber und
/// darunter, soweit keine Decke dazwischen liegt (`grenzen`, aus
/// [`decke_grenzen`]).
pub fn linien(m: &Model, st: StoreyId, (lo, hi): (f64, f64)) -> Vec<FaceLine> {
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
    out.retain(|l| l.z >= lo - 1e-6 && l.z <= hi + 1e-6);
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

/// Steht die Kante auf OK Rohdecke ihres Geschosses? Gebunden und frei
/// sind dort dasselbe: Sie geht mit dem Boden, darum gibt es dort kein
/// Schloss (Review W43 B3).
fn am_boden(r: HeightRef) -> bool {
    matches!(r, HeightRef::Level(l) if l.edge == LevelEdge::Bottom && l.offset == 0.0)
}

/// Freie Höhe `z` (genau) als Maß über OK Rohdecke des Geschosses, in dem
/// sie liegt; so behält die Kante ihren Abstand zum Boden, wenn sich eine
/// Geschosshöhe ändert. Genau auf OK Rohdecke ist das die Bindung dort
/// ([`am_boden`]).
fn frei_bei(m: &Model, st: StoreyId, z: f64) -> LevelRef {
    let alle = geschosse(m, st);
    let (storey, e) = alle
        .iter()
        .rev()
        .find(|g| g.1 <= z + 1e-6)
        .or(alle.first())
        .copied()
        .unwrap_or((st, m.storey(st).map_or(0.0, |s| s.elevation)));
    LevelRef {
        storey,
        edge: LevelEdge::Bottom,
        offset: if (z - e).abs() > 1e-6 { z - e } else { 0.0 },
    }
}

/// Die Öffnung `id` mit gelöster bzw. gebundener Ober- (`oben`) oder
/// Unterkante: gebunden wird sie frei auf ihrer Höhe; frei bindet sie an
/// die Höhenlinie, auf der sie steht. `None`, wenn es keine solche gibt
/// oder die Kante auf OK Rohdecke steht ([`am_boden`]).
pub fn umbinden(scene: &Scene, id: ElementId, oben: bool) -> Option<Opening> {
    let m = scene.model();
    let mut o = m.opening(id)?.clone();
    let st = m.element(id)?.storey;
    let r = if oben { o.top } else { o.bottom };
    if am_boden(r) {
        return None;
    }
    let z = m.height_z(r)?;
    let neu = if bindung_name(m, r).is_some() {
        HeightRef::Level(frei_bei(m, st, z))
    } else {
        linien(m, st, decke_grenzen(scene, zug(scene, id), id))
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
/// steht, nicht auf OK Rohdecke ([`am_boden`]).
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
    let linien = linien(m, st, decke_grenzen(scene, zug(scene, t.id), t.id));
    for (oben, r, z) in [(false, o.bottom, z0), (true, o.top, z1)] {
        let gebunden = bindung_name(m, r).is_some();
        let auf_linie = linien
            .iter()
            .any(|l| (l.z - z).abs() < 0.5 && !am_boden(l.bind));
        if am_boden(r) || (!gebunden && !auf_linie) {
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
    /// Spielraum zwischen den Decken ([`decke_grenzen`]).
    grenzen: (f64, f64),
    /// Warum die Kante an der Decke stehen bleibt ([`decke_grund`]).
    grund: Option<String>,
    /// Die Maus steht jenseits der Decke (dann nennt die Pille den Grund).
    drueber: bool,
    /// Geschoss der Öffnung.
    st: StoreyId,
    /// Höhe der anderen Kante.
    gegen: f64,
    /// Fangabstand (mm).
    tol: f64,
    /// Ein Zug am Pfad (er wird beim Ziehen live gezeichnet).
    run: Option<sk_model::RunId>,
    geaendert: bool,
    /// Beim Schließen als Schritt übernommen (dann folgt die Nachkorrektur).
    fest: bool,
    /// Getippte Zahl; solange sie offen ist, ruht die Maus.
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
        // Ohne Maßstab am Treffpunkt der am Schenkelanfang (Review W43 H4)
        let mm = mm_je_px(cam, p, w, h).or_else(|| mm_je_px(cam, a, w, h));
        let tol = FANG_PX * scale * mm.unwrap_or(0.0);
        let run = zug(scene, t.id);
        let grenzen = decke_grenzen(scene, run, t.id);
        let grund = decke_grund(scene, run, t.id, oben);
        let linien = linien(m, st, grenzen);
        scene.begin(SCHRITT);
        Some(HoeheZug {
            id: t.id,
            oben,
            original,
            a,
            nrm,
            linien,
            grenzen,
            grund,
            drueber: false,
            st,
            gegen: if oben { z0 } else { z1 },
            tol,
            run,
            geaendert: false,
            fest: false,
            input: None,
            maus: None,
        })
    }

    /// Feldname der Eingabe: oben die Höhe, unten die Brüstung.
    pub fn label(&self) -> &'static str {
        if self.oben {
            LABEL
        } else {
            LABEL_UNTEN
        }
    }

    /// Nach dem Schließen als Schritt: die Nachkorrektur dazu.
    pub fn nach(self, scene: &Scene) -> Option<HoeheNach> {
        self.fest.then(|| HoeheNach {
            serial: scene.step_serial(),
            zug: HoeheZug {
                input: None,
                ..self
            },
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
        // Frei: nicht durch die Decke, nicht niedriger als die Mindesthöhe
        let (lo, hi) = self.grenzen;
        let grenze = if self.oben {
            z.min(hi).max(self.gegen + MIN_OPENING)
        } else {
            z.max(lo).min(self.gegen - MIN_OPENING)
        };
        let mut l = frei_bei(m, self.st, grenze);
        l.offset = (l.offset / RASTER).round() * RASTER;
        let z_neu = m.level_z(l).unwrap_or(grenze);
        if !self.hoch_genug(z_neu) {
            l.offset += if self.oben { RASTER } else { -RASTER };
        } else if z_neu > hi + 1e-6 {
            l.offset -= RASTER;
        } else if z_neu < lo - 1e-6 {
            l.offset += RASTER;
        }
        HeightRef::Level(l)
    }

    /// Setzt die Kante auf `r`; `None`, wenn das Modell ablehnt (der letzte
    /// Stand bleibt), sonst ob sich etwas geändert hat.
    fn setzen(&mut self, scene: &mut Scene, r: HeightRef) -> Option<bool> {
        let o = self.mit(r);
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
        let (lo, hi) = self.grenzen;
        self.drueber = if self.oben {
            z > hi + self.tol
        } else {
            z < lo - self.tol
        };
        let r = self.bindung(scene.model(), z);
        self.setzen(scene, r) == Some(true)
    }

    /// Kante für die getippte Zahl `v`, frei: oben die Höhe der Öffnung,
    /// unten die Brüstung über OK Rohdecke ihres Geschosses (Bedienbarkeit
    /// 40.7). `Err` mit dem Grund, wenn sie nicht passt.
    fn ziel(&self, m: &Model, v: f64) -> Result<HeightRef, Meldung> {
        let passt = || Meldung::satz("So hoch passt die Öffnung nicht in die Wand");
        let z = if self.oben {
            self.gegen + v
        } else {
            m.level_z(LevelRef::bottom(self.st)).ok_or_else(passt)? + v
        };
        if !self.hoch_genug(z) {
            return Err(Meldung::satz("Eine Öffnung ist mindestens 10 cm hoch"));
        }
        let (lo, hi) = self.grenzen;
        if z < lo - 1e-6 || z > hi + 1e-6 {
            return Err(passt());
        }
        Ok(HeightRef::Level(frei_bei(m, self.st, z)))
    }

    /// Die Öffnung mit der gezogenen Kante auf `r`.
    fn mit(&self, r: HeightRef) -> Opening {
        let mut o = self.original.clone();
        if self.oben {
            o.top = r;
        } else {
            o.bottom = r;
        }
        o
    }

    /// Getippte Zahl: die gezogene Kante genau dort, frei. Passt sie
    /// nicht, wird die Eingabe rot und Enter gilt nicht.
    fn getippt(&mut self, scene: &mut Scene) -> bool {
        let Some(Some(Ok(v))) = self.input.as_ref().map(|i| i.value(0)) else {
            return false;
        };
        let fehler = match self.ziel(scene.model(), v) {
            Ok(r) => match self.setzen(scene, r) {
                Some(geaendert) => return geaendert,
                None => Meldung::satz("So hoch passt die Öffnung nicht in die Wand"),
            },
            Err(f) => f,
        };
        if let Some(i) = self.input.as_mut() {
            i.error = Some(fehler);
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
        self.fest = !zurueck;
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

    /// Pille am Cursor: die Eingabe, sonst die Zahl, die man tippen würde
    /// (oben die Höhe, unten die Brüstung), und die Bindung der Kante.
    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        let label = self.label();
        if let Some(i) = &self.input {
            return Some((i.text([label, ""]), true));
        }
        let m = scene.model();
        let o = m.opening(self.id)?;
        let (z0, z1) = m.opening_band(o)?;
        let wert = if self.oben {
            z1 - z0
        } else {
            z0 - m.level_z(LevelRef::bottom(self.st))?
        };
        let hoehe = crate::ui::live_length_text(wert);
        let r = if self.oben { o.top } else { o.bottom };
        // Frei an der Decke: sagen, warum die Kante dort stehen bleibt
        // (Einstellungen zu #136), mit dem Grund (Bedienbarkeit 41.3)
        let (z, grenze, an) = if self.oben {
            (z1, self.grenzen.1, "bis unter die Decke")
        } else {
            (z0, self.grenzen.0, "bis auf die Decke")
        };
        let an_der_decke = grenze.is_finite() && (z - grenze).abs() < RASTER + 1e-6;
        let mut text = match bindung_name(m, r) {
            Some(b) => format!("{label} {hoehe} · {b}"),
            None if an_der_decke => format!("{label} {hoehe} · {an}"),
            None => format!("{label} {hoehe}"),
        };
        // Zieht die Maus über die Decke hinaus, auch gebunden
        if let Some(g) = self.grund.as_ref().filter(|_| an_der_decke && self.drueber) {
            text = format!("{text} · {g}");
        }
        Some((text, false))
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

/// Nachkorrektur nach dem Loslassen (Bedienbarkeit 40.7): Wie beim
/// Wandziehen ersetzt eine getippte Zahl mit Enter den letzten Schritt,
/// solange kein anderer dazwischen kam. Die App blendet die Pille nach
/// 1,5 s aus und beendet sie damit.
pub struct HoeheNach {
    zug: HoeheZug,
    serial: u64,
}

impl HoeheNach {
    /// Kam kein anderer Schritt dazwischen, nichts wiederherzustellen?
    pub fn gilt(&self, scene: &Scene) -> bool {
        self.serial == scene.step_serial() && scene.redo_label().is_none()
    }

    pub fn input(&self) -> Option<&MeasureInput> {
        self.zug.input.as_ref()
    }

    pub fn label(&self) -> &'static str {
        self.zug.label()
    }

    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        self.zug.pill(scene)
    }

    /// Prüft die getippte Zahl, ohne etwas zu ändern: rot, wenn sie nicht
    /// passt; sonst die neue Öffnung.
    fn pruefen(&mut self, m: &Model) -> Option<Opening> {
        let Some(Ok(v)) = self.zug.input.as_ref()?.value(0) else {
            return None;
        };
        let o = self.zug.ziel(m, v).and_then(|r| {
            let o = self.zug.mit(r);
            match m.opening_fault(&o, Some(self.zug.id)) {
                None => Ok(o),
                Some(_) => Err(Meldung::satz("So hoch passt die Öffnung nicht in die Wand")),
            }
        });
        match o {
            Ok(o) => Some(o),
            Err(f) => {
                if let Some(i) = self.zug.input.as_mut() {
                    i.error = Some(f);
                }
                None
            }
        }
    }

    /// Taste in der Nachkorrektur; `None`: nicht für sie, sie endet.
    /// Sonst ob sich das Modell geändert hat.
    pub fn taste(&mut self, key: Key, mods: Modifiers, scene: &mut Scene) -> Option<bool> {
        let Some(i) = self.zug.input.as_mut() else {
            let Key::Char(ch) = key else {
                return None;
            };
            if !opens(ch, MeasureKind::Length, mods) {
                return None;
            }
            let mut i = MeasureInput::new(MeasureKind::Length, None);
            i.push(ch);
            self.zug.input = Some(i);
            self.pruefen(scene.model());
            return Some(false);
        };
        match i.key(key, mods) {
            InputOutcome::Ignored | InputOutcome::Escape | InputOutcome::EnterEmpty => None,
            InputOutcome::Changed | InputOutcome::Refused => {
                self.pruefen(scene.model());
                Some(false)
            }
            // Leer getippt: neu tippen (wie beim Wandziehen, A306)
            InputOutcome::Emptied => {
                self.zug.input = None;
                Some(false)
            }
            InputOutcome::Enter => {
                let Some(o) = self.pruefen(scene.model()) else {
                    return Some(false);
                };
                scene.undo();
                scene.begin(SCHRITT);
                if !scene.set_opening(self.zug.id, o) {
                    scene.rollback();
                    scene.redo();
                    return Some(false);
                }
                scene.commit();
                scene.clear_redo();
                // Die Pille bleibt stehen, eine neue Ziffer korrigiert erneut
                self.serial = scene.step_serial();
                self.zug.input = None;
                Some(true)
            }
        }
    }
}

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
        let (m, id, eg, og) = haus_modell();
        (Scene::with_model(m), kamera(), id, eg, og)
    }

    /// Segment der Südwand (Außenfläche nach −y).
    pub fn sued(m: &Model, eg: RunId) -> usize {
        (0..4)
            .find(|&k| m.outer_face(eg, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap()
    }

    pub fn kamera() -> Camera {
        Camera::looking_at(
            vec3(7000.0, -16000.0, 3500.0),
            vec3(7000.0, 0.0, 3000.0),
            45.0,
        )
    }

    /// Das Prüfhaus als Modell: Fenster, EG-Zug, OG-Zug.
    pub fn haus_modell() -> (Model, ElementId, RunId, RunId) {
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
        let f = m.outer_face(eg, sued(&m, eg)).unwrap();
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
        (m, id, eg, og)
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

    /// Eine Strecke, die hinter die Nahebene reicht, zählt im Bild mit dem
    /// Stück davor; ganz dahinter gar nicht (Review H1 zu #151: vorher
    /// entfiel dann die Prüfung im Bild).
    #[test]
    fn strecke_hinter_der_nahebene() {
        let cam = Camera::looking_at(vec3(0.0, 0.0, 1000.0), vec3(0.0, 10_000.0, 1000.0), 45.0);
        let (a, b) = (vec3(2000.0, 3000.0, 1000.0), vec3(-2000.0, -3000.0, 1000.0));
        assert!(cam.project(b, W, H).is_none());
        let auf = cam.project(vec3(1000.0, 1500.0, 1000.0), W, H).unwrap();
        assert!(strecke_px(&cam, W, H, auf, a, b).unwrap() < 0.01);
        let neben = (auf.0, auf.1 - 50.0);
        let d = strecke_px(&cam, W, H, neben, a, b).unwrap();
        assert!((40.0..=50.0).contains(&d), "{d}");
        let (c, e) = (vec3(0.0, -100.0, 1000.0), vec3(100.0, -200.0, 1000.0));
        assert_eq!(strecke_px(&cam, W, H, auf, c, e), None);
    }

    /// Übersicht schräg von oben aus 470 m wie in `perf_kanten` bei 1000
    /// Häusern: Am Boden vor der Hauswand liegt der Schnitt des Strahls mit
    /// der Ebene des Fensters Meter unter dem Boden, nach dem senkrechten
    /// Maß in der Ebene noch im Greifabstand, im Bild aber mehr als
    /// [`GRIFF_PX`] daneben: kein Treffer. Näher am Fenster greift die Maus
    /// (Review zu #151).
    #[test]
    fn im_bild_gemessen() {
        let (s, _, id, ..) = haus();
        let ziel = vec3(240_000.0, 240_000.0, 0.0);
        let cam = Camera::looking_at(ziel + vec3(-0.5, -0.9, 0.9) * 480_000.0, ziel, 45.0);
        let o = s.model().opening(id).unwrap();
        let (z0, z1) = band(&s, id);
        let x1 = o.path[0].x.max(o.path[1].x);
        let maus = |g: Vec3| cam.project(g, W, H).unwrap();
        let sch = crate::scene::kanten::Schenkel {
            id,
            k: 0,
            n: 1,
            a: flat(o.path[0]),
            b: flat(o.path[1]),
            z0,
            z1,
        };
        let abstand = |m| {
            seiten_px(&cam, W, H, m, &sch)
                .into_iter()
                .flatten()
                .fold(f64::INFINITY, f64::min)
        };
        // 7 m rechts des Fensters und 2,5 m vor der Wand am Boden
        let fern = maus(vec3(x1 + 7000.0, -2500.0, 0.0));
        let (ro, rd) = cam.ray(fern.0, fern.1, W, H);
        let p = ro + rd * ((o.path[0].y - ro.y) / rd.y);
        let tol = GRIFF_PX * mm_je_px(&cam, p, W, H).unwrap();
        assert!(
            p.z < -3000.0 && p.x - x1 < tol && z0 - p.z < tol,
            "{p:?} {tol}"
        );
        let a = abstand(fern);
        assert!(a > GRIFF_PX, "{a} px");
        assert!(treffer(&s, &cam, fern, W, H, 1.0, None).is_none());
        // Gehalten greift sie weiter
        assert!(a < HALTEN_PX, "{a} px");
        assert_eq!(
            treffer(&s, &cam, fern, W, H, 1.0, Some(id)).map(|t| t.id),
            Some(id)
        );
        // Am Fuß der Wand unter dem Fenster
        let nah = maus(vec3(x1 - 200.0, o.path[0].y, 0.0));
        assert!(abstand(nah) < GRIFF_PX);
        assert_eq!(
            treffer(&s, &cam, nah, W, H, 1.0, None).map(|t| t.id),
            Some(id)
        );
    }

    /// Von weit weg greift die Maus weiter als der feste Rand des Rasters
    /// um die Schenkel: Die gehaltene Öffnung bleibt 50 Bildpunkte neben
    /// ihrer Kante getroffen, Meter außerhalb des Hauses (Review H1 zu #147).
    #[test]
    fn halten_von_weit_weg() {
        let (s, _, id, ..) = haus();
        let cam = Camera::looking_at(
            vec3(7500.0, -96_000.0, 3500.0),
            vec3(7500.0, 0.0, 1500.0),
            45.0,
        );
        let (z0, z1) = band(&s, id);
        let (x, y) = bei(&s, &cam, id, 7500.0, (z0 + z1) / 2.0);
        let p = (x + 50.0, y);
        // So weit rechts der Öffnung, in ihrer Fläche: neben dem Haus
        let (o, d) = cam.ray(p.0, p.1, W, H);
        let fy = s.model().opening(id).unwrap().path[0].y;
        let neben = o.x + d.x * (fy - o.y) / d.y;
        assert!(neben > 11_500.0, "{neben}");
        assert!(treffer(&s, &cam, p, W, H, 1.0, None).is_none());
        assert_eq!(
            treffer(&s, &cam, p, W, H, 1.0, Some(id)).map(|t| t.id),
            Some(id)
        );
    }

    /// Oberkante über das Wandband greifen und bis an den Sturz des OG
    /// ziehen: Sie bindet dort, das Fenster geht über zwei Geschosse und die
    /// Decke dazwischen wird ausgespart, ein Schritt. Das Schloss löst die
    /// Bindung bei gleicher Höhe und bindet sie wieder.
    #[test]
    fn hoehe_ueber_zwei_geschosse() {
        let (mut s, cam, id, eg, og) = haus();
        let st = s.model().run(eg).unwrap().storey;
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
        // Die Decke über dem EG ist ausgespart: Ein Strahl in Deckenhöhe
        // durch das Fenster trifft erst ihre Stirn an der Innenfläche
        let d = s.model().floor(eg).unwrap().unwrap();
        assert_eq!(d.aussparungen.len(), 1, "{:?}", d.aussparungen);
        assert_eq!(d.aussparungen[0].opening, Some(id));
        let (zb, zt) = d.band();
        let strahl = |s: &Scene| {
            s.raycast(vec3(7000.0, -5000.0, (zb + zt) * 0.5), vec3(0.0, 1.0, 0.0))
                .map(|h| h.0)
        };
        let tief = strahl(&s).unwrap();
        assert!(tief > 5000.0 + 200.0, "{tief}");
        // Lösen: frei auf derselben Höhe im OG, dann wieder gebunden
        let frei = umbinden(&s, id, true).unwrap();
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
        let wieder = umbinden(&s, id, true).unwrap();
        assert_eq!(wieder.top, sturz_og);
        // Zurück auf den Sturz EG: Die Decke ist wieder ganz, der Strahl
        // trifft sie vorn
        let mut o = s.model().opening(id).unwrap().clone();
        o.top = HeightRef::Line(st, HeightLine::Lintel);
        assert!(s.opening_schritt(id, o, "Höhe gebunden"));
        assert!(s
            .model()
            .floor(eg)
            .unwrap()
            .unwrap()
            .aussparungen
            .is_empty());
        let vorn = strahl(&s).unwrap();
        assert!(vorn < tief - 100.0, "{vorn} {tief}");
        // Durch die nächste Decke geht sie nicht
        let (unten, oben) = decke_grenzen(&s, Some(eg), id);
        assert!(unten.is_infinite() && oben > z_og, "{unten} {oben}");
    }

    /// Springt die Wand im OG vor, spart das Modell die Decke nicht aus:
    /// Die Oberkante, zum Sturz des OG gezogen, endet frei unter der Decke
    /// und sagt es im Maßfeld; sie fängt an keiner Linie des OG, ein
    /// Schritt. Getippt über die Decke gilt Enter nicht (Geometrie).
    #[test]
    fn vorsprung_haelt_an_der_decke() {
        let (mut m, id, eg, og) = haus_modell();
        assert!(m.spart_aus(eg, id), "ohne Vorsprung: ausgespart");
        let wand = m.wall_at(og, sued(&m, eg)).unwrap();
        m.begin("Versatz");
        assert!(m.set_offset(wand, 600.0).is_some());
        m.commit().unwrap();
        assert!(!m.spart_aus(eg, id));
        let (mut s, cam) = (Scene::with_model(m), kamera());
        let z0 = band(&s, id).0;
        let st_og = s.model().run(og).unwrap().storey;
        let sturz_og = HeightRef::Line(st_og, HeightLine::Lintel);
        let z_og = s.model().height_z(sturz_og).unwrap();
        let (unten, decke) = decke_grenzen(&s, Some(eg), id);
        assert!(unten.is_infinite() && decke < z_og, "{unten} {decke}");
        assert_eq!(zug(&s, id), Some(eg));
        let st = s.model().run(eg).unwrap().storey;
        assert!(linien(s.model(), st, (unten, decke))
            .iter()
            .all(|l| l.z <= decke && l.bind != sturz_og));
        let mut e = crate::wall_edit::WallEdit::default();
        let mods = Default::default();
        let (x, y) = bei(&s, &cam, id, 7000.0, band(&s, id).1 - 5.0);
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
        let top = s.model().opening(id).unwrap().top;
        assert!(
            matches!(top, HeightRef::Level(l) if l.storey == st),
            "{top:?}"
        );
        let z1 = band(&s, id).1;
        assert!(
            z1 <= decke + 1e-6 && z1 > decke - RASTER - 1e-6,
            "{z1} {decke}"
        );
        let pille = e.pill(&s).unwrap().0;
        assert!(
            !pille.contains("OG")
                && pille.ends_with("bis unter die Decke · Wand darüber springt vor"),
            "{pille}"
        );
        // Getippt über die Decke: rot, Enter gilt nicht
        for c in "4".chars().chain(std::iter::once(',')).chain("0".chars()) {
            let k = Event::Key {
                key: Key::Char(c),
                down: true,
                mods,
                repeat: false,
            };
            e.handle(&k, &mut s, &cam, W, H, 1.0, true);
        }
        let enter = Event::Key {
            key: Key::Enter,
            down: true,
            mods,
            repeat: false,
        };
        e.handle(&enter, &mut s, &cam, W, H, 1.0, true);
        assert!(band(&s, id).1 <= decke + 1e-6, "nicht durch die Decke");
        assert!(e.hoehe.is_some(), "Enter gilt nicht");
        e.handle(
            &Event::Key {
                key: Key::Escape,
                down: true,
                mods,
                repeat: false,
            },
            &mut s,
            &cam,
            W,
            H,
            1.0,
            true,
        );
        assert!(band(&s, id).0 == z0);
    }

    /// An OK Rohdecke gibt es kein Schloss, im untersten Geschoss wie
    /// darüber: Gebunden und frei wären dort dasselbe (Review W43 B3).
    #[test]
    fn rohdecke_ohne_schloss() {
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
            let hoehe: Vec<_> = schloesser(&s, t)
                .into_iter()
                .filter(|c| matches!(c.art, Schlossart::Hoehe(_)))
                .map(|c| c.art)
                .collect();
            assert_eq!(hoehe, [Schlossart::Hoehe(true)], "nur am Sturz");
            assert!(umbinden(&s, id, false).is_none());
            // Frei auf genau diese Höhe gezogen: dasselbe wie gebunden
            assert_eq!(HeightRef::Level(frei_bei(s.model(), st, z)), unten);
        }
    }

    /// Ändert sich eine Geschosshöhe um ±200 mm, behält eine gelöste Kante
    /// ihren Abstand zum Boden ihres Geschosses, die an OK Rohdecke
    /// gebundene bleibt auf ihm (Review W43 B3).
    #[test]
    fn geschosshoehe_haelt_den_abstand() {
        let (mut s, _, id, eg, og) = haus();
        let (st, st_og) = (
            s.model().run(eg).unwrap().storey,
            s.model().run(og).unwrap().storey,
        );
        // Unterkante an der Brüstung OG gebunden, dann gelöst
        let mut o = s.model().opening(id).unwrap().clone();
        (o.bottom, o.top) = (
            HeightRef::Line(st_og, HeightLine::Sill),
            HeightRef::Line(st_og, HeightLine::Lintel),
        );
        assert!(s.opening_schritt(id, o, "Höhe gebunden"));
        let frei = umbinden(&s, id, false).unwrap();
        assert!(matches!(frei.bottom, HeightRef::Level(l) if l.storey == st_og && l.offset > 0.0));
        assert!(s.opening_schritt(id, frei, "Höhe gelöst"));
        let boden = |s: &Scene| s.model().storey(st_og).unwrap().elevation;
        let ueber = |s: &Scene| band(s, id).0 - boden(s);
        let a = ueber(&s);
        for (geschoss, d) in [(st, 200.0), (st, -200.0), (st_og, 200.0), (st_og, -200.0)] {
            let h = s.model().storey(geschoss).unwrap().height;
            assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(geschoss, h + d)));
            assert!((ueber(&s) - a).abs() < 1e-6, "gelöst {geschoss:?} {d}");
            assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(geschoss, h)));
        }
        // An OK Rohdecke OG gebunden: geht mit dem Boden
        let mut o = s.model().opening(id).unwrap().clone();
        o.bottom = HeightRef::Level(LevelRef::bottom(st_og));
        assert!(s.opening_schritt(id, o, "Höhe gebunden"));
        for d in [200.0, -200.0] {
            let h = s.model().storey(st).unwrap().height;
            assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(st, h + d)));
            assert!(ueber(&s).abs() < 1e-6, "gebunden {d}");
            assert!(s.edit_model("Geschosshöhe", |m| m.set_storey_height(st, h)));
        }
    }

    /// Eine Öffnung im OG: Die Unterkante, nach unten bis ins EG gezogen,
    /// endet auf der Decke darunter und fängt an keiner Linie des EG
    /// (Review H5).
    #[test]
    fn unterkante_endet_auf_der_decke() {
        let (mut s, cam, id, eg, og) = haus();
        let st_og = s.model().run(og).unwrap().storey;
        let mut o = s.model().opening(id).unwrap().clone();
        (o.bottom, o.top) = (
            HeightRef::Line(st_og, HeightLine::Sill),
            HeightRef::Line(st_og, HeightLine::Lintel),
        );
        assert!(s.opening_schritt(id, o, "Höhe gebunden"));
        assert_eq!(zug(&s, id), Some(og));
        let (decke, oben) = decke_grenzen(&s, Some(og), id);
        let ok = s.model().floor(eg).unwrap().unwrap().band().1;
        assert_eq!(decke, ok);
        assert!(oben > decke);
        let z0 = band(&s, id).0;
        let p = bei(&s, &cam, id, 7000.0, z0 + 5.0);
        let t = treffer(&s, &cam, p, W, H, 1.0, None).unwrap();
        assert_eq!(t.kante, Some(Kante::Unten));
        let mut z = HoeheZug::greifen(&mut s, t, false, &cam, p, W, H, 1.0).unwrap();
        let mods = Default::default();
        let (x, y) = bei(&s, &cam, id, 7000.0, 1000.0);
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let unten = s.model().opening(id).unwrap().bottom;
        assert!(
            matches!(unten, HeightRef::Level(l) if l.storey == st_og),
            "{unten:?}"
        );
        let z1 = band(&s, id).0;
        assert!(
            z1 >= decke - 1e-6 && z1 < decke + RASTER + 1e-6,
            "{z1} {decke}"
        );
        let pille = z.pill(&s).unwrap().0;
        assert!(
            pille.ends_with("OK Rohdecke OG · vom EG aus nach oben ziehen"),
            "{pille}"
        );
    }

    /// Frei gezogen steht die Unterkante im Raster über OK Rohdecke; Zahl +
    /// Enter setzt die Brüstung genau, zu hoch gilt Enter nicht.
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
        // Unten ist die Zahl die Brüstung über OK Rohdecke (Bedienbarkeit
        // 40.7): bis 10 cm unter den Sturz, nicht darüber
        let boden = s.model().level_z(LevelRef::bottom(st)).unwrap();
        for ch in "2,4".chars() {
            taste(&mut z, &mut s, Key::Char(ch));
        }
        assert!(z1 - boden < 2400.0 + MIN_OPENING, "{z1} {boden}");
        assert!(z.input.as_ref().unwrap().error.is_some());
        assert!(!taste(&mut z, &mut s, Key::Enter).fertig);
        taste(&mut z, &mut s, Key::Escape);
        for ch in "0,9".chars() {
            taste(&mut z, &mut s, Key::Char(ch));
        }
        assert_eq!(z.pill(&s), Some(("Brüstung 0,9 m".into(), true)));
        assert!(taste(&mut z, &mut s, Key::Enter).fertig);
        assert!((band(&s, id).0 - (boden + 900.0)).abs() < 1e-6);
        assert_eq!(band(&s, id).1, z1);
    }

    /// Nach dem Loslassen ersetzt eine getippte Zahl mit Enter den Schritt
    /// (Nachkorrektur wie beim Wandziehen, Bedienbarkeit 40.7): unten die
    /// Brüstung, ein Schritt im Verlauf, erneut korrigierbar; nach einem
    /// anderen Schritt oder dem Ausblenden nicht mehr.
    #[test]
    fn nachkorrektur_nach_dem_loslassen() {
        let (mut s, cam, id, eg, _) = haus();
        let st = s.model().run(eg).unwrap().storey;
        let boden = s.model().level_z(LevelRef::bottom(st)).unwrap();
        let vorher = s.model().opening(id).unwrap().clone();
        let (z0, z1) = band(&s, id);
        let mut e = crate::wall_edit::WallEdit::default();
        let mods = Default::default();
        let ev = |e: &mut crate::wall_edit::WallEdit, s: &mut Scene, ev: Event| {
            e.handle(&ev, s, &cam, W, H, 1.0, true)
        };
        let taste = |e: &mut crate::wall_edit::WallEdit, s: &mut Scene, key| {
            let k = Event::Key {
                key,
                down: true,
                mods,
                repeat: false,
            };
            e.handle(&k, s, &cam, W, H, 1.0, true)
        };
        // Unterkante greifen, nach unten ziehen, loslassen
        let (x, y) = bei(&s, &cam, id, 7000.0, z0 + 5.0);
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        let left = MouseButton::Left;
        ev(
            &mut e,
            &mut s,
            Event::MouseDown {
                button: left,
                x,
                y,
                mods,
            },
        );
        assert!(e.hoehe.is_some());
        let (x, y) = bei(&s, &cam, id, 7000.0, z0 - 333.0);
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        ev(
            &mut e,
            &mut s,
            Event::MouseUp {
                button: left,
                x,
                y,
                mods,
            },
        );
        assert!(e.hoehe.is_none());
        assert_eq!(s.undo_label(), Some(SCHRITT));
        let gezogen = band(&s, id).0;
        assert!(gezogen < z0 - 300.0, "{gezogen} {z0}");
        // Die Pille zeigt die Brüstung, die man tippen würde
        let (pille, tippt) = e.pill(&s).unwrap();
        assert!(!tippt && pille.starts_with("Brüstung "), "{pille}");
        // Tippen und Enter: ersetzt den Schritt
        for ch in "0,8".chars() {
            assert!(taste(&mut e, &mut s, Key::Char(ch)).consumed);
        }
        assert_eq!(e.input_label(), LABEL_UNTEN);
        assert_eq!(e.pill(&s), Some(("Brüstung 0,8 m".into(), true)));
        assert_eq!(band(&s, id).0, gezogen, "erst Enter ändert");
        let o = taste(&mut e, &mut s, Key::Enter);
        assert!(o.consumed && o.changed);
        assert!((band(&s, id).0 - (boden + 800.0)).abs() < 1e-6);
        assert_eq!(band(&s, id).1, z1);
        assert_eq!(s.undo_label(), Some(SCHRITT));
        assert!(s.redo_label().is_none());
        // Erneut: zu hoch wird rot, Enter gilt nicht
        for ch in "5".chars() {
            taste(&mut e, &mut s, Key::Char(ch));
        }
        assert!(e.input().unwrap().error.is_some());
        assert!(!taste(&mut e, &mut s, Key::Enter).changed);
        taste(&mut e, &mut s, Key::Backspace);
        for ch in "0,7".chars() {
            taste(&mut e, &mut s, Key::Char(ch));
        }
        assert!(taste(&mut e, &mut s, Key::Enter).changed);
        assert!((band(&s, id).0 - (boden + 700.0)).abs() < 1e-6);
        // Ein Rückgängig: wie vor dem Ziehen
        assert!(s.undo());
        assert_eq!(s.model().opening(id), Some(&vorher));
        // Nach einem anderen Schritt korrigiert die Zahl nichts mehr
        assert!(!taste(&mut e, &mut s, Key::Char('1')).consumed);
        assert!(e.input().is_none());
        // Ausgeblendet ebenso
        let (x, y) = bei(&s, &cam, id, 7000.0, z0 + 5.0);
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        ev(
            &mut e,
            &mut s,
            Event::MouseDown {
                button: left,
                x,
                y,
                mods,
            },
        );
        let (x, y) = bei(&s, &cam, id, 7000.0, z0 - 200.0);
        ev(&mut e, &mut s, Event::MouseMove { x, y, mods });
        ev(
            &mut e,
            &mut s,
            Event::MouseUp {
                button: left,
                x,
                y,
                mods,
            },
        );
        assert!(e.end_post());
        assert!(e.pill(&s).is_none());
        assert!(!taste(&mut e, &mut s, Key::Char('1')).consumed);
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

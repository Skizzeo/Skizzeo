//! Breite einer Öffnung am Griff ziehen (Paket O6, Grundriss): Steht die
//! Maus bei einer Öffnung des aktiven Geschosses, zeigt sie an beiden Enden
//! ihres Pfads einen Griff, an der Außenwand auf der Außenfläche, an der
//! Innenwand in der Wandmitte. Ziehen schiebt das Ende längs seiner Fläche
//! im Raster von 10 mm; eine Innentür rastet an den Normbreiten nach
//! DIN 18101. Die Öffnung bleibt mindestens 10 cm breit, das Ende kommt
//! nicht über das andere hinaus, und was das Modell ablehnt (Wandende,
//! Nachbaröffnung), bleibt beim letzten gültigen Stand stehen. Kommt das
//! Ende bis auf 5 cm an die Seitenflucht einer Öffnung im Geschoss darüber
//! oder darunter, rastet es dort ein und eine Spurlinie quer durch die
//! Wand zeigt es (Jörn 12.10. 05:38). Live, Loslassen ist ein Schritt, Esc
//! bricht ab. Die Pille am Cursor zeigt die Breite; eine Ziffer öffnet die
//! Eingabe „Breite“, Enter setzt sie genau (Bedienbarkeit 39.1). Von
//! außen in 3D und den Ansichten greift dasselbe Ziehen an der Seitenkante
//! ([`BreiteZug::greifen_aussen`], Paket O7). Teil von
//! [`crate::wall_edit`].

use super::drag_point;
use super::ende_edit::{self, Outcome, GRIFF_PX};
use crate::camera::Camera;
use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::meldung::Meldung;
use crate::oeffnung_app::innentuer::{nah, Koerper, DICKE};
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::{dist_to_segment, vec3, Vec3};
use sk_model::hoehen::{DOOR_CATCH, DOOR_WIDTHS};
use sk_model::oeffnung::{Opening, OpeningKind, OuterFace, MIN_OPENING};
use sk_model::{Category, ElementId, ElementKind, Model, RunId, StoreyId};
use sk_platform::{Event, Key, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bezeichnung des Schritts im Verlauf.
pub const SCHRITT: &str = "Öffnungsbreite geändert";
/// Feldname der Eingabe.
pub const LABEL: &str = "Breite";
/// Raster längs der Fläche (mm).
const STEP: f64 = 10.0;
/// Fang an den Seitenfluchten der Öffnungen darüber und darunter (mm).
const SPUR: f64 = 50.0;
/// So weit quer zur Fläche darf die Öffnung des Nachbargeschosses liegen
/// (versetzte Wand darüber), mm.
const SPUR_QUER: f64 = 1000.0;
/// Überstand der Spurlinie vor und hinter der Wand (mm).
const SPUR_UEBER: f64 = 300.0;
/// Von außen: Überstand der Spurlinie über und unter der Öffnung (mm), bis
/// ins Geschoss darüber bzw. darunter.
const SPUR_HOCH: f64 = 1500.0;

/// Die Griffe erscheinen, wenn die Maus so nah am Pfad steht (mm), bei
/// breiten Öffnungen bis zur halben Breite.
const NAH: f64 = 500.0;

fn flat(p: Vec3) -> Vec3 {
    vec3(p.x, p.y, 0.0)
}

/// Ein Griff: Öffnung, Ende im Pfad (0 oder letzter Punkt), Zeichenpunkt.
pub type Griff = (ElementId, usize, Vec3);

/// Fläche einer öffnungsfähigen Wand des Geschosses `st`, auf der der Punkt
/// `p` liegt, und die Wanddicke; `innen` bei einer Innenwand. Nur Züge nahe
/// bei `p` bauen ihren Körper, und der kommt aus `k` (Review #109 B1).
pub(crate) fn flaeche_bei(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
) -> Option<(OuterFace, f64, bool)> {
    flaeche_unter(m, k, st, p, m.runs().ids())
}

/// So weit darf die Bezugslinie eines Zugs über den Quader seines Körpers
/// hinausragen (mm), großzügig (Anschluss an eine dicke Wand).
const RAGT: f64 = 2000.0;

/// [`flaeche_bei`] über das Raster der Szene: nur die Züge, deren Körper
/// bis [`DICKE`] + [`RAGT`] an `p` reicht (Review #140, Zugschritt).
pub(crate) fn flaeche_in(scene: &Scene, st: StoreyId, p: Vec3) -> Option<(OuterFace, f64, bool)> {
    let r = DICKE + RAGT;
    let zuege = scene.zuege_im((p.x - r, p.y - r), (p.x + r, p.y + r));
    let k = |r| scene.koerper(r);
    flaeche_unter(scene.model(), &k, st, p, zuege.map(|(id, ..)| id))
}

/// [`flaeche_bei`] unter den Zügen `zuege`, aufsteigend.
fn flaeche_unter(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    zuege: impl Iterator<Item = RunId>,
) -> Option<(OuterFace, f64, bool)> {
    let p = flat(p);
    for id in zuege {
        let Some(run) = m.run(id) else {
            continue;
        };
        // Das Bauteil erst bei Zügen nahe `p` (Review #140, Zugschritt)
        if run.storey != st || !nah(&run.points, p, DICKE) {
            continue;
        }
        let cat = run
            .segments
            .first()
            .and_then(|e| m.element(*e))
            .map(|e| e.category);
        if !cat.is_some_and(Category::takes_openings) {
            continue;
        }
        let Some(c) = k(id) else {
            continue;
        };
        for seg in 0..c.segment_count() {
            let Some(f) = m.outer_face_in(id, seg, &c) else {
                continue;
            };
            let u = f.u_of(p);
            let (lo, hi) = (f.u.0.min(f.u.1), f.u.0.max(f.u.1));
            if f.on_line(p) && u > lo - 1.0 && u < hi + 1.0 {
                let innen = cat == Some(Category::InteriorWall);
                return Some((f, c.thickness(), innen));
            }
        }
    }
    None
}

/// Zeichenpunkt eines Pfadendes: Außenfläche, an der Innenwand die Mitte.
fn zeichenpunkt(f: &OuterFace, p: Vec3, dicke: f64, innen: bool) -> Vec3 {
    let p = flat(p);
    if innen {
        p - f.n * (dicke / 2.0)
    } else {
        p
    }
}

/// Griffe der Öffnungen des aktiven Geschosses bei der Maus (Punkt `p` im
/// Grundriss); `r_min` (mm) hält sie auch weit herausgezoomt nah genug,
/// dass die Maus das Kettensymbol daneben erreicht.
pub fn sichtbar(scene: &Scene, p: Vec3, r_min: f64) -> Vec<Griff> {
    // Ein Pfadstück liegt auf der Fläche eines Zugs, also in dessen Quader;
    // reicht es über zwei Züge, ist es höchstens doppelt so lang wie einer
    let r = NAH.max(r_min) + scene.zug_diagonale() + DICKE;
    let ids = super::tuer_edit::oeffnungen_nah(scene, p, r);
    griffe_von(scene, p, r_min, ids)
}

/// [`sichtbar`] unter den Öffnungen `ids`.
fn griffe_von(scene: &Scene, p: Vec3, r_min: f64, ids: Vec<ElementId>) -> Vec<Griff> {
    let m = scene.model();
    let st = scene.active_storey();
    let p = flat(p);
    let mut out = Vec::new();
    for id in ids {
        let Some(e) = m.element(id) else {
            continue;
        };
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let n = o.path.len();
        if e.storey != st || !(2..=3).contains(&n) {
            continue;
        }
        let nah = o.path.windows(2).any(|w| {
            let r = NAH.max(r_min).max((w[1] - w[0]).length() / 2.0);
            let xy = |q: Vec3| (q.x, q.y);
            dist_to_segment(xy(p), xy(w[0]), xy(w[1])) <= r
        });
        if !nah {
            continue;
        }
        for e in [0, n - 1] {
            if let Some((f, t, innen)) = flaeche_in(scene, st, o.path[e]) {
                out.push((id, e, zeichenpunkt(&f, o.path[e], t, innen)));
            }
        }
    }
    out
}

/// Enden von Öffnungen im Geschoss direkt darüber oder darunter, die mit
/// dem Ende `k` von `id` in einer Flucht liegen (Partner fürs
/// Breitenschloss, [`Model::ends_aligned`]).
pub fn partner(m: &Model, id: ElementId, k: usize) -> Vec<(ElementId, usize)> {
    let Some(st) = m.element(id).map(|e| e.storey) else {
        return Vec::new();
    };
    let nachbarn = nachbargeschosse(m, st);
    let mut out = Vec::new();
    for (pid, e) in m.elements().iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        if !nachbarn.contains(&e.storey) {
            continue;
        }
        for pk in [0, o.path.len().saturating_sub(1)] {
            if m.ends_aligned(id, k, pid, pk) {
                out.push((pid, pk));
            }
        }
    }
    out
}

/// Breitenschlösser an den Griffen `griffe` (Zeichenhöhe `z`): je Ende mit
/// Partner ein Kettensymbol außen neben dem Ende, gekoppelt, wenn es mit
/// allen Partnern gekoppelt ist.
pub fn schloesser(scene: &Scene, griffe: &[Griff], z: f64) -> Vec<crate::link_view::Schloss> {
    let m = scene.model();
    griffe
        .iter()
        .filter_map(|&(id, k, p)| {
            let partner = partner(m, id, k);
            if partner.is_empty() {
                return None;
            }
            let locks = m.end_locks(id, k);
            let linked = partner
                .iter()
                .all(|&(pid, pk)| locks.iter().any(|l| l.partner == pid && l.end == pk));
            let st = m.element(id)?.storey;
            let (f, ..) = flaeche_in(scene, st, *m.opening(id)?.path.get(k)?)?;
            Some(crate::link_view::Schloss {
                id,
                art: crate::link_view::Schlossart::Ende(k),
                p: vec3(p.x, p.y, z),
                out: f.n,
                linked,
            })
        })
        .collect()
}

/// Griff unter der Maus (Ansichtspixel `x`, `y`) aus `griffe`, gezeichnet
/// in Höhe `z`.
pub fn griff_bei(
    griffe: &[Griff],
    cam: &Camera,
    z: f64,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<Griff> {
    griffe
        .iter()
        .filter_map(|&(id, k, p)| {
            let (px, py) = cam.project(vec3(p.x, p.y, z), w, h)?;
            let d = (px - x).hypot(py - y);
            (d <= GRIFF_PX * scale).then_some((d, (id, k, p)))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|x| x.1)
}

/// Lage längs der Fläche für das gezogene Ende bei `u` (roh): im Raster,
/// mindestens [`MIN_OPENING`] vom Nachbarpunkt `u_nb` und auf der Seite, auf
/// der es beim Greifen lag (`vorz`); bei `norm` rastet die Breite an den
/// Normbreiten der Innentüren.
pub fn lage(u: f64, u_nb: f64, vorz: f64, norm: bool) -> f64 {
    let mut b = ((u - u_nb) * vorz / STEP).round() * STEP;
    if norm {
        if let Some(w) = DOOR_WIDTHS.iter().find(|w| (b - **w).abs() <= DOOR_CATCH) {
            b = *w;
        }
    }
    u_nb + vorz * b.max(MIN_OPENING)
}

/// Geschosse desselben Gebäudes direkt über und unter `st`.
fn nachbargeschosse(m: &Model, st: StoreyId) -> Vec<StoreyId> {
    let Some(s) = m.storey(st) else {
        return Vec::new();
    };
    let (b, z) = (s.building, s.elevation);
    let andere = || {
        m.storeys()
            .iter()
            .filter(move |(id, x)| *id != st && x.building == b)
    };
    let unten = andere()
        .filter(|(_, x)| x.elevation < z)
        .max_by(|a, c| a.1.elevation.total_cmp(&c.1.elevation));
    let oben = andere()
        .filter(|(_, x)| x.elevation > z)
        .min_by(|a, c| a.1.elevation.total_cmp(&c.1.elevation));
    unten.into_iter().chain(oben).map(|x| x.0).collect()
}

/// Seitenfluchten der Öffnungen im Geschoss über und unter `st` als Lage
/// längs der Fläche `f`: die Enden ihrer Pfade, die nahe vor oder hinter
/// der Fläche liegen, ohne die Enden `ohne` (die gekoppelt mitgehen).
pub fn spuren(m: &Model, st: StoreyId, f: &OuterFace, ohne: &[(ElementId, usize)]) -> Vec<f64> {
    let nachbarn = nachbargeschosse(m, st);
    let (lo, hi) = (f.u.0.min(f.u.1), f.u.0.max(f.u.1));
    let mut out = Vec::new();
    for (id, e) in m.elements().iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        if !nachbarn.contains(&e.storey) {
            continue;
        }
        let n = o.path.len();
        let enden = [0, n.saturating_sub(1)]
            .into_iter()
            .filter(|k| !ohne.contains(&(id, *k)));
        for q in enden.map(|k| flat(o.path[k])) {
            let u = f.u_of(q);
            if (q - f.a).dot(f.n).abs() <= SPUR_QUER && u > lo && u < hi {
                out.push(u);
            }
        }
    }
    out
}

/// Ein gekoppeltes Ende in einem anderen Geschoss (Breitenschloss).
struct Mit {
    id: ElementId,
    k: usize,
    /// Stand beim Greifen.
    original: Opening,
    /// Fläche des Endes.
    face: OuterFace,
    /// Lage des Nachbarpunkts und Seite des Endes dazu: Auch das
    /// gekoppelte Ende bleibt mindestens [`MIN_OPENING`] davor (Review
    /// #109 B3).
    u_nb: f64,
    vorz: f64,
}

/// Laufendes Ziehen an einem Ende einer Öffnung.
pub struct BreiteZug {
    pub id: ElementId,
    k: usize,
    original: Opening,
    face: OuterFace,
    dicke: f64,
    innen: bool,
    /// Lage des Nachbarpunkts im Pfad und Seite des Endes dazu.
    u_nb: f64,
    vorz: f64,
    /// Seitenfluchten darüber und darunter ([`spuren`]).
    spuren: Vec<f64>,
    /// Gekoppelte Enden in anderen Geschossen.
    mit: Vec<Mit>,
    /// Höhe des Grundrisses.
    z: f64,
    /// Von außen gegriffen (3D, Ansicht): die Maus zählt auf der Ebene der
    /// Fläche, gezeichnet wird die Seitenkante.
    aussen: bool,
    geaendert: bool,
    /// Getippte Breite (39.1); solange sie offen ist, ruht die Maus.
    pub input: Option<MeasureInput>,
    /// Letzte Maus, für die Rückkehr aus der Eingabe.
    maus: Option<(f64, f64)>,
}

impl BreiteZug {
    /// Greift Ende `k` der Öffnung `id` und öffnet den Schritt.
    pub fn greifen(scene: &mut Scene, id: ElementId, k: usize, z: f64) -> Option<BreiteZug> {
        let m = scene.model();
        let koerper = |r| scene.koerper(r);
        // Fläche, Nachbarpunkt und Seite des Endes `k` der Öffnung `o`
        let ende = |o: &Opening, st, k: usize| {
            let p = *o.path.get(k)?;
            let nb = if k == 0 { 1 } else { k.checked_sub(1)? };
            let (face, dicke, innen) = flaeche_bei(m, &koerper, st, p)?;
            let u_nb = face.u_of(flat(*o.path.get(nb)?));
            let vorz = (face.u_of(flat(p)) - u_nb).signum();
            Some((face, dicke, innen, u_nb, vorz))
        };
        let original = m.opening(id)?.clone();
        let st = m.element(id)?.storey;
        let (face, dicke, innen, u_nb, vorz) = ende(&original, st, k)?;
        let gekoppelt = m.locked_ends(id, k);
        let mit = gekoppelt
            .iter()
            .filter_map(|&(pid, pk)| {
                let po = m.opening(pid)?.clone();
                let (pf, _, _, u_nb, vorz) = ende(&po, m.element(pid)?.storey, pk)?;
                Some(Mit {
                    id: pid,
                    k: pk,
                    original: po,
                    face: pf,
                    u_nb,
                    vorz,
                })
            })
            .collect();
        let spuren = spuren(m, st, &face, &gekoppelt);
        scene.begin(SCHRITT);
        Some(BreiteZug {
            id,
            k,
            original,
            face,
            dicke,
            innen,
            u_nb,
            vorz,
            spuren,
            mit,
            z,
            aussen: false,
            geaendert: false,
            input: None,
            maus: None,
        })
    }

    /// Greift Ende `k` der Öffnung `id` von außen (Seitenkante in 3D oder
    /// einer Ansicht) und öffnet den Schritt.
    pub fn greifen_aussen(scene: &mut Scene, id: ElementId, k: usize) -> Option<BreiteZug> {
        let mut z = BreiteZug::greifen(scene, id, k, 0.0)?;
        z.aussen = true;
        Some(z)
    }

    /// Punkt unter der Maus: im Grundriss auf seiner Höhe, von außen auf
    /// der Ebene der Fläche.
    fn punkt(&self, cam: &Camera, x: f64, y: f64, w: f64, h: f64) -> Option<Vec3> {
        if !self.aussen {
            return drag_point(cam, x, y, w, h, self.z);
        }
        let (o, d) = cam.ray(x, y, w, h);
        let dn = d.dot(self.face.n);
        if dn.abs() < 1e-9 {
            return None;
        }
        Some(o + d * ((self.face.a - o).dot(self.face.n) / dn))
    }

    /// Die Öffnung mit dem gezogenen Ende bei `u` (roh).
    fn mit_ende(&self, u: f64) -> Opening {
        let norm = self.innen && self.original.kind == OpeningKind::Door;
        // Die nächste Seitenflucht im Fang geht vor Raster und Normbreite,
        // wenn das Ende dort auf seiner Seite und breit genug bleibt
        let spur = self
            .spuren
            .iter()
            .filter(|s| (*s - u).abs() <= SPUR && (*s - self.u_nb) * self.vorz >= MIN_OPENING)
            .min_by(|a, b| (*a - u).abs().total_cmp(&(*b - u).abs()));
        let u = spur
            .copied()
            .unwrap_or_else(|| lage(u, self.u_nb, self.vorz, norm));
        let mut o = self.original.clone();
        let alt = o.path[self.k];
        o.path[self.k] = self.face.at(u, alt.z);
        o
    }

    /// Setzt `o` und die gekoppelten Enden quer darunter, jedes auf seiner
    /// Seite und breit genug (Review #109 B3). `None`, wenn eines zu schmal
    /// würde oder das Modell ablehnt: Dann bleibt der letzte Stand. Sonst,
    /// ob sich etwas geändert hat.
    fn setzen(&mut self, scene: &mut Scene, o: Opening) -> Option<bool> {
        let q = flat(o.path[self.k]);
        let mut neu = vec![(self.id, o)];
        for x in &self.mit {
            let u = x.face.u_of(q);
            if (u - x.u_nb) * x.vorz < MIN_OPENING - 1e-6 {
                return None;
            }
            let mut po = x.original.clone();
            po.path[x.k] = x.face.at(u, po.path[x.k].z);
            neu.push((x.id, po));
        }
        let m = scene.model();
        if neu.iter().all(|(id, o)| m.opening(*id) == Some(o)) {
            return Some(false);
        }
        // Das Modell prüft beim Setzen; lehnt es eine ab, gehen die schon
        // gesetzten zurück (Review #109 B4)
        let mut alt = Vec::new();
        for (id, o) in neu {
            let vorher = scene.model().opening(id).cloned();
            if !scene.set_opening(id, o) {
                for (id, v) in alt.into_iter().rev() {
                    scene.set_opening(id, v);
                }
                return None;
            }
            alt.extend(vorher.map(|v| (id, v)));
        }
        self.geaendert = true;
        Some(true)
    }

    /// Das Ende unter der Maus (Raster, Normbreite, Seitenflucht).
    fn zur_maus(&mut self, scene: &mut Scene, cam: &Camera, w: f64, h: f64) -> bool {
        let Some((x, y)) = self.maus else {
            return false;
        };
        let Some(p) = self.punkt(cam, x, y, w, h) else {
            return false;
        };
        let o = self.mit_ende(self.face.u_of(flat(p)));
        self.setzen(scene, o) == Some(true)
    }

    /// Getippte Breite (ganze Öffnung, um die Ecke beide Schenkel): das
    /// gezogene Ende genau dort, ohne Raster und Fang. Passt sie nicht,
    /// wird die Eingabe rot und Enter gilt nicht.
    fn getippt(&mut self, scene: &mut Scene) -> bool {
        let Some(Some(Ok(v))) = self.input.as_ref().map(|i| i.value(0)) else {
            return false;
        };
        let alt = self.original.path[self.k];
        let schenkel = (self.face.u_of(flat(alt)) - self.u_nb).abs();
        let b = v - (self.original.width() - schenkel);
        let fehler = if b < MIN_OPENING - 1e-6 {
            Some(Meldung::satz("Eine Öffnung ist mindestens 10 cm breit"))
        } else {
            let mut o = self.original.clone();
            o.path[self.k] = self.face.at(self.u_nb + self.vorz * b, alt.z);
            match self.setzen(scene, o) {
                Some(geaendert) => {
                    return geaendert;
                }
                None => Some(Meldung::satz(
                    "So breit stößt die Öffnung an Wandende oder Nachbaröffnung",
                )),
            }
        };
        if let Some(i) = self.input.as_mut() {
            i.error = fehler;
        }
        false
    }

    /// Loslassen bzw. Enter: ein Schritt, wenn sich etwas geändert hat und
    /// keine ungültige Eingabe offen ist.
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
                // Eine getippte Breite gilt, bis die Eingabe schließt
                if self.input.is_none() && self.zur_maus(scene, cam, w, h) {
                    out.changed = true;
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => self.schliessen(scene, &mut out),
            // Mit offener Eingabe schließt Esc erst sie (unten)
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

    /// Pille am Cursor: die Eingabe, sonst die Breite, wie die Öffnung
    /// gerade steht (wie beim Aufziehen).
    pub fn pill(&self, scene: &Scene) -> Option<(String, bool)> {
        if let Some(i) = &self.input {
            return Some((i.text([LABEL, ""]), true));
        }
        let b = scene.model().opening(self.id)?.width();
        Some((format!("{LABEL} {}", crate::ui::live_length_text(b)), false))
    }

    /// Zug des gezogenen Endes: Er wird beim Ziehen live gezeichnet, die
    /// gekoppelten mit ([`Scene::set_opening`]).
    pub fn run(&self) -> sk_model::RunId {
        self.face.run
    }

    /// Der Griff am gezogenen Ende, wie die Öffnung gerade steht; von außen
    /// die Seitenkante als Band.
    pub fn helpers(&self, scene: &Scene, scale: f32, occlude: bool, theme: &Theme) -> Vec<Helper> {
        let Some(p) = scene
            .model()
            .opening(self.id)
            .and_then(|o| o.path.get(self.k))
        else {
            return Vec::new();
        };
        if self.aussen {
            return self.helpers_aussen(scene, *p, scale, occlude, theme);
        }
        let g = zeichenpunkt(&self.face, *p, self.dicke, self.innen);
        let mut out = ende_edit::griff(g, self.z, scale, occlude, theme).to_vec();
        // Spurlinie, wenn das Ende auf einer Seitenflucht steht
        let u = self.face.u_of(flat(*p));
        if self.spuren.iter().any(|s| (s - u).abs() < 1e-6) {
            let f = &self.face;
            let q = |t: f64| {
                let v = f.at(u, 0.0) + f.n * t;
                [v.x as f32, v.y as f32, self.z as f32 + 2.0]
            };
            out.push(Helper {
                a: q(SPUR_UEBER),
                b: q(-self.dicke - SPUR_UEBER),
                color: theme.interact.track,
                width: theme.size.link_line * scale,
                dash: theme.size.link_dash * scale,
                pattern: sk_render::SOLID,
                occlude,
                round: false,
            });
        }
        out
    }

    /// Von außen: die Seitenkante als Band, auf einer Seitenflucht dazu die
    /// Spurlinie senkrecht bis ins Geschoss darüber und darunter.
    fn helpers_aussen(
        &self,
        scene: &Scene,
        p: Vec3,
        scale: f32,
        occlude: bool,
        theme: &Theme,
    ) -> Vec<Helper> {
        let m = scene.model();
        let Some((z0, z1)) = m.opening(self.id).and_then(|o| m.opening_band(o)) else {
            return Vec::new();
        };
        let f = &self.face;
        let u = f.u_of(flat(p));
        let mut out = super::oeffnung_kanten::band(
            f.at(u, z0) + f.n * super::oeffnung_kanten::VOR,
            f.at(u, z1) + f.n * super::oeffnung_kanten::VOR,
            true,
            scale,
            occlude,
            theme,
        )
        .to_vec();
        if self.spuren.iter().any(|s| (s - u).abs() < 1e-6) {
            let q = |z: f64| {
                let v = f.at(u, z) + f.n * super::oeffnung_kanten::VOR;
                [v.x as f32, v.y as f32, v.z as f32]
            };
            out.push(Helper {
                a: q(z0 - SPUR_HOCH),
                b: q(z1 + SPUR_HOCH),
                color: theme.interact.track,
                width: theme.size.link_line * scale,
                dash: theme.size.link_dash * scale,
                pattern: sk_render::SOLID,
                occlude,
                round: false,
            });
        }
        out
    }

    /// Steht das gezogene Ende gerade auf einer Seitenflucht darüber oder
    /// darunter?
    #[cfg(test)]
    fn auf_spur(&self, scene: &Scene) -> bool {
        let p = scene.model().opening(self.id).map(|o| o.path[self.k]);
        p.is_some_and(|p| {
            let u = self.face.u_of(flat(p));
            self.spuren.iter().any(|s| (s - u).abs() < 1e-6)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::oeffnung::{Din, FaceSide, HeightLine, HeightRef, Swing};
    use sk_model::{LevelRef, RefSide};

    /// Prüfhaus 10 × 8 m mit Innenwand bei x = 5 m und einer Tür darin,
    /// 88,5 cm breit um y = 3 m, dazu ein Fenster in der Außenwand y = 0.
    fn haus() -> (Scene, ElementId, ElementId) {
        let mut m = Model::with_seed(413);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let st = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        let iw = m
            .add_wall_run(
                &[vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let f = m.outer_face(iw, 0).unwrap();
        let u = f.u_of(vec3(5000.0, 3000.0, 0.0));
        let tuer = Opening {
            kind: OpeningKind::Door,
            path: vec![f.at(u - 442.5, 0.0), f.at(u + 442.5, 0.0)],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: Some(Swing {
                side: FaceSide::Right,
                din: Din::Links,
            }),
            lock: Default::default(),
        };
        let tuer = m.add_opening(tuer).unwrap();
        let seg = (0..4)
            .find(|&s| {
                m.outer_face(eg, s)
                    .is_some_and(|f| f.n.y < -0.5 && f.u_of(vec3(7000.0, 0.0, 0.0)).is_finite())
            })
            .unwrap();
        let fa = m.outer_face(eg, seg).unwrap();
        let ua = fa.u_of(vec3(7000.0, -300.0, 0.0));
        let fenster = Opening {
            kind: OpeningKind::Window,
            path: vec![fa.at(ua - 500.0, 0.0), fa.at(ua + 500.0, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Default::default(),
        };
        let fenster = m.add_opening(fenster).unwrap();
        (Scene::with_model(m), tuer, fenster)
    }

    /// Raster, Mindestbreite, keine Kreuzung, Normbreiten.
    #[test]
    fn lage_im_raster() {
        assert_eq!(lage(1234.0, 0.0, 1.0, false), 1230.0);
        assert_eq!(lage(-50.0, 0.0, 1.0, false), MIN_OPENING);
        assert_eq!(lage(-1234.0, 0.0, -1.0, false), -1230.0);
        assert_eq!(lage(500.0, 0.0, -1.0, false), -MIN_OPENING);
        assert_eq!(lage(870.0, 0.0, 1.0, true), 885.0);
        assert_eq!(lage(1040.0, 0.0, 1.0, true), 1010.0);
        assert_eq!(lage(950.0, 0.0, 1.0, true), 950.0);
    }

    /// Griffe erscheinen nur bei der Maus: an der Innentür in der
    /// Wandmitte, am Fenster auf der Außenfläche.
    #[test]
    fn griffe_bei_der_maus() {
        let (s, tuer, fenster) = haus();
        let g = sichtbar(&s, vec3(5100.0, 3000.0, 0.0), 0.0);
        assert_eq!(g.iter().filter(|x| x.0 == tuer).count(), 2);
        assert!(g.iter().all(|x| x.0 == tuer));
        let ys: Vec<f64> = g.iter().map(|x| x.2.y).collect();
        assert!(ys.iter().any(|y| (y - 2557.5).abs() < 1e-6), "{ys:?}");
        assert!(g.iter().all(|x| (x.2.x - 5000.0).abs() < 1e-6), "{g:?}");
        let g = sichtbar(&s, vec3(7000.0, 200.0, 0.0), 0.0);
        assert_eq!(g.len(), 2);
        assert!(g.iter().all(|x| x.0 == fenster));
        assert!(sichtbar(&s, vec3(2500.0, 6000.0, 0.0), 0.0).is_empty());
    }

    /// Vorfilter je Wandzug (Review #140, Zugschritt): Rund um das Haus mit
    /// Innentür und um ein zweigeschossiges mit Fenstern, eines 3,5 m
    /// breit, stehen in jedem Geschoss, nah und weit herausgezoomt,
    /// dieselben Griffe wie über alle Bauteile, und das Raster findet
    /// dieselbe Wandfläche wie der Gang über alle Züge.
    #[test]
    fn vorfilter_wie_alle() {
        let (s, _, _) = haus();
        let (mut m, eg, og) = zwei_geschosse();
        fenster(&mut m, eg, 1000.0, 4500.0);
        fenster(&mut m, eg, 6500.0, 7500.0);
        fenster(&mut m, og, 6000.0, 7000.0);
        let geschosse = [m.run(eg).unwrap().storey, m.run(og).unwrap().storey];
        let pruefen = |s: &Scene| {
            let alle: Vec<ElementId> = s.model().elements().ids().collect();
            let (mut gesehen, mut flaechen) = (0, 0);
            for x in (-3000..=13000).step_by(250) {
                for y in (-3000..=11000).step_by(250) {
                    let p = vec3(x as f64, y as f64, 0.0);
                    for r_min in [0.0, 1500.0, 30000.0] {
                        let g = sichtbar(s, p, r_min);
                        assert_eq!(g, griffe_von(s, p, r_min, alle.clone()), "{p:?} {r_min}");
                        gesehen += g.len();
                    }
                    let st = s.active_storey();
                    let k = |r| s.koerper(r);
                    let f = flaeche_in(s, st, p);
                    assert_eq!(f, flaeche_bei(s.model(), &k, st, p), "{p:?}");
                    flaechen += usize::from(f.is_some());
                }
            }
            assert!(gesehen > 0 && flaechen > 0);
        };
        pruefen(&s);
        let mut zwei = Scene::with_model(m);
        for st in geschosse {
            zwei.set_active_storey(st);
            assert_eq!(zwei.active_storey(), st);
            pruefen(&zwei);
        }
    }

    /// Ziehen am Ende der Innentür: rastet an der Normbreite, Esc stellt
    /// zurück, Loslassen ist ein Schritt; am Fenster bleibt die Öffnung vor
    /// der Hausecke stehen, wenn das Modell weiter ablehnt.
    #[test]
    fn ende_ziehen() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(
            vec3(5000.0, 3000.0, 20000.0),
            vec3(5000.0, 3001.0, 0.0),
            45.0,
        );
        let (mut s, tuer, fenster) = haus();
        let mods = Default::default();
        let breite = |s: &Scene, id| s.model().opening(id).unwrap().width();
        let k_bei = |s: &Scene, id, y: f64| {
            let o = s.model().opening(id).unwrap();
            usize::from((o.path[1].y - y).abs() < (o.path[0].y - y).abs())
        };
        // Oberes Ende der Tür (y = 3442,5) auf 3555: Breite 997,5, im
        // Raster 1000, gefangen an 1010
        let k = k_bei(&s, tuer, 3442.5);
        let mut z = BreiteZug::greifen(&mut s, tuer, k, 0.0).unwrap();
        let (x, y) = cam.project(vec3(5000.0, 3555.0, 0.0), W, H).unwrap();
        assert!(
            z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H)
                .changed
        );
        assert!(
            (breite(&s, tuer) - 1010.0).abs() < 1e-6,
            "{}",
            breite(&s, tuer)
        );
        let esc = Event::Key {
            key: Key::Escape,
            down: true,
            mods,
            repeat: false,
        };
        assert!(z.handle(&esc, &mut s, &cam, W, H).fertig);
        assert!((breite(&s, tuer) - 885.0).abs() < 1e-6);
        // Unteres Ende nach unten auf 760 schmaler (Fang), Loslassen
        let k = k_bei(&s, tuer, 2557.5);
        let mut z = BreiteZug::greifen(&mut s, tuer, k, 0.0).unwrap();
        let (x, y) = cam.project(vec3(5000.0, 2690.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let los = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        assert!(z.handle(&los, &mut s, &cam, W, H).fertig);
        assert!(
            (breite(&s, tuer) - 760.0).abs() < 1e-6,
            "{}",
            breite(&s, tuer)
        );
        assert_eq!(s.undo_label(), Some(SCHRITT));
        // Fenster: das rechte Ende weit über die Hausecke hinaus gezogen
        // bleibt beim letzten gültigen Stand vor der Ecke
        let o = s.model().opening(fenster).unwrap().clone();
        let k = usize::from(o.path[1].x > o.path[0].x);
        let mut z = BreiteZug::greifen(&mut s, fenster, k, 0.0).unwrap();
        for x_mm in [8000.0, 9000.0, 9600.0, 12000.0] {
            let (x, y) = cam.project(vec3(x_mm, 0.0, 0.0), W, H).unwrap();
            z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        }
        let o = s.model().opening(fenster).unwrap();
        let xmax = o.path[0].x.max(o.path[1].x);
        assert!(xmax > 9000.0 && xmax < 10000.0, "{xmax}");
        assert_eq!(s.model().opening_fault(o, Some(fenster)), None);
    }

    /// Bedienbarkeit 39.1/39.2: Beim Ziehen zeigt die Pille die Breite,
    /// Zahl + Enter setzt sie genau, ohne Raster. Zu schmal oder zu breit
    /// wird die Eingabe rot und Enter gilt nicht; Esc schließt erst die
    /// Eingabe, dann das Ziehen. F1 zeigt dabei das Thema Öffnungen.
    #[test]
    fn breite_tippen() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(vec3(7000.0, 0.0, 20000.0), vec3(7000.0, 1.0, 0.0), 45.0);
        let (mut s, _, fenster) = haus();
        let mods = Default::default();
        let taste = |z: &mut BreiteZug, s: &mut Scene, key| {
            let e = Event::Key {
                key,
                down: true,
                mods,
                repeat: false,
            };
            z.handle(&e, s, &cam, W, H)
        };
        let tippen = |z: &mut BreiteZug, s: &mut Scene, t: &str| {
            for ch in t.chars() {
                taste(z, s, Key::Char(ch));
            }
        };
        let breite = |s: &Scene| s.model().opening(fenster).unwrap().width();
        let o = s.model().opening(fenster).unwrap().clone();
        let k = usize::from(o.path[1].x > o.path[0].x);
        let links = o.path[1 - k];
        let z = BreiteZug::greifen(&mut s, fenster, k, 0.0).unwrap();
        let mut e = crate::wall_edit::WallEdit {
            breite: Some(z),
            ..Default::default()
        };
        assert!(e.eigenes_thema());
        assert_eq!(e.thema(), crate::help::Topic::Openings);
        assert_eq!(e.input_label(), LABEL);
        assert_eq!(e.pill(&s), Some(("Breite 1,00 m".into(), false)));
        let mut z = e.breite.take().unwrap();
        tippen(&mut z, &mut s, "1,234");
        assert_eq!(z.pill(&s), Some(("Breite 1,234 m".into(), true)));
        assert!((breite(&s) - 1234.0).abs() < 1e-6, "{}", breite(&s));
        assert!(taste(&mut z, &mut s, Key::Enter).fertig);
        assert_eq!(s.undo_label(), Some(SCHRITT));
        let o = s.model().opening(fenster).unwrap();
        assert!(
            (o.path[1 - k] - links).length() < 1e-6,
            "linkes Ende bleibt"
        );
        // Unter 10 cm und über die Hausecke hinaus gilt Enter nicht
        let mut z = BreiteZug::greifen(&mut s, fenster, k, 0.0).unwrap();
        for zu in ["0,05", "9"] {
            tippen(&mut z, &mut s, zu);
            assert!(z.input.as_ref().unwrap().error.is_some(), "{zu}");
            assert!(!taste(&mut z, &mut s, Key::Enter).fertig, "{zu}");
            assert!((breite(&s) - 1234.0).abs() < 1e-6, "{zu}: {}", breite(&s));
            assert!(!taste(&mut z, &mut s, Key::Escape).fertig);
            assert!(z.input.is_none());
        }
        assert!(taste(&mut z, &mut s, Key::Escape).fertig);
        assert!((breite(&s) - 1234.0).abs() < 1e-6);
        assert_eq!(s.undo_label(), Some(SCHRITT));
    }

    /// Zweigeschossiges Haus 10 × 8 m: Modell, EG- und OG-Zug.
    fn zwei_geschosse() -> (Model, sk_model::RunId, sk_model::RunId) {
        let mut m = Model::with_seed(414);
        let b = m.add_building(2);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).unwrap();
        let og = m.runs_above(eg)[0];
        (m, eg, og)
    }

    /// Fenster in der Südwand des Zugs `run` von `von` bis `bis` (x, mm).
    fn fenster(m: &mut Model, run: sk_model::RunId, von: f64, bis: f64) -> ElementId {
        let st = m.run(run).unwrap().storey;
        let seg = (0..4)
            .find(|&k| m.outer_face(run, k).is_some_and(|f| f.n.y < -0.5))
            .unwrap();
        let f = m.outer_face(run, seg).unwrap();
        let (a, b) = (f.u_of(vec3(von, 0.0, 0.0)), f.u_of(vec3(bis, 0.0, 0.0)));
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
        m.add_opening(o).unwrap()
    }

    /// Zweigeschossiges Haus: ein Fenster im EG von 6,5 bis 7,5 m, eines im
    /// OG von 6 bis 7 m. Das rechte Ende des EG-Fensters rastet bis 5 cm an
    /// der Seitenflucht 7 m des OG-Fensters ein, weiter weg gilt das Raster.
    #[test]
    fn spur_zum_geschoss_darueber() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(vec3(7000.0, 0.0, 20000.0), vec3(7000.0, 1.0, 0.0), 45.0);
        let (mut m, eg, og) = zwei_geschosse();
        let unten = fenster(&mut m, eg, 6500.0, 7500.0);
        let oben = fenster(&mut m, og, 6000.0, 7000.0);
        let mut s = Scene::with_model(m);
        let o = s.model().opening(unten).unwrap().clone();
        let k = usize::from(o.path[1].x > o.path[0].x);
        let mut z = BreiteZug::greifen(&mut s, unten, k, 0.0).unwrap();
        let mods = Default::default();
        let rechts = |s: &Scene| {
            let o = s.model().opening(unten).unwrap();
            o.path[0].x.max(o.path[1].x)
        };
        let (x, y) = cam.project(vec3(7040.0, 0.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        assert!((rechts(&s) - 7000.0).abs() < 1e-6, "{}", rechts(&s));
        assert!(z.auf_spur(&s));
        let (x, y) = cam.project(vec3(7083.0, 0.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        assert!((rechts(&s) - 7080.0).abs() < 1e-6, "{}", rechts(&s));
        assert!(!z.auf_spur(&s));
        // Auf der Flucht loslassen, koppeln: das OG-Ende geht mit
        let (x, y) = cam.project(vec3(7010.0, 0.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        let los = Event::MouseUp {
            button: MouseButton::Left,
            x,
            y,
            mods,
        };
        z.handle(&los, &mut s, &cam, W, H);
        let ob = s.model().opening(oben).unwrap().clone();
        let ko = usize::from(ob.path[1].x > ob.path[0].x);
        // Kettensymbol am Ende: erst gelöst, nach dem Klick gekoppelt
        let ende = s.model().opening(unten).unwrap().path[k];
        let g = [(unten, k, ende)];
        assert_eq!(partner(s.model(), unten, k), vec![(oben, ko)]);
        assert!(partner(s.model(), unten, 1 - k).is_empty(), "6,5 m und 6 m");
        let sl = schloesser(&s, &g, 0.0);
        assert_eq!(sl.len(), 1);
        assert!(!sl[0].linked);
        let chips = crate::link_view::chips(&crate::link_view::Want {
            scene: &s,
            cam: &cam,
            w: W,
            h: H,
            top: 0.0,
            scale: 1.0,
            plan_z: Some(0.0),
            band: None,
            hover: None,
            keep_linked: None,
            schloesser: sl,
        });
        assert!(chips
            .iter()
            .any(|c| c.schloss == Some(crate::link_view::Schlossart::Ende(k)) && c.wall == unten));
        assert!(s.set_end_locks(unten, k, &[(oben, ko)], true));
        assert!(schloesser(&s, &g, 0.0)[0].linked);
        let mut z = BreiteZug::greifen(&mut s, unten, k, 0.0).unwrap();
        let (x, y) = cam.project(vec3(7300.0, 0.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        z.handle(&los, &mut s, &cam, W, H);
        let x_oben = s.model().opening(oben).unwrap().path[ko].x;
        assert!((rechts(&s) - 7300.0).abs() < 1e-6, "{}", rechts(&s));
        assert!((x_oben - 7300.0).abs() < 1e-6, "{x_oben}");
        // Ein Schritt für beide: Rückgängig stellt beide zurück
        assert!(s.undo());
        let x_oben = s.model().opening(oben).unwrap().path[ko].x;
        assert!((rechts(&s) - 7000.0).abs() < 1e-6 && (x_oben - 7000.0).abs() < 1e-6);
    }

    /// Review #109 B3: EG 6,0–7,0 m und OG 6,9–7,0 m bei 7 m gekoppelt.
    /// Zieht man das EG-Ende auf 6,3 m, bliebe das OG-Fenster nicht
    /// 10 cm breit: Beide bleiben beim letzten gültigen Stand, nichts
    /// dreht sich um.
    #[test]
    fn gekoppeltes_ende_mit_grenzen() {
        const W: f64 = 1200.0;
        const H: f64 = 800.0;
        let cam = Camera::looking_at(vec3(7000.0, 0.0, 20000.0), vec3(7000.0, 1.0, 0.0), 45.0);
        let (mut m, eg, og) = zwei_geschosse();
        let unten = fenster(&mut m, eg, 6000.0, 7000.0);
        let oben = fenster(&mut m, og, 6900.0, 7000.0);
        let mut s = Scene::with_model(m);
        let rechts = |s: &Scene, id| {
            let o = s.model().opening(id).unwrap();
            let k = usize::from(o.path[1].x > o.path[0].x);
            (k, o.path[k].x, o.path[1 - k].x)
        };
        let (k, ..) = rechts(&s, unten);
        let (ko, ..) = rechts(&s, oben);
        assert_eq!(partner(s.model(), unten, k), vec![(oben, ko)]);
        assert!(s.set_end_locks(unten, k, &[(oben, ko)], true));
        let mut z = BreiteZug::greifen(&mut s, unten, k, 0.0).unwrap();
        let mods = Default::default();
        for x_mm in [7050.0, 6300.0] {
            let (x, y) = cam.project(vec3(x_mm, 0.0, 0.0), W, H).unwrap();
            z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        }
        let (_, r_unten, _) = rechts(&s, unten);
        let (k2, r_oben, l_oben) = rechts(&s, oben);
        assert_eq!(k2, ko, "OG-Fenster umgedreht");
        assert!((r_unten - 7050.0).abs() < 1e-6, "{r_unten}");
        assert!((r_oben - 7050.0).abs() < 1e-6 && (l_oben - 6900.0).abs() < 1e-6);
        // Bis 7,0 m geht es, das OG-Fenster bleibt 10 cm breit
        let (x, y) = cam.project(vec3(7000.0, 0.0, 0.0), W, H).unwrap();
        z.handle(&Event::MouseMove { x, y, mods }, &mut s, &cam, W, H);
        assert!((rechts(&s, oben).1 - 7000.0).abs() < 1e-6);
    }
}

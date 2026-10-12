//! Werkzeug „Innentür“ im Grundriss (Jörn 12.10. 06:32–06:33): Knopf
//! „Tür“ im Grundriss. Über einer Innenwand hängt die Tür als Vorschau am
//! Cursor, mit Lichte, offenem Blatt und Viertelkreis; sie schlägt in den
//! Raum auf, auf dessen Seite der Cursor steht. Bezugspunkt ist die
//! Türmitte, **Tab** schaltet zu links und rechts (aus diesem Raum zur Wand
//! gesehen); das Mausrad blättert durch die Normbreiten nach DIN 18101. Ein
//! Klick setzt die Tür: unten OK Rohdecke, oben die Innentürlinie, DIN
//! links. Eine Maßkette auf der Wandseite gegenüber dem Aufschlag zeigt
//! die Abstände zu den Flächen der anschließenden Wände, die Pille den zur
//! näheren; eine Ziffer öffnet dort die Eingabe, **Enter** setzt die Tür
//! in diesem Abstand (plan.md §4.3, Grundriss G7).
//!
//! Hier steht die reine Rechnung; Maus und Szene in `oeffnung_app.rs`.

use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::hoehen::DOOR_WIDTHS;
use sk_model::oeffnung::{
    Din, FaceSide, HeightLine, HeightRef, Opening, OpeningKind, OuterFace, Swing,
};
use sk_model::{Category, LevelRef, Model, RunId, StoreyId, WallChain};
use sk_platform::{Key, Modifiers};

/// Raster längs der Wand für den Bezugspunkt (mm).
const RASTER: f64 = 10.0;
/// Stücke des Viertelkreises in der Vorschau.
const BOGEN: usize = 12;
/// Vorgabe der Breite: 88,5 cm.
const BREITE: usize = 2;
/// Abstand der Maßkette von der Wandfläche (mm).
const KETTE: f64 = 250.0;
/// Halbe Länge der Schrägstriche an der Maßkette (mm).
const STRICH: f64 = 60.0;

/// Bezugspunkt der Tür am Cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bezug {
    #[default]
    Mitte,
    Links,
    Rechts,
}

impl Bezug {
    /// **Tab**: Mitte, links, rechts, wieder Mitte.
    pub fn next(self) -> Bezug {
        match self {
            Bezug::Mitte => Bezug::Links,
            Bezug::Links => Bezug::Rechts,
            Bezug::Rechts => Bezug::Mitte,
        }
    }

    /// Name in der Statuszeile.
    pub fn name(self) -> &'static str {
        match self {
            Bezug::Mitte => "Mitte",
            Bezug::Links => "links",
            Bezug::Rechts => "rechts",
        }
    }
}

/// Die Tür am Cursor über einer Innenwand.
#[derive(Clone, Debug, PartialEq)]
pub struct Vorschau {
    /// Fläche der Innenwand, auf der der Pfad liegt.
    pub face: OuterFace,
    /// Lichte längs der Fläche, aufsteigend.
    pub u: (f64, f64),
    pub swing: Swing,
    pub dicke: f64,
    /// Zeichenhöhe (Schnitthöhe des Grundrisses).
    pub z: f64,
    /// Bezugspunkt am Cursor, auf der Wandmitte.
    pub punkt: Vec3,
}

/// Zustand des Werkzeugs.
#[derive(Clone, Debug, PartialEq)]
pub struct Innentuer {
    /// Stelle in [`DOOR_WIDTHS`].
    pub breite: usize,
    pub bezug: Bezug,
    pub hover: Option<Vorschau>,
    /// Letzte Suche: Grundrisspunkt, Toleranz, Zeichenhöhe (für Tab und
    /// Mausrad ohne Mausbewegung).
    pub ort: Option<(Vec3, f64, f64)>,
    /// Getippter Abstand zur näheren Wandfläche.
    pub eingabe: Option<MeasureInput>,
}

/// Was eine Taste mit der Tür am Cursor bewirkt hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taste {
    /// Nicht für die Eingabe, sie geht weiter (Tab, Esc ohne Eingabe).
    Frei,
    /// Eingabe geöffnet, geändert oder geschlossen.
    Geaendert,
    /// Enter: Die Tür steht im getippten Abstand und wird gesetzt.
    Setzen,
    /// Enter, doch so passt die Tür nicht in die Wand; die Eingabe bleibt.
    PasstNicht,
}

impl Default for Innentuer {
    fn default() -> Innentuer {
        Innentuer {
            breite: BREITE,
            bezug: Bezug::Mitte,
            hover: None,
            ort: None,
            eingabe: None,
        }
    }
}

/// Seite der Fläche `f` zu ihrer Richtung.
fn flaechenseite(f: &OuterFace) -> FaceSide {
    if f.n.dot(vec3(f.d.y, -f.d.x, 0.0)) > 0.0 {
        FaceSide::Right
    } else {
        FaceSide::Left
    }
}

/// Wandkörper je Zug, in der App aus dem Zwischenspeicher der Szene
/// (Review #99 B1).
pub type Koerper<'a> = &'a dyn Fn(RunId) -> Option<WallChain>;

/// Vorfilter für [`treffer`] (mm): So weit darf eine Wandfläche von der
/// Bezugslinie ihres Zuges liegen.
pub(crate) const DICKE: f64 = 1000.0;

/// Liegt `p` im Rechteck um die Punkte `pts`, um `rand` vergrößert?
pub(crate) fn nah(pts: &[Vec3], p: Vec3, rand: f64) -> bool {
    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
    for q in pts {
        (lo.0, lo.1, hi.0, hi.1) = (lo.0.min(q.x), lo.1.min(q.y), hi.0.max(q.x), hi.1.max(q.y));
    }
    p.x >= lo.0 - rand && p.x <= hi.0 + rand && p.y >= lo.1 - rand && p.y <= hi.1 + rand
}

/// Innenwand des Geschosses `st` unter dem Grundrisspunkt `p` (Toleranz
/// `tol` in mm): ihre Fläche, die Lage längs, ob `p` auf der Seite der
/// Fläche liegt, und die Wanddicke. Die nächste Wandmitte gewinnt.
pub fn treffer(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    wand_unter(m, k, st, p, tol, |c| c == Category::InteriorWall)
}

/// Wie [`treffer`] für Außenwände: Fenster und Außentür im Grundriss
/// (Jörn 12.10. 09:26); die Fläche ist die Außenfläche.
pub fn treffer_aussen(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    wand_unter(m, k, st, p, tol, Category::is_outer_wall)
}

/// Wand des Geschosses `st` mit einer Kategorie nach `art` unter `p`.
fn wand_unter(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
    art: impl Fn(Category) -> bool,
) -> Option<(OuterFace, f64, bool, f64)> {
    let p = vec3(p.x, p.y, 0.0);
    let mut best: Option<(f64, (OuterFace, f64, bool, f64))> = None;
    for (id, run) in m.runs().iter() {
        let passt = run
            .segments
            .first()
            .and_then(|e| m.element(*e))
            .is_some_and(|e| art(e.category));
        if run.storey != st || !passt || !nah(&run.points, p, tol + DICKE) {
            continue;
        }
        // Ein Körper je Zug für alle Segmente (Review #99 B1)
        let Some(c) = k(id) else {
            continue;
        };
        let t = c.thickness();
        for seg in 0..c.segment_count() {
            let Some(f) = m.outer_face_in(id, seg, &c) else {
                continue;
            };
            let u = f.u_of(p);
            let (lo, hi) = (f.u.0.min(f.u.1), f.u.0.max(f.u.1));
            let q = (p - f.a).dot(f.n);
            if u < lo - tol || u > hi + tol || q > tol || q < -t - tol {
                continue;
            }
            let d = (q + t / 2.0).abs();
            if best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, (f, u, q > -t / 2.0, t)));
            }
        }
    }
    best.map(|b| b.1)
}

impl Innentuer {
    /// Breite in mm.
    pub fn breite_mm(&self) -> f64 {
        DOOR_WIDTHS[self.breite.min(DOOR_WIDTHS.len() - 1)]
    }

    /// Mausrad: nach vorn breiter, nach hinten schmaler, an den Enden
    /// bleibt es. `true`, wenn sich die Breite ändert.
    pub fn rad(&mut self, delta: f64) -> bool {
        let alt = self.breite;
        if delta > 0.0 {
            self.breite = (self.breite + 1).min(DOOR_WIDTHS.len() - 1);
        } else if delta < 0.0 {
            self.breite = self.breite.saturating_sub(1);
        }
        self.breite != alt
    }

    /// Breite und Bezugspunkt für die Statuszeile, z. B.
    /// „Tür 88,5 cm · Bezug Mitte“.
    pub fn anzeige(&self) -> String {
        format!(
            "Tür {} cm · Bezug {}",
            crate::ui::cm_text(self.breite_mm()),
            self.bezug.name()
        )
    }

    /// **Tab**: nächster Bezugspunkt.
    pub fn tab(&mut self) {
        self.bezug = self.bezug.next();
    }

    /// Vorschau für den Grundrisspunkt `p` im Geschoss `st`, gezeichnet in
    /// Höhe `z`; `None` neben jeder Innenwand oder wenn die Tür dort nicht
    /// in die Wandfläche passt.
    pub fn suchen(&mut self, m: &Model, k: Koerper, st: StoreyId, p: Vec3, tol: f64, z: f64) {
        self.ort = Some((p, tol, z));
        self.hover = self.vorschau(m, k, st, p, tol, z);
        // Fort von der Innenwand: die Eingabe hat keine Tür mehr
        if self.hover.is_none() {
            self.eingabe = None;
        }
    }

    /// Sucht an der letzten Stelle neu (nach Tab, Mausrad oder Klick).
    pub fn nochmal(&mut self, m: &Model, k: Koerper, st: StoreyId) {
        if let Some((p, tol, z)) = self.ort {
            self.suchen(m, k, st, p, tol, z);
        }
    }

    /// Maus fort: keine Vorschau, die Eingabe schließt.
    pub fn weg(&mut self) {
        self.ort = None;
        self.hover = None;
        self.eingabe = None;
    }

    /// Taste für die Eingabe des Abstands: Eine Ziffer öffnet sie, solange
    /// die Tür an einer Innenwand hängt; **Enter** rückt die Tür so weit
    /// von der näheren Wandfläche, dann wird sie gesetzt.
    pub fn taste(&mut self, key: Key, mods: Modifiers) -> Taste {
        let Some(i) = self.eingabe.as_mut() else {
            let Key::Char(ch) = key else {
                return Taste::Frei;
            };
            if self.hover.is_none() || !opens(ch, MeasureKind::Length, mods) {
                return Taste::Frei;
            }
            let mut i = MeasureInput::new(MeasureKind::Length, None);
            i.push(ch);
            self.eingabe = Some(i);
            return Taste::Geaendert;
        };
        match i.key(key, mods) {
            InputOutcome::Ignored => Taste::Frei,
            InputOutcome::Changed | InputOutcome::Refused => Taste::Geaendert,
            InputOutcome::Emptied | InputOutcome::Escape | InputOutcome::EnterEmpty => {
                self.eingabe = None;
                Taste::Geaendert
            }
            InputOutcome::Enter => {
                let a = i.value(0).and_then(Result::ok);
                if a.is_some_and(|a| self.abstand_setzen(a)) {
                    self.eingabe = None;
                    Taste::Setzen
                } else {
                    if let Some(i) = self.eingabe.as_mut() {
                        i.error = Some(crate::meldung::Meldung::satz(PASST_NICHT));
                    }
                    Taste::PasstNicht
                }
            }
        }
    }

    /// Rückt die Tür der Vorschau so, dass ihre Kante `a` mm von der
    /// näheren Wandfläche steht. `false`, wenn sie dann nicht in die Wand
    /// passt.
    pub fn abstand_setzen(&mut self, a: f64) -> bool {
        let Some(v) = self.hover.as_mut() else {
            return false;
        };
        let w = v.u.1 - v.u.0;
        let (s0, s1) = v.face.span;
        let u0 = match naeher(v).0 {
            Seite::Anfang => s0 + a,
            Seite::Ende => s1 - a - w,
        };
        let (f0, f1) = v.face.free;
        if u0 < f0 - 1e-6 || u0 + w > f1 + 1e-6 {
            return false;
        }
        v.u = (u0, u0 + w);
        let m = v.face.at(u0 + w / 2.0, 0.0) - v.face.n * (v.dicke / 2.0);
        v.punkt = vec3(m.x, m.y, v.z);
        true
    }

    /// Pille an der Maßkette: der Abstand zur näheren Wandfläche, von der
    /// Fläche zur Kante der Tür.
    pub fn label(&self) -> Option<([Vec3; 2], String)> {
        let v = self.hover.as_ref()?;
        let (seite, a) = naeher(v);
        if a < 1.0 {
            return None;
        }
        let (s0, s1) = v.face.span;
        let (von, bis) = match seite {
            Seite::Anfang => (s0, v.u.0),
            Seite::Ende => (s1, v.u.1),
        };
        let auf = kette_auf(v);
        Some(([auf(von), auf(bis)], crate::ui::live_length_text(a)))
    }

    fn vorschau(
        &self,
        m: &Model,
        k: Koerper,
        st: StoreyId,
        p: Vec3,
        tol: f64,
        z: f64,
    ) -> Option<Vorschau> {
        let (face, u, auf_flaeche, dicke) = treffer(m, k, st, p, tol)?;
        let fs = flaechenseite(&face);
        // Raum auf der Seite des Cursors, zur Pfadrichtung (= Flächenrichtung)
        let raum = if auf_flaeche { fs } else { fs.other() };
        let w = self.breite_mm();
        let uc = (u / RASTER).round() * RASTER;
        // Aus dem Raum rechts der Richtung zur Wand gesehen liegt das
        // kleinere u links
        let links_klein = raum == FaceSide::Right;
        let u0 = match (self.bezug, links_klein) {
            (Bezug::Mitte, _) => uc - w / 2.0,
            (Bezug::Links, true) | (Bezug::Rechts, false) => uc,
            (Bezug::Links, false) | (Bezug::Rechts, true) => uc - w,
        };
        let (f0, f1) = face.free;
        if f1 - f0 < w {
            return None;
        }
        let u0 = u0.clamp(f0, f1 - w);
        let mitte = face.at(uc.clamp(f0, f1), 0.0) - face.n * (dicke / 2.0);
        Some(Vorschau {
            u: (u0, u0 + w),
            swing: Swing {
                side: raum,
                din: Din::Links,
            },
            dicke,
            z,
            punkt: vec3(mitte.x, mitte.y, z),
            face,
        })
    }

    /// Die Tür der Vorschau als Öffnung.
    pub fn oeffnung(&self) -> Option<Opening> {
        let v = self.hover.as_ref()?;
        let st = v.face.storey;
        let flat = |p: Vec3| vec3(p.x, p.y, 0.0);
        Some(Opening {
            kind: OpeningKind::Door,
            path: vec![flat(v.face.at(v.u.0, 0.0)), flat(v.face.at(v.u.1, 0.0))],
            bottom: HeightRef::Level(LevelRef::bottom(st)),
            top: HeightRef::Line(st, HeightLine::Door),
            reveal: None,
            install: Default::default(),
            swing: Some(v.swing),
            lock: Default::default(),
        })
    }
}

/// Umriss der Vorschau als Strecken in Zeichenhöhe: die Pfosten quer durch
/// die Wand, das offene Blatt am Bandpfosten und der Viertelkreis.
pub fn umriss(v: &Vorschau) -> Vec<(Vec3, Vec3)> {
    let f = &v.face;
    let hoch = |p: Vec3| vec3(p.x, p.y, v.z);
    let auf = |u: f64, tiefe: f64| hoch(f.at(u, 0.0) - f.n * tiefe);
    let mut out = vec![
        (auf(v.u.0, 0.0), auf(v.u.0, v.dicke)),
        (auf(v.u.1, 0.0), auf(v.u.1, v.dicke)),
    ];
    // Blatt auf der Wandseite des Aufschlags
    let tiefe = if v.swing.side == flaechenseite(f) {
        0.0
    } else {
        v.dicke
    };
    let (ub, ug) = if v.swing.hinge_at_start() {
        (v.u.0, v.u.1)
    } else {
        (v.u.1, v.u.0)
    };
    let w = v.u.1 - v.u.0;
    let band = auf(ub, tiefe);
    let offen = v.swing.side.normal(f.d) * w;
    let zu = f.d * (ug - ub);
    let mut vorher = band + offen;
    out.push((band, vorher));
    for k in 1..=BOGEN {
        let t = std::f64::consts::FRAC_PI_2 * k as f64 / BOGEN as f64;
        let p = band + offen * t.cos() + zu * t.sin();
        out.push((vorher, p));
        vorher = p;
    }
    out
}

/// Satz, wenn die Tür im getippten Abstand nicht in die Wand passt.
pub const PASST_NICHT: &str = "So passt die Tür nicht in die Wand";

/// Seite der Wand längs, an der eine Fläche liegt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seite {
    Anfang,
    Ende,
}

/// Nähere Fläche der anschließenden Wände und der Abstand der Tür zu ihr.
fn naeher(v: &Vorschau) -> (Seite, f64) {
    let (s0, s1) = v.face.span;
    let (a0, a1) = (v.u.0 - s0, s1 - v.u.1);
    if a0 <= a1 {
        (Seite::Anfang, a0)
    } else {
        (Seite::Ende, a1)
    }
}

/// Punkt der Maßkette bei `u`: neben der Wandseite gegenüber dem
/// Aufschlag, [`KETTE`] vor ihrer Fläche, in Zeichenhöhe.
fn kette_auf(v: &Vorschau) -> impl Fn(f64) -> Vec3 + '_ {
    let f = &v.face;
    // Die Fläche liegt zur Normale `n`; schlägt die Tür dorthin auf, steht
    // die Kette vor der Gegenfläche
    let vor_der_flaeche = v.swing.side != flaechenseite(f);
    let quer = if vor_der_flaeche {
        f.n * KETTE
    } else {
        -f.n * (v.dicke + KETTE)
    };
    move |u: f64| {
        let p = f.at(u, 0.0) + quer;
        vec3(p.x, p.y, v.z)
    }
}

/// Maßkette der Vorschau als Strecken in Zeichenhöhe: von der Fläche der
/// anschließenden Wand bis zur Tür auf beiden Seiten, mit Schrägstrichen
/// an Flächen und Kanten.
pub fn kette(v: &Vorschau) -> Vec<(Vec3, Vec3)> {
    let (s0, s1) = v.face.span;
    if s1 - s0 < v.u.1 - v.u.0 {
        return Vec::new();
    }
    let auf = kette_auf(v);
    let schraeg = (v.face.d + v.face.n) * (STRICH / std::f64::consts::SQRT_2);
    let mut out = vec![(auf(s0), auf(v.u.0)), (auf(v.u.1), auf(s1))];
    for u in [s0, v.u.0, v.u.1, s1] {
        let p = auf(u);
        out.push((p - schraeg, p + schraeg));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{RefSide, RunId};

    /// Prüfhaus 10 × 8 m mit Innenwand bei x = 5 m (Zeichenrichtung +y).
    fn haus() -> (Model, RunId, StoreyId) {
        let mut m = Model::with_seed(411);
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
        (m, iw, st)
    }

    /// Über der Innenwand hängt die Tür mit der Mitte am Cursor, 88,5 cm
    /// breit, und schlägt in den Raum des Cursors auf; neben der Wand und
    /// über der Außenwand nichts. Der Klick ergibt eine Tür, die das
    /// Modell annimmt, mit der Oberkante an der Innentürlinie.
    #[test]
    fn vorschau_am_cursor() {
        let (m, iw, st) = haus();
        let mut t = Innentuer::default();
        t.suchen(
            &m,
            &|r| m.chain(r),
            st,
            vec3(5020.0, 3004.0, 0.0),
            30.0,
            1500.0,
        );
        let v = t.hover.clone().unwrap();
        assert_eq!(v.face.run, iw);
        assert_eq!(v.u.1 - v.u.0, 885.0);
        let mitte = (v.face.at(v.u.0, 0.0) + v.face.at(v.u.1, 0.0)) * 0.5;
        assert!((mitte.y - 3000.0).abs() < 1e-6, "{mitte:?}");
        // Cursor bei x > 5 m: Aufschlag nach +x
        let into = v.swing.side.normal(v.face.d);
        assert!(into.x > 0.5, "{into:?}");
        let o = t.oeffnung().unwrap();
        assert_eq!(o.top, HeightRef::Line(st, HeightLine::Door));
        assert_eq!(m.opening_fault(&o, None), None);
        // Andere Wandseite: Aufschlag nach −x
        t.suchen(
            &m,
            &|r| m.chain(r),
            st,
            vec3(4980.0, 3000.0, 0.0),
            30.0,
            1500.0,
        );
        let v = t.hover.clone().unwrap();
        assert!(v.swing.side.normal(v.face.d).x < -0.5);
        // Neben der Wand, über der Außenwand: keine Vorschau
        t.suchen(
            &m,
            &|r| m.chain(r),
            st,
            vec3(4000.0, 3000.0, 0.0),
            30.0,
            1500.0,
        );
        assert!(t.hover.is_none());
        t.suchen(
            &m,
            &|r| m.chain(r),
            st,
            vec3(0.0, 3000.0, 0.0),
            30.0,
            1500.0,
        );
        assert!(t.hover.is_none());
        // Umriss: zwei Pfosten, Blatt und Bogen, alles in Zeichenhöhe
        t.suchen(
            &m,
            &|r| m.chain(r),
            st,
            vec3(5020.0, 3000.0, 0.0),
            30.0,
            1500.0,
        );
        let u = umriss(t.hover.as_ref().unwrap());
        assert_eq!(u.len(), 3 + BOGEN);
        assert!(u.iter().all(|(a, b)| a.z == 1500.0 && b.z == 1500.0));
    }

    /// **Tab** schaltet den Bezugspunkt Mitte, links, rechts; links und
    /// rechts wie aus dem Raum des Cursors zur Wand gesehen. Das Mausrad
    /// blättert durch die Normbreiten und bleibt an den Enden. Die
    /// Statuszeile sagt Breite und Bezugspunkt (Einstellungen #99, B1).
    #[test]
    fn tab_und_mausrad() {
        let (m, _, st) = haus();
        let mut t = Innentuer::default();
        let p = vec3(5020.0, 3000.0, 0.0);
        let lichte = |t: &mut Innentuer| {
            t.suchen(&m, &|r| m.chain(r), st, p, 30.0, 1500.0);
            let v = t.hover.clone().unwrap();
            let (a, b) = (v.face.at(v.u.0, 0.0).y, v.face.at(v.u.1, 0.0).y);
            (a.min(b), a.max(b))
        };
        assert_eq!(lichte(&mut t), (2557.5, 3442.5));
        assert_eq!(t.anzeige(), "Tür 88,5 cm · Bezug Mitte");
        // Aus dem Raum bei +x zur Wand geschaut (Blick nach −x) liegt
        // links −y: die linke Kante am Cursor, die Tür geht nach +y
        t.tab();
        assert_eq!(t.bezug, Bezug::Links);
        assert_eq!(t.anzeige(), "Tür 88,5 cm · Bezug links");
        assert_eq!(lichte(&mut t), (3000.0, 3885.0));
        t.tab();
        assert_eq!(lichte(&mut t), (2115.0, 3000.0));
        t.tab();
        assert_eq!(t.bezug, Bezug::Mitte);
        assert!(t.rad(1.0));
        assert_eq!(t.breite_mm(), 1010.0);
        assert!(!t.rad(1.0), "am Ende");
        for _ in 0..5 {
            t.rad(-1.0);
        }
        assert_eq!(t.breite_mm(), 635.0);
        assert_eq!(t.anzeige(), "Tür 63,5 cm · Bezug Mitte");
        t.rad(1.0);
        assert_eq!(t.anzeige(), "Tür 76 cm · Bezug Mitte");
        t.rad(-1.0);
        assert_eq!(lichte(&mut t), (2682.5, 3317.5));
    }

    /// Grundriss G7: Die Maßkette steht auf der Wandseite gegenüber dem
    /// Aufschlag und reicht von den Flächen der anschließenden Wände bis zur
    /// Tür; die Pille zeigt den Abstand zur näheren. Eine Ziffer öffnet die
    /// Eingabe, **Enter** rückt die Tür so weit von der näheren Fläche, an
    /// beiden Enden der Wand. Passt sie so nicht, bleibt die Eingabe mit
    /// Fehler; **Esc** schließt sie, **Tab** geht weiter an den Bezugspunkt.
    #[test]
    fn kette_und_abstand() {
        let (m, _, st) = haus();
        let mut t = Innentuer::default();
        let k = |r| m.chain(r);
        let mods = Modifiers::default();
        let tippe = |t: &mut Innentuer, s: &str| {
            for ch in s.chars() {
                assert_eq!(t.taste(Key::Char(ch), mods), Taste::Geaendert, "{s}");
            }
        };
        t.suchen(&m, &k, st, vec3(5020.0, 3000.0, 0.0), 30.0, 1500.0);
        let v = t.hover.clone().unwrap();
        let (s0, s1) = v.face.span;
        let (f0, f1) = v.face.free;
        assert!(s0 < f0 && f1 < s1, "{:?} {:?}", v.face.span, v.face.free);
        // Die anschließenden Außenwände: innen bei y = 0,315 m und 7,685 m
        let y = |u: f64| v.face.at(u, 0.0).y;
        let (ya, yb) = (y(s0).min(y(s1)), y(s0).max(y(s1)));
        assert!((ya - 315.0).abs() < 1e-6 && (yb - 7685.0).abs() < 1e-6);
        // Aufschlag nach +x, die Kette steht bei x < 5 m vor der Gegenfläche
        let kette = kette(&v);
        assert_eq!(kette.len(), 2 + 4);
        let x_gegen = 5000.0 - v.dicke / 2.0;
        assert!(kette
            .iter()
            .all(|(a, b)| a.x < x_gegen && b.x < x_gegen && a.z == 1500.0 && b.z == 1500.0));
        assert!((kette[0].0.x - (x_gegen - KETTE)).abs() < 1e-6, "{kette:?}");
        // Pille: zur näheren Fläche (unten, y = 0,315 m)
        let (ends, text) = t.label().unwrap();
        assert_eq!(text, crate::ui::live_length_text(2557.5 - 315.0));
        assert!((ends[0].y - 315.0).abs() < 1e-6 && (ends[1].y - 2557.5).abs() < 1e-6);
        // 0,5 m und Enter: die Tür 50 cm von der Fläche, das Modell nimmt sie
        tippe(&mut t, "0,5");
        assert!(t.eingabe.is_some());
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        assert!(t.eingabe.is_none());
        let v = t.hover.clone().unwrap();
        let lichte = |v: &Vorschau| {
            let (a, b) = (y(v.u.0), y(v.u.1));
            (a.min(b), a.max(b))
        };
        assert_eq!(lichte(&v), (815.0, 1700.0));
        assert!((v.punkt.y - 1257.5).abs() < 1e-6, "{:?}", v.punkt);
        assert_eq!(m.opening_fault(&t.oeffnung().unwrap(), None), None);
        // Oben ist die obere Fläche die nähere
        t.suchen(&m, &k, st, vec3(5020.0, 6500.0, 0.0), 30.0, 1500.0);
        tippe(&mut t, "0,3");
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        assert_eq!(lichte(t.hover.as_ref().unwrap()), (6500.0, 7385.0));
        // Zu weit: bleibt offen, mit Fehler
        tippe(&mut t, "7,2");
        assert_eq!(t.taste(Key::Enter, mods), Taste::PasstNicht);
        assert!(t.eingabe.as_ref().unwrap().error.is_some());
        assert_eq!(t.taste(Key::Tab, mods), Taste::Frei, "Tab: Bezugspunkt");
        assert_eq!(t.taste(Key::Escape, mods), Taste::Geaendert);
        assert!(t.eingabe.is_none());
        assert_eq!(t.taste(Key::Escape, mods), Taste::Frei, "dann schließt Esc");
        // Fort von der Wand: keine Eingabe, keine Pille, Ziffern gehen weiter
        tippe(&mut t, "1");
        t.suchen(&m, &k, st, vec3(3000.0, 3000.0, 0.0), 30.0, 1500.0);
        assert!(t.eingabe.is_none() && t.label().is_none());
        assert_eq!(t.taste(Key::Char('1'), mods), Taste::Frei);
    }
}

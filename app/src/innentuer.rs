//! Werkzeug „Innentür“ im Grundriss (Jörn 12.10. 06:32–06:33): Knopf
//! „Tür“ im Grundriss. Über einer Innenwand hängt die Tür als Vorschau am
//! Cursor, mit Lichte, offenem Blatt und Viertelkreis; sie schlägt in den
//! Raum auf, auf dessen Seite der Cursor steht. Bezugspunkt ist die
//! Türmitte, **Tab** schaltet zu links und rechts (aus diesem Raum zur Wand
//! gesehen); das Mausrad blättert durch die Normbreiten nach DIN 18101. Ein
//! Klick setzt die Tür: unten OK Rohdecke, oben die Innentürlinie, DIN
//! links.
//!
//! Hier steht die reine Rechnung; Maus und Szene in `oeffnung_app.rs`.

use sk_math::{vec3, Vec3};
use sk_model::hoehen::DOOR_WIDTHS;
use sk_model::oeffnung::{
    Din, FaceSide, HeightLine, HeightRef, Opening, OpeningKind, OuterFace, Swing,
};
use sk_model::{Category, LevelRef, Model, RunId, StoreyId, WallChain};

/// Raster längs der Wand für den Bezugspunkt (mm).
const RASTER: f64 = 10.0;
/// Stücke des Viertelkreises in der Vorschau.
const BOGEN: usize = 12;
/// Vorgabe der Breite: 88,5 cm.
const BREITE: usize = 2;

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
}

impl Default for Innentuer {
    fn default() -> Innentuer {
        Innentuer {
            breite: BREITE,
            bezug: Bezug::Mitte,
            hover: None,
            ort: None,
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
const DICKE: f64 = 1000.0;

/// Liegt `p` im Rechteck um die Punkte `pts`, um `rand` vergrößert?
fn nah(pts: &[Vec3], p: Vec3, rand: f64) -> bool {
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
    let p = vec3(p.x, p.y, 0.0);
    let mut best: Option<(f64, (OuterFace, f64, bool, f64))> = None;
    for (id, run) in m.runs().iter() {
        let innen = run
            .segments
            .first()
            .and_then(|e| m.element(*e))
            .is_some_and(|e| e.category == Category::InteriorWall);
        if run.storey != st || !innen || !nah(&run.points, p, tol + DICKE) {
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
    }

    /// Sucht an der letzten Stelle neu (nach Tab, Mausrad oder Klick).
    pub fn nochmal(&mut self, m: &Model, k: Koerper, st: StoreyId) {
        if let Some((p, tol, z)) = self.ort {
            self.suchen(m, k, st, p, tol, z);
        }
    }

    /// Maus fort: keine Vorschau.
    pub fn weg(&mut self) {
        self.ort = None;
        self.hover = None;
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
}

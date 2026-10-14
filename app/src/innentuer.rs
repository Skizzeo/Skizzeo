//! Werkzeug „Innentür“ im Grundriss (Jörn 12.10. 06:32–06:33): Knopf
//! „Tür“ im Grundriss. Über einer Innenwand hängt die Tür als Vorschau am
//! Cursor, mit Lichte, offenem Blatt und Viertelkreis; sie schlägt in den
//! Raum auf, auf dessen Seite der Cursor steht. Bezugspunkt ist die
//! Türmitte, **Tab** schaltet zu links und rechts (aus diesem Raum zur Wand
//! gesehen); das Mausrad blättert durch die Normbreiten nach DIN 18101. Ein
//! Klick setzt die Tür: unten OK Rohdecke, oben die Innentürlinie, DIN
//! links. Eine Maßkette auf der Wandseite gegenüber dem Aufschlag zeigt
//! die Abstände zu den Flächen der anschließenden Wände und die Breite,
//! gezeichnet wie die Außenmaßketten (Bemaßung Stufe 2) in der Ziehfarbe;
//! die Pille zeigt den Abstand zur näheren. Eine Ziffer öffnet dort die
//! Eingabe, **Enter** setzt die Tür in diesem Abstand (plan.md §4.3,
//! Grundriss G7).
//!
//! Hier steht die reine Rechnung; Maus und Szene in `oeffnung_app.rs`.

use crate::measure_input::{opens, InputOutcome, MeasureInput};
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::bemassung::{Aussenbezug, Kettenart, Stil};
use sk_model::hoehen::DOOR_WIDTHS;
use sk_model::join::{JoinEnd, JoinKind};
use sk_model::oeffnung::{
    Din, FaceSide, HeightLine, HeightRef, Opening, OpeningKind, OuterFace, Swing,
};
use sk_model::{Category, LevelRef, Model, RunId, StoreyId, WallChain};
use sk_plan::aussen::{
    flaechen, Aussenmasse, Bezug as Massbezug, Herkunft, Kette, Seite as Massseite,
};
use sk_plan::bild::{bild, Bild};
use sk_plan::schrift::Schrift;
use sk_plan::zahl::masszahl;
use sk_platform::{Key, Modifiers};

/// Raster längs der Wand für den Bezugspunkt (mm).
const RASTER: f64 = 10.0;
/// Stücke des Viertelkreises in der Vorschau.
const BOGEN: usize = 12;
/// Vorgabe der Breite: 88,5 cm.
const BREITE: usize = 2;
/// Abstand der Maßkette von der Wandfläche (mm).
const KETTE: f64 = 250.0;
/// Felder der Pille beim Tippen des Abstands.
pub const FELDER: [&str; 2] = ["Abstand", ""];

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
    /// Fläche auf der Seite der Kette längs (u, aufsteigend) bis zu ihren
    /// Ecken; am Knick des Zugs außen bis zur Außenecke (Review W49 H2).
    pub seite: (f64, f64),
    /// Wände, die auf der Seite der Kette im T-Stoß anschließen: je die
    /// Spanne zwischen ihren Flächen längs, aufsteigend (Review W49 H1).
    pub stoesse: Vec<(f64, f64)>,
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
    /// Zuletzt gerechnete Seite der Kette.
    merk: Option<KettenMerk>,
    /// Wie oft die Vorschau die Seite der Kette neu gerechnet hat.
    #[cfg(test)]
    neu_gerechnet: usize,
}

/// Seite der Kette ([`kettenseite`]) mit allem, wovon sie abhängt. Sie geht
/// über alle Anschlüsse des Modells; längs derselben Wandfläche und beim
/// selben Stand des Modells nimmt die Vorschau sie von hier, statt sie je
/// Mausbewegung neu zu rechnen (Review #158 B1).
#[derive(Clone, Debug, PartialEq)]
struct KettenMerk {
    face: OuterFace,
    vor: bool,
    dicke: f64,
    bezug: Aussenbezug,
    revision: u64,
    seite: (f64, f64),
    stoesse: Vec<(f64, f64)>,
}

/// Was eine Taste mit der Tür am Cursor bewirkt hat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taste {
    /// Nicht für die Eingabe, sie geht weiter (Tab, Esc ohne Eingabe).
    Frei,
    /// Eingabe geöffnet, geändert oder geschlossen.
    Geaendert,
    /// Enter: Die Tür steht im getippten Abstand und wird gesetzt; die
    /// Eingabe bleibt, bis [`Innentuer::pruefen`] die Tür annimmt.
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
            merk: None,
            #[cfg(test)]
            neu_gerechnet: 0,
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

/// Vorfilter für [`wand_unter`] (mm): So weit darf eine Wandfläche von der
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

/// Wie [`treffer_in`], doch unter allen Zügen des Modells mit den Körpern
/// aus `k`: der Vergleich in den Tests.
#[cfg(test)]
pub fn treffer(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    let zuege = m.runs().ids();
    wand_unter(m, k, st, p, tol, zuege, |c| c == Category::InteriorWall)
}

/// Innenwand des Geschosses `st` unter dem Grundrisspunkt `p` (Toleranz
/// `tol` in mm): ihre Fläche, die Lage längs, ob `p` auf der Seite der
/// Fläche liegt, und die Wanddicke. Die nächste Wandmitte gewinnt. Über das
/// Raster der Szene (Review #158 H1): nur unter den Zügen, deren Quader bis
/// `tol` an `p` reicht, statt über alle Züge des Modells.
pub fn treffer_in(
    scene: &Scene,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    let k = |r| scene.koerper(r);
    let zuege = zuege_bei(scene, p, tol);
    wand_unter(scene.model(), &k, st, p, tol, zuege, |c| {
        c == Category::InteriorWall
    })
}

/// Wie [`treffer_aussen_in`] unter allen Zügen des Modells.
#[cfg(test)]
pub fn treffer_aussen(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    let zuege = m.runs().ids();
    wand_unter(m, k, st, p, tol, zuege, Category::is_outer_wall)
}

/// Wie [`treffer_in`] für Außenwände: Fenster und Außentür im Grundriss
/// (Jörn 12.10. 09:26); die Fläche ist die Außenfläche.
pub fn treffer_aussen_in(
    scene: &Scene,
    st: StoreyId,
    p: Vec3,
    tol: f64,
) -> Option<(OuterFace, f64, bool, f64)> {
    let k = |r| scene.koerper(r);
    let zuege = zuege_bei(scene, p, tol);
    wand_unter(
        scene.model(),
        &k,
        st,
        p,
        tol,
        zuege,
        Category::is_outer_wall,
    )
}

/// Züge der Szene, deren Quader bis `tol` an `p` reicht, aufsteigend wie
/// [`Model::runs`]. Eine Fläche, auf die [`wand_unter`] `p` nimmt, ist
/// eine Seite des Körpers ihres Zugs, und `p` liegt höchstens `tol` neben
/// ihr; der Körper steckt im Quader.
fn zuege_bei(scene: &Scene, p: Vec3, tol: f64) -> Vec<RunId> {
    let r = tol;
    let mut zuege: Vec<RunId> = scene
        .zuege_im((p.x - r, p.y - r), (p.x + r, p.y + r))
        .map(|(id, ..)| id)
        .collect();
    zuege.sort_by_key(|id| id.index());
    zuege.dedup();
    zuege
}

/// Wand des Geschosses `st` mit einer Kategorie nach `art` unter `p`, unter
/// den Zügen `zuege` (aufsteigend; bei gleichem Abstand gewinnt der erste).
fn wand_unter(
    m: &Model,
    k: Koerper,
    st: StoreyId,
    p: Vec3,
    tol: f64,
    zuege: impl IntoIterator<Item = RunId>,
    art: impl Fn(Category) -> bool,
) -> Option<(OuterFace, f64, bool, f64)> {
    let p = vec3(p.x, p.y, 0.0);
    let mut best: Option<(f64, (OuterFace, f64, bool, f64))> = None;
    for id in zuege {
        let Some(run) = m.run(id) else {
            continue;
        };
        // Das Bauteil erst bei Zügen nahe `p` (Review #140, Zugschritt)
        if run.storey != st || !nah(&run.points, p, tol + DICKE) {
            continue;
        }
        let passt = run
            .segments
            .first()
            .and_then(|e| m.element(*e))
            .is_some_and(|e| art(e.category));
        if !passt {
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

    /// Wie [`Innentuer::suchen_in`], die Wand über alle Züge des Modells
    /// ([`treffer`]).
    #[cfg(test)]
    pub fn suchen(&mut self, m: &Model, k: Koerper, st: StoreyId, p: Vec3, tol: f64, z: f64) {
        let wand = treffer(m, k, st, p, tol);
        self.zeigen(m, k, wand, (p, tol, z));
    }

    /// Vorschau für den Grundrisspunkt `p` im Geschoss `st`, gezeichnet in
    /// Höhe `z`; `None` neben jeder Innenwand oder wenn die Tür dort nicht
    /// in die Wandfläche passt. Die Wand über das Raster der Szene
    /// ([`treffer_in`], Review #158 H1).
    pub fn suchen_in(&mut self, scene: &Scene, st: StoreyId, p: Vec3, tol: f64, z: f64) {
        let wand = treffer_in(scene, st, p, tol);
        self.zeigen(scene.model(), &|r| scene.koerper(r), wand, (p, tol, z));
    }

    /// Vorschau an der Wand `wand` für die Suche `ort` (Punkt, Toleranz,
    /// Zeichenhöhe).
    fn zeigen(
        &mut self,
        m: &Model,
        k: Koerper,
        wand: Option<(OuterFace, f64, bool, f64)>,
        ort: (Vec3, f64, f64),
    ) {
        self.ort = Some(ort);
        self.hover = wand.and_then(|w| self.vorschau(m, k, w, ort.2));
        // Fort von der Innenwand: die Eingabe hat keine Tür mehr
        if self.hover.is_none() {
            self.eingabe = None;
        }
    }

    /// Sucht an der letzten Stelle neu (nach Tab, Mausrad oder Klick).
    pub fn nochmal_in(&mut self, scene: &Scene, st: StoreyId) {
        if let Some((p, tol, z)) = self.ort {
            self.suchen_in(scene, st, p, tol, z);
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
        let (s0, s1) = spanne(v);
        let u0 = match naeher(v).0 {
            Seite::Anfang => s0 + a,
            Seite::Ende => s1 - a - w,
        };
        // In der Wand und zwischen den Flächen der Kette, also nicht über
        // eine Wand im T-Stoß hinweg
        let (f0, f1) = v.face.free;
        if u0 < f0.max(s0) - 1e-6 || u0 + w > f1.min(s1) + 1e-6 {
            return false;
        }
        v.u = (u0, u0 + w);
        let m = v.face.at(u0 + w / 2.0, 0.0) - v.face.n * (v.dicke / 2.0);
        v.punkt = vec3(m.x, m.y, v.z);
        true
    }

    /// Pille an der Maßkette: der Abstand zur näheren Wandfläche, von der
    /// Fläche zur Kante der Tür, als Maßzahl auf `rundung` mm, beim Tippen
    /// die Eingabe nach jeder Taste (Review W49 B1).
    pub fn label(&self, rundung: u8) -> Option<([Vec3; 2], String)> {
        let v = self.hover.as_ref()?;
        let (seite, a) = naeher(v);
        let text = match &self.eingabe {
            Some(i) => i.text(FELDER),
            None if a >= 1.0 => masszahl(a, rundung).text(),
            None => return None,
        };
        let (s0, s1) = spanne(v);
        let (von, bis) = match seite {
            Seite::Anfang => (s0, v.u.0),
            Seite::Ende => (s1, v.u.1),
        };
        let auf = kette_auf(v);
        Some(([auf(von), auf(bis)], text))
    }

    /// Die Tür der Vorschau, wenn das Modell sie nimmt (Klick oder
    /// **Enter**); dann schließt die Eingabe. Lehnt es ab, steht der Grund
    /// in `Err`, und eine getippte Zahl bleibt mit diesem Fehler offen, die
    /// Tür im getippten Abstand (Review W49 B2). `None` ohne Vorschau.
    pub fn pruefen(&mut self, m: &Model) -> Option<Result<Opening, String>> {
        let o = self.oeffnung()?;
        match m.opening_fault(&o, None).map(|f| f.text(m)) {
            None => {
                self.eingabe = None;
                Some(Ok(o))
            }
            Some(t) => {
                if let Some(i) = self.eingabe.as_mut() {
                    i.error = Some(crate::meldung::Meldung::mit("{}.", &[&t]));
                }
                Some(Err(t))
            }
        }
    }

    fn vorschau(
        &mut self,
        m: &Model,
        k: Koerper,
        (face, u, auf_flaeche, dicke): (OuterFace, f64, bool, f64),
        z: f64,
    ) -> Option<Vorschau> {
        let fs = flaechenseite(&face);
        // Raum auf der Seite des Cursors, zur Pfadrichtung (= Flächenrichtung)
        let raum = if auf_flaeche { fs } else { fs.other() };
        let w = self.breite_mm();
        let uc = (u / RASTER).round() * RASTER;
        // Aus dem Raum rechts der Richtung zur Wand gesehen liegt das
        // kleinere u links
        let links_klein = raum == FaceSide::Right;
        let u0 = match (self.bezug, links_klein) {
            // Auch bei Mitte die Kante im Raster: Bei 88,5 cm lägen die
            // Kanten sonst auf halben Millimetern, und die Teilmaße der
            // Kette ergäben gerundet nicht das Ganze (Bedienbarkeit 41.1)
            (Bezug::Mitte, _) => ((u - w / 2.0) / RASTER).round() * RASTER,
            (Bezug::Links, true) | (Bezug::Rechts, false) => uc,
            (Bezug::Links, false) | (Bezug::Rechts, true) => uc - w,
        };
        let uc = if self.bezug == Bezug::Mitte {
            u0 + w / 2.0
        } else {
            uc
        };
        let (f0, f1) = face.free;
        if f1 - f0 < w {
            return None;
        }
        let u0 = u0.clamp(f0, f1 - w);
        let mitte = face.at(uc.clamp(f0, f1), 0.0) - face.n * (dicke / 2.0);
        // Die Kette steht gegenüber dem Aufschlag
        let (vor, bezug, revision) = (raum != fs, m.massstil().bezug, m.revision());
        let gilt = |g: &KettenMerk| {
            g.face == face
                && g.vor == vor
                && g.dicke == dicke
                && g.bezug == bezug
                && g.revision == revision
        };
        let (seite, stoesse) = match self.merk.as_ref().filter(|g| gilt(g)) {
            Some(g) => (g.seite, g.stoesse.clone()),
            None => {
                let (seite, stoesse) = kettenseite(m, k, &face, vor, dicke, bezug);
                self.merk = Some(KettenMerk {
                    face: face.clone(),
                    vor,
                    dicke,
                    bezug,
                    revision,
                    seite,
                    stoesse: stoesse.clone(),
                });
                #[cfg(test)]
                {
                    self.neu_gerechnet += 1;
                }
                (seite, stoesse)
            }
        };
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
            seite,
            stoesse,
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

/// Flächen der anschließenden Wände links und rechts der Tür auf der Seite
/// der Kette (u, aufsteigend): die Enden der Fläche oder die nächste Wand im
/// T-Stoß davor (Review W49 H1, H2).
pub fn spanne(v: &Vorschau) -> (f64, f64) {
    let (mut s0, mut s1) = v.seite;
    for &(a, b) in &v.stoesse {
        if b <= v.u.0 + 1e-6 {
            s0 = s0.max(b);
        }
        if a >= v.u.1 - 1e-6 {
            s1 = s1.min(a);
        }
    }
    (s0, s1)
}

/// Fläche der Kette, vor der Fläche `f` (`vor`) oder vor der Gegenfläche im
/// Abstand `dicke`, nach dem Bezug der Außenmaße `bezug` (Reiter
/// „Bemaßung“; Bedienbarkeit 41.2): beim ganzen Wandpaket die fertigen
/// Flächen, bei tragender Schicht das Mauerwerk. Ergebnis: ihre Spanne
/// längs bis zu ihren Ecken, an einem Ende des Zugs bis zur Fläche der
/// Wand, an die er dort stößt, und die Wände, die auf dieser Seite im
/// T-Stoß anschließen, je mit der Spanne zwischen ihren Flächen.
fn kettenseite(
    m: &Model,
    k: Koerper,
    f: &OuterFace,
    vor: bool,
    dicke: f64,
    bezug: Aussenbezug,
) -> ((f64, f64), Vec<(f64, f64)>) {
    let flach = |p: Vec3| vec3(p.x, p.y, 0.0);
    let u_of = |p: Vec3| (flach(p) - f.a).dot(f.d);
    let Some(c) = k(f.run) else {
        return (f.span, Vec::new());
    };
    // Linie der Kette: die Fläche nach dem Bezug auf ihrer Seite
    let (aussen, innen) = flaechen(&c, bezug);
    let cs = c.face_corners(if vor { aussen } else { innen });
    let j = (f.seg + 1) % cs.len().max(1);
    let (Some(&e0), Some(&e1)) = (cs.get(f.seg), cs.get(j)) else {
        return (f.span, Vec::new());
    };
    let q = flach(e0);
    let kreuz = |a: Vec3, b: Vec3| a.x * b.y - a.y * b.x;
    // u, wo die Linie durch `p` und `n` die Linie der Kette schneidet
    let schnitt = |p: Vec3, n: Vec3| {
        let r = flach(n) - flach(p);
        let nenner = kreuz(f.d, r);
        (nenner.abs() > 1e-9 * r.length()).then(|| kreuz(flach(p) - q, r) / nenner + u_of(q))
    };
    // Flächen nach dem Bezug am Segment `sg` der Wand `co`, als u
    let flaechen_an = |co: &WallChain, sg: usize| -> Vec<f64> {
        let (a, b) = flaechen(co, bezug);
        [a, b]
            .into_iter()
            .filter_map(|off| {
                let cs = co.face_corners(off);
                let n = (sg + 1) % cs.len().max(1);
                schnitt(*cs.get(sg)?, *cs.get(n)?)
            })
            .collect()
    };
    let ende_seg = |co: &WallChain, e: JoinEnd| match e {
        JoinEnd::Start => 0,
        JoinEnd::End => co.segment_count().saturating_sub(1),
    };
    // Enden des Zugs: bis zur nächsten Fläche der Wand, an die er stößt
    // (beim Wandpaket ist das die eigene Ecke)
    let mut ecken = [u_of(e0), u_of(e1)];
    if !c.closed {
        for jn in m.joins() {
            let (andere, eigen, ihr_seg) = if jn.a_run == f.run && jn.b_run != f.run {
                let Some(co) = k(jn.b_run) else {
                    continue;
                };
                let sg = match (jn.kind, jn.b_end) {
                    (JoinKind::L, Some(e)) => ende_seg(&co, e),
                    _ => match m
                        .run(jn.b_run)
                        .and_then(|r| r.segments.iter().position(|e| *e == jn.b))
                    {
                        Some(sg) => sg,
                        None => continue,
                    },
                };
                (co, jn.a_end, sg)
            } else if jn.kind == JoinKind::L && jn.b_run == f.run && jn.a_run != f.run {
                let (Some(co), Some(e)) = (k(jn.a_run), jn.b_end) else {
                    continue;
                };
                let sg = ende_seg(&co, jn.a_end);
                (co, e, sg)
            } else {
                continue;
            };
            let i = match eigen {
                JoinEnd::Start if f.seg == 0 => 0,
                JoinEnd::End if f.seg == ende_seg(&c, JoinEnd::End) => 1,
                _ => continue,
            };
            let weit = andere.thickness() + c.thickness();
            if let Some(u) = flaechen_an(&andere, ihr_seg)
                .into_iter()
                .filter(|u| (u - ecken[i]).abs() <= weit)
                .min_by(|a, b| (a - ecken[i]).abs().total_cmp(&(b - ecken[i]).abs()))
            {
                ecken[i] = u;
            }
        }
    }
    let seite = (ecken[0].min(ecken[1]), ecken[0].max(ecken[1]));
    let wirt = m.run(f.run).and_then(|r| r.segments.get(f.seg).copied());
    let mut stoesse: Vec<(f64, f64)> = m
        .joins()
        .iter()
        .filter(|j| j.kind == JoinKind::T && j.b_run == f.run && Some(j.b) == wirt)
        .filter_map(|j| {
            let ca = k(j.a_run)?;
            // Nur Wände auf der Seite der Kette
            let cs = ca.face_corners(ca.outer_offset());
            let nb = match j.a_end {
                JoinEnd::Start => *cs.get(1)?,
                JoinEnd::End => *cs.get(cs.len().checked_sub(2)?)?,
            };
            let s = (flach(nb) - f.a).dot(f.n);
            if (vor && s <= 0.0) || (!vor && s >= -dicke) {
                return None;
            }
            let us = flaechen_an(&ca, ende_seg(&ca, j.a_end));
            let (a, b) = (us.first()?, us.get(1)?);
            Some((a.min(*b), a.max(*b)))
        })
        .collect();
    stoesse.sort_by(|x, y| x.0.total_cmp(&y.0));
    (seite, stoesse)
}

/// Nähere Fläche der anschließenden Wände und der Abstand der Tür zu ihr.
fn naeher(v: &Vorschau) -> (Seite, f64) {
    let (s0, s1) = spanne(v);
    let (a0, a1) = (v.u.0 - s0, s1 - v.u.1);
    if a0 <= a1 {
        (Seite::Anfang, a0)
    } else {
        (Seite::Ende, a1)
    }
}

/// Steht die Maßkette vor der Fläche der Vorschau (sonst vor der
/// Gegenfläche)? Sie steht auf der Wandseite gegenüber dem Aufschlag: Die
/// Fläche liegt zur Normale `n`; schlägt die Tür dorthin auf, steht die
/// Kette vor der Gegenfläche.
fn vor_der_flaeche(v: &Vorschau) -> bool {
    v.swing.side != flaechenseite(&v.face)
}

/// Punkt der Maßkette bei `u`: neben der Wandseite gegenüber dem
/// Aufschlag, [`KETTE`] vor ihrer Fläche, in Zeichenhöhe.
fn kette_auf(v: &Vorschau) -> impl Fn(f64) -> Vec3 + '_ {
    let f = &v.face;
    let quer = if vor_der_flaeche(v) {
        f.n * KETTE
    } else {
        -f.n * (v.dicke + KETTE)
    };
    move |u: f64| {
        let p = f.at(u, 0.0) + quer;
        vec3(p.x, p.y, v.z)
    }
}

/// Maßkette der Vorschau, gezeichnet wie die Außenmaßketten (Bemaßung
/// Stufe 2, [`sk_plan::bild::bild`]): Maßlinie, Hilfslinien, Begrenzung
/// und Maßzahlen nach dem Stil `stil` in der Schrift `schrift`, von den
/// Flächen der anschließenden Wände über die Kanten der Tür, in
/// Zeichenhöhe. Leer, wenn die Tür nicht zwischen die Flächen passt.
pub fn kette(v: &Vorschau, st: StoreyId, stil: &Stil, schrift: &Schrift) -> Bild {
    let (s0, s1) = spanne(v);
    if s1 - s0 < v.u.1 - v.u.0 {
        return Bild::default();
    }
    // Von der Wand zur Kette
    let n = if vor_der_flaeche(v) {
        v.face.n
    } else {
        -v.face.n
    };
    let n = vec3(n.x, n.y, 0.0).normalized();
    // Maßzahlen von unten oder von rechts lesbar, wie bei den Außenmaßen
    let t = sk_plan::aussen::laengs(n);
    let bezug = |u: f64, herkunft| {
        let p = v.face.at(u, 0.0);
        let p = vec3(p.x, p.y, 0.0);
        Massbezug {
            t: p.dot(t),
            p,
            herkunft,
            quelle: None,
        }
    };
    let mut bezuege = vec![
        bezug(s0, Herkunft::Innenwand),
        bezug(v.u.0, Herkunft::Oeffnung),
        bezug(v.u.1, Herkunft::Oeffnung),
        bezug(s1, Herkunft::Innenwand),
    ];
    bezuege.sort_by(|a, b| a.t.total_cmp(&b.t));
    let lage = kette_auf(v)(s0);
    let lage = vec3(lage.x, lage.y, 0.0).dot(n);
    let a = Aussenmasse {
        storey: st,
        z: v.z,
        seiten: vec![Massseite {
            grad: 0,
            n,
            t,
            aussen: lage,
            ketten: vec![Kette {
                art: Kettenart::Wand,
                bezuege,
                lage,
                versatz: 0.0,
            }],
            oeffnungen: Vec::new(),
        }],
    };
    bild(&a, stil, schrift)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{LayerFunction, MaterialLayer, RefSide, RunId, TypeCategory};
    use sk_plan::bild::Strichart;

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
        // Die linke Kante rastet (Bedienbarkeit 41.1): 2,56 bis 3,445 m
        assert!((mitte.y - 3002.5).abs() < 1e-6, "{mitte:?}");
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
        assert_eq!(lichte(&mut t), (2560.0, 3445.0));
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
        assert_eq!(lichte(&mut t), (2680.0, 3315.0));
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
        // Aufschlag nach +x, die Kette steht bei x < 5 m vor der Gegenfläche,
        // gezeichnet wie die Außenmaße: drei Maßzahlen, Fläche, Tür, Fläche
        let stil = m.massstil();
        let x_gegen = 5000.0 - v.dicke / 2.0;
        if let Some(f) =
            std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
                .ok()
                .and_then(sk_paint::font::Font::parse)
                .map(Schrift::new)
        {
            let b = kette(&v, st, stil, &f);
            let mut werte: Vec<f64> = b.zahlen.iter().map(|z| z.wert).collect();
            werte.sort_by(f64::total_cmp);
            // Kanten im Raster: die Teilmaße gehen in ganzen mm auf
            // (Bedienbarkeit 41.1), 2.245 + 885 + 4.240 = 7.370
            assert_eq!(werte, [885.0, 2245.0, 4240.0]);
            assert!(!b.tinte.is_empty());
            assert!(b.striche.iter().all(|s| s.a.x <= x_gegen + 1e-6
                && s.b.x <= x_gegen + 1e-6
                && s.a.z == 1500.0
                && s.b.z == 1500.0));
            let masslinie = b.striche.iter().filter(|s| s.art == Strichart::Masslinie);
            assert!(masslinie
                .clone()
                .all(|s| (s.a.x - (x_gegen - KETTE)).abs() < 1e-6
                    && (s.b.x - (x_gegen - KETTE)).abs() < 1e-6));
            assert!(masslinie.count() > 0);
        } else {
            eprintln!("Liberation Sans fehlt, Kette nicht geprüft");
        }
        // Pille: zur näheren Fläche (unten, y = 0,315 m), Maßzahl im Stil
        let (ends, text) = t.label(stil.rundung).unwrap();
        assert_eq!(text, masszahl(2560.0 - 315.0, stil.rundung).text());
        assert!((ends[0].y - 315.0).abs() < 1e-6 && (ends[1].y - 2560.0).abs() < 1e-6);
        // 0,5 m und Enter: die Tür 50 cm von der Fläche, das Modell nimmt sie
        tippe(&mut t, "0,5");
        assert!(t.eingabe.is_some());
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        let v = t.hover.clone().unwrap();
        let lichte = |v: &Vorschau| {
            let (a, b) = (y(v.u.0), y(v.u.1));
            (a.min(b), a.max(b))
        };
        assert_eq!(lichte(&v), (815.0, 1700.0));
        assert!((v.punkt.y - 1257.5).abs() < 1e-6, "{:?}", v.punkt);
        assert!(t.pruefen(&m).unwrap().is_ok());
        assert!(t.eingabe.is_none(), "angenommen: die Eingabe schließt");
        // Oben ist die obere Fläche die nähere
        t.suchen(&m, &k, st, vec3(5020.0, 6500.0, 0.0), 30.0, 1500.0);
        tippe(&mut t, "0,3");
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        assert_eq!(lichte(t.hover.as_ref().unwrap()), (6500.0, 7385.0));
        assert!(t.pruefen(&m).unwrap().is_ok());
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
        assert!(t.eingabe.is_none() && t.label(1).is_none());
        assert_eq!(t.taste(Key::Char('1'), mods), Taste::Frei);
    }

    /// Review W49: Die Pille zeigt nach jeder Taste die getippte Zahl (B1).
    /// Lehnt das Modell die Tür im getippten Abstand ab, hier wegen einer
    /// Tür, die dort schon steht, bleibt die Eingabe mit dem Grund offen
    /// und die Tür im getippten Abstand (B2); **Esc** schließt sie.
    #[test]
    fn eingabe_in_der_pille_und_abgelehnt() {
        let (mut m, _, st) = haus();
        let mut t = Innentuer::default();
        let mods = Modifiers::default();
        let p = vec3(5020.0, 3000.0, 0.0);
        t.suchen(&m, &|r| m.chain(r), st, p, 30.0, 1500.0);
        let mut texte = vec![t.label(1).unwrap().1];
        for ch in "0,5".chars() {
            assert_eq!(t.taste(Key::Char(ch), mods), Taste::Geaendert);
            let (_, text) = t.label(1).unwrap();
            assert_eq!(text, t.eingabe.as_ref().unwrap().text(FELDER));
            assert!(!texte.contains(&text), "{text} nach {texte:?}");
            texte.push(text);
        }
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        let o = t.pruefen(&m).unwrap().unwrap();
        m.add_opening(o).unwrap();
        // Noch einmal 0,5 m von derselben Fläche: Dort steht die Tür schon
        t.suchen(&m, &|r| m.chain(r), st, p, 30.0, 1500.0);
        for ch in "0,5".chars() {
            t.taste(Key::Char(ch), mods);
        }
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        let vorher = t.hover.clone();
        assert!(t.pruefen(&m).unwrap().is_err());
        let i = t.eingabe.as_ref().expect("die Eingabe bleibt");
        assert!(i.error.is_some());
        assert_eq!(t.label(1).unwrap().1, i.text(FELDER));
        assert_eq!(t.hover, vorher, "die Tür bleibt im getippten Abstand");
        assert_eq!(t.taste(Key::Escape, mods), Taste::Geaendert);
        assert!(t.eingabe.is_none());
    }

    /// Spanne der Kette um die Tür als y von unten nach oben (die Innenwand
    /// läuft in +y).
    fn spanne_y(v: &Vorschau) -> (f64, f64) {
        let (s0, s1) = spanne(v);
        let y = |u: f64| v.face.at(u, 0.0).y;
        (y(s0).min(y(s1)), y(s0).max(y(s1)))
    }

    /// Review W49 H1: Eine Wand, die mitten an der Innenwand im T-Stoß
    /// anschließt, begrenzt die Kette auf ihrer Seite wie eine anschließende
    /// Wand am Ende, auf der anderen Seite nicht. Die Zahl misst von ihrer
    /// Fläche; über sie hinweg passt die Tür nicht.
    #[test]
    fn kette_am_t_stoss() {
        let (mut m, iw, st) = haus();
        let set = m.defaults().interior_wall;
        let quer = m
            .add_wall_run(
                &[vec3(5000.0, 4000.0, 0.0), vec3(10000.0, 4000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        assert!(m
            .joins()
            .iter()
            .any(|j| j.kind == JoinKind::T && j.a_run == quer && j.b_run == iw));
        let d = m.chain(quer).unwrap().thickness() / 2.0;
        let k = |r| m.chain(r);
        let mods = Modifiers::default();
        let mut t = Innentuer::default();
        // Cursor bei x < 5 m: Aufschlag nach −x, die Kette steht bei +x, wo
        // die Wand anschließt
        let bei = |t: &mut Innentuer, x: f64, y: f64| {
            t.suchen(&m, &k, st, vec3(x, y, 0.0), 30.0, 1500.0);
            t.hover.clone().unwrap()
        };
        let v = bei(&mut t, 4980.0, 2000.0);
        assert_eq!(v.stoesse.len(), 1);
        let (ya, yb) = spanne_y(&v);
        assert!(
            (ya - 315.0).abs() < 1e-6 && (yb - (4000.0 - d)).abs() < 1e-6,
            "{ya} {yb}"
        );
        let v = bei(&mut t, 4980.0, 6000.0);
        let (ya, yb) = spanne_y(&v);
        assert!(
            (ya - (4000.0 + d)).abs() < 1e-6 && (yb - 7685.0).abs() < 1e-6,
            "{ya} {yb}"
        );
        // Cursor bei x > 5 m: die Kette steht bei −x, dort schließt nichts an
        let v = bei(&mut t, 5020.0, 2000.0);
        assert!(v.stoesse.is_empty());
        assert_eq!(spanne_y(&v), (315.0, 7685.0));
        // Nahe der Wand im T-Stoß: 0,3 m von ihrer Fläche
        let v = bei(&mut t, 4980.0, 3000.0);
        let y = |u: f64| v.face.at(u, 0.0).y;
        for ch in "0,3".chars() {
            t.taste(Key::Char(ch), mods);
        }
        assert_eq!(t.taste(Key::Enter, mods), Taste::Setzen);
        let w = t.hover.clone().unwrap();
        let oben = y(w.u.0).max(y(w.u.1));
        assert!((oben - (4000.0 - d - 300.0)).abs() < 1e-6, "{oben}");
        assert!(t.pruefen(&m).unwrap().is_ok());
        // 3 m von der Fläche liefe die Tür über den Stoß hinweg
        bei(&mut t, 4980.0, 3000.0);
        t.taste(Key::Char('3'), mods);
        assert_eq!(t.taste(Key::Enter, mods), Taste::PasstNicht);
    }

    /// Review #158 B1: Längs derselben Wandfläche rechnet die Vorschau die
    /// Seite der Kette nur einmal. Auf der anderen Seite der Wand, nach
    /// einer Änderung des Modells und mit anderem Bezug rechnet sie neu.
    #[test]
    fn kette_gemerkt() {
        let (mut m, _, st) = haus();
        let mut t = Innentuer::default();
        let bei = |t: &mut Innentuer, m: &Model, x: f64, y: f64| {
            let k = |r| m.chain(r);
            t.suchen(m, &k, st, vec3(x, y, 0.0), 30.0, 1500.0);
            (spanne_y(&t.hover.clone().unwrap()), t.neu_gerechnet)
        };
        // Kette bei −x
        assert_eq!(bei(&mut t, &m, 5020.0, 2000.0).1, 1);
        // Andere Seite, Kette bei +x; die Maus wandert längs der Wand
        for y in [1000.0, 2000.0, 3500.0, 6000.0] {
            assert_eq!(bei(&mut t, &m, 4980.0, y), ((315.0, 7685.0), 2));
        }
        // Eine Wand im T-Stoß kommt dazu, die Wandfläche bleibt: Die Kette
        // endet an der neuen Wand
        let set = m.defaults().interior_wall;
        let quer = m
            .add_wall_run(
                &[vec3(5000.0, 4000.0, 0.0), vec3(10000.0, 4000.0, 0.0)],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let d = m.chain(quer).unwrap().thickness() / 2.0;
        let k = |r| m.chain(r);
        let f = treffer(&m, &k, st, vec3(4980.0, 2000.0, 0.0), 30.0)
            .unwrap()
            .0;
        assert_eq!(Some(&f), t.merk.as_ref().map(|g| &g.face));
        assert_eq!(bei(&mut t, &m, 4980.0, 2000.0), ((315.0, 4000.0 - d), 3));
        assert_eq!(bei(&mut t, &m, 4980.0, 2500.0).1, 3);
        // Anderer Bezug der Außenmaße
        let mut stil = m.massstil().clone();
        stil.bezug = Aussenbezug::Rohbau;
        assert!(m.set_massstil(stil));
        assert_eq!(bei(&mut t, &m, 4980.0, 2000.0).1, 4);
        // Wechsel auf eine andere Wand und zurück (Review #158 H2)
        let k = |r| m.chain(r);
        t.suchen(&m, &k, st, vec3(7000.0, 4020.0, 0.0), 30.0, 1500.0);
        assert_eq!(t.hover.as_ref().map(|v| v.face.run), Some(quer));
        assert_eq!(t.neu_gerechnet, 5);
        assert_eq!(bei(&mut t, &m, 4980.0, 2000.0), ((315.0, 4000.0 - d), 6));
    }

    /// Review #158 H2: Der Bezug der Außenmaße zählt im Schlüssel der
    /// gemerkten Kette auch bei gleichem Modellstand. „Abbrechen“ im
    /// Einstellungsfenster setzt die Revision zurück, die nächste Änderung
    /// bekommt dieselbe Nummer wie ein verworfener Stand. Im Fenster selbst
    /// sucht die Tür nicht (modal); der Test tut es, um den Fall zu bauen.
    #[test]
    fn kette_bezug_ohne_revision() {
        let (m, st) = haus_mit_putz();
        let d = 102.5;
        let mut s = Scene::with_model(m);
        let mut t = Innentuer::default();
        let bei = |t: &mut Innentuer, s: &Scene| {
            t.suchen_in(s, st, vec3(4980.0, 2000.0, 0.0), 30.0, 1500.0);
            (spanne_y(&t.hover.clone().unwrap()), t.neu_gerechnet)
        };
        assert_eq!(bei(&mut t, &s), ((330.0, 4000.0 - d), 1));
        let rohbau = |m: &mut Model| {
            let mut stil = m.massstil().clone();
            stil.bezug = Aussenbezug::Rohbau;
            m.set_massstil(stil)
        };
        let rev = s.begin_settings();
        assert!(s.edit_attr(rohbau));
        assert_eq!(bei(&mut t, &s), ((315.0, 4000.0 - d + 15.0), 2));
        s.cancel_settings(rev);
        assert_eq!(s.model().revision(), rev);
        // Eine andere Änderung am Stil, Bezug bleibt das ganze Wandpaket:
        // dieselbe Revision wie der verworfene Stand
        assert!(s.edit_model("Stil", |m| {
            let mut stil = m.massstil().clone();
            stil.farbe = [1, 2, 3];
            m.set_massstil(stil)
        }));
        assert_eq!(
            t.merk.as_ref().map(|g| g.revision),
            Some(s.model().revision())
        );
        assert_eq!(bei(&mut t, &s), ((330.0, 4000.0 - d), 3));
    }

    /// Review #158 H1: Der Fang über das Raster der Szene findet auf einem
    /// Gitter über dem Haus dieselbe Wand wie die Suche über alle Züge, für
    /// Innen- und Außenwände, auch an einer schrägen Wand und am T-Stoß.
    #[test]
    fn treffer_ueber_raster() {
        let (mut m, st) = haus_mit_putz();
        let set = m.defaults().interior_wall;
        m.add_wall_run(
            &[vec3(5000.0, 6000.0, 0.0), vec3(8000.0, 4000.0, 0.0)],
            false,
            RefSide::Center,
            st,
            set,
            Category::InteriorWall,
        )
        .unwrap();
        let s = Scene::with_model(m);
        let k = |r| s.koerper(r);
        let (mut innen, mut aussen) = (0, 0);
        for i in 0..=110 {
            for j in 0..=90 {
                let p = vec3(
                    -500.0 + i as f64 * 100.0 + 7.0,
                    -500.0 + j as f64 * 100.0 + 3.0,
                    0.0,
                );
                let a = treffer(s.model(), &k, st, p, 30.0);
                assert_eq!(treffer_in(&s, st, p, 30.0), a, "{p:?}");
                innen += a.is_some() as usize;
                let b = treffer_aussen(s.model(), &k, st, p, 30.0);
                assert_eq!(treffer_aussen_in(&s, st, p, 30.0), b, "{p:?}");
                aussen += b.is_some() as usize;
            }
        }
        assert!(innen > 100 && aussen > 100, "{innen} {aussen}");
    }

    /// Review W49 H2: Am Knick eines Innenwandzugs reicht die Kette außen
    /// bis zur Außenecke, innen bis zur Innenecke.
    #[test]
    fn kette_am_knick() {
        let (mut m, iw, st) = haus();
        // Statt der geraden Innenwand ein L: hoch bis 5 m, dann nach +x
        assert!(m.remove_run(iw));
        let set = m.defaults().interior_wall;
        let l = m
            .add_wall_run(
                &[
                    vec3(5000.0, 0.0, 0.0),
                    vec3(5000.0, 5000.0, 0.0),
                    vec3(10000.0, 5000.0, 0.0),
                ],
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        let d = m.chain(l).unwrap().thickness() / 2.0;
        let k = |r| m.chain(r);
        let mut t = Innentuer::default();
        // Cursor bei x > 5 m (im Winkel des L): Kette bei −x, außen am Knick
        t.suchen(&m, &k, st, vec3(5020.0, 2000.0, 0.0), 30.0, 1500.0);
        let v = t.hover.clone().unwrap();
        assert_eq!(v.face.run, l);
        let (ya, yb) = spanne_y(&v);
        assert!(
            (ya - 315.0).abs() < 1e-6 && (yb - (5000.0 + d)).abs() < 1e-6,
            "{ya} {yb}"
        );
        // Cursor bei x < 5 m: Kette bei +x, innen am Knick
        t.suchen(&m, &k, st, vec3(4980.0, 2000.0, 0.0), 30.0, 1500.0);
        let (ya, yb) = spanne_y(&t.hover.clone().unwrap());
        assert!(
            (ya - 315.0).abs() < 1e-6 && (yb - (5000.0 - d)).abs() < 1e-6,
            "{ya} {yb}"
        );
    }

    /// Wie [`haus`], die Außenwände innen, die Innenwände beidseitig 15 mm
    /// verputzt: fertig innen bei y = 0,33 und 7,67 m, Mauerwerk bei 0,315
    /// und 7,685 m.
    fn haus_mit_putz() -> (Model, StoreyId) {
        let mut m = Model::with_seed(411);
        let putz = m
            .materials()
            .iter()
            .find(|(_, x)| x.name == "Putz")
            .unwrap()
            .0;
        let p = MaterialLayer::new(putz, 15.0, LayerFunction::Finish);
        for (cat, alt) in [
            (TypeCategory::ExteriorWall, m.defaults().exterior_wall),
            (TypeCategory::InteriorWall, m.defaults().interior_wall),
        ] {
            let mut t = m.layer_set(alt).unwrap().clone();
            t.guid = m.new_guid();
            t.code = format!("{}-P", t.code);
            if cat == TypeCategory::InteriorWall {
                t.layers.insert(0, p);
            }
            t.layers.push(p);
            let id = m.add_layer_set(t).unwrap();
            assert!(m.set_default_type(cat, id));
        }
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
        for pts in [
            [vec3(5000.0, 0.0, 0.0), vec3(5000.0, 8000.0, 0.0)],
            [vec3(5000.0, 4000.0, 0.0), vec3(10000.0, 4000.0, 0.0)],
        ] {
            m.add_wall_run(
                &pts,
                false,
                RefSide::Center,
                st,
                set,
                Category::InteriorWall,
            )
            .unwrap();
        }
        (m, st)
    }

    /// Bedienbarkeit 41.2: Die Kette misst nach dem Bezug aus dem Reiter
    /// „Bemaßung“. Beim ganzen Wandpaket reicht sie bis zu den fertigen
    /// Flächen, bei tragender Schicht bis zum Mauerwerk der Außenwände und
    /// der Wand im T-Stoß, also um deren Putz weiter bzw. kürzer.
    #[test]
    fn kette_nach_bezug() {
        let (mut m, st) = haus_mit_putz();
        let quer = m.runs().ids().last().unwrap();
        let d = m.chain(quer).unwrap().thickness() / 2.0;
        assert_eq!(d, 102.5);
        let spannen = |m: &Model| {
            let k = |r| m.chain(r);
            let mut t = Innentuer::default();
            // Kette bei +x, dort die Wand im T-Stoß; dann bei −x
            [4980.0, 5020.0].map(|x| {
                t.suchen(m, &k, st, vec3(x, 2000.0, 0.0), 30.0, 1500.0);
                spanne_y(&t.hover.clone().unwrap())
            })
        };
        let gleich = |a: [(f64, f64); 2], b: [(f64, f64); 2]| {
            a.iter()
                .zip(&b)
                .all(|(x, y)| (x.0 - y.0).abs() < 1e-6 && (x.1 - y.1).abs() < 1e-6)
        };
        let s = spannen(&m);
        assert!(gleich(s, [(330.0, 4000.0 - d), (330.0, 7670.0)]), "{s:?}");
        let mut stil = m.massstil().clone();
        stil.bezug = Aussenbezug::Rohbau;
        assert!(m.set_massstil(stil));
        let s = spannen(&m);
        assert!(
            gleich(s, [(315.0, 4000.0 - d + 15.0), (315.0, 7685.0)]),
            "{s:?}"
        );
    }
}

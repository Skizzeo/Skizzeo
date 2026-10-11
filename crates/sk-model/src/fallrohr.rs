//! Regenfallrohr am Flachdach (Jörn 11.10., `entwaesserung/konzept-
//! regenfallrohr.md`): je Ablauf außen ein Wasserfangkasten unter dem
//! Auslauf des Attikaablaufs, das senkrechte Fallrohr mit Rohrschellen bis
//! ins Regenstandrohr, dazu die Notentwässerung nach DIN 1986-100 (eigenes
//! Notfallrohr, frei über Gelände endend, oder Wasserspeier) und der
//! Kontrollablauf der Dampfsperrebene in denselben Kasten.
//!
//! Bezug ist der Ablaufpunkt auf der Innenfläche der Aufkantung, die nächste
//! Kante und die Außenfläche der Aufkantung (die Fassade darunter ist mit
//! ihr bündig, wie die Aufkantung gebaut wird). Alle Höhen absolut wie die
//! des Flachdachs.

use crate::qto::AutoMenge;
use crate::roof::{FlatRoof, OVERFLOW_GAP, OVERFLOW_W};
use crate::solid::{at_z, edge_kind, right_of, Solid};
use sk_math::{vec3, Vec3};

/// Notentwässerung am Notüberlauf (DIN 1986-100: frei auf das Grundstück,
/// nicht an die Grundleitung).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// Eigenes Notfallrohr mit Wasserfangkasten, endet mit Auslaufbogen
    /// über Gelände (Vorgabe).
    Pipe,
    /// Wasserspeier vor der Fassade.
    Spout,
}

/// Eigenschaften eines Regenfallrohrs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownpipeSpec {
    pub overflow: Overflow,
    /// Kontrollablauf DN 50 auf der Dampfsperre in den Wasserfangkasten
    /// (Jörns Notanschluss für Wasser in der Dämmebene).
    pub control: bool,
    /// Größter Abstand der Rohrschellen (mm).
    pub spacing: f64,
}

impl Default for DownpipeSpec {
    fn default() -> DownpipeSpec {
        DownpipeSpec {
            overflow: Overflow::Pipe,
            control: true,
            spacing: SPACING,
        }
    }
}

/// Schellenabstand: Vorgabe und Bereich (mm). Hersteller (Rheinzink,
/// VMZINC): bis Nenngröße 100 höchstens 3,0 m; 2,0 m wegen der
/// Abstandsmontage durch das WDVS.
pub const SPACING: f64 = 2000.0;
pub const SPACING_MIN: f64 = 1000.0;
pub const SPACING_MAX: f64 = 3000.0;

/// Fallrohr DN 100: Durchmesser und lichter Abstand zur Fassade (≥ 20 mm).
pub const PIPE_D: f64 = 100.0;
pub const WALL_GAP: f64 = 30.0;
/// Wasserfangkasten: Breite längs der Fassade, Tiefe, Höhe; Oberkante über
/// der Auslaufachse des Attikaablaufs (mm).
pub const BOX_W: f64 = 250.0;
pub const BOX_D: f64 = 160.0;
pub const BOX_H: f64 = 300.0;
const BOX_ABOVE: f64 = 100.0;
/// Auslaufachse des Attikaablaufs unter OK Dachhaut am Ablauf (mm).
pub const OUTLET_DROP: f64 = 60.0;
/// Kontrollablauf: Achse über OK Rohdecke, Kasten reicht so weit darunter.
pub const CONTROL_UP: f64 = 40.0;
const CONTROL_BELOW: f64 = 100.0;
/// Erste Schelle unter dem Kasten, letzte über dem Standrohr bzw. dem
/// Auslaufbogen (mm).
const FIRST_CLAMP: f64 = 150.0;
const LAST_CLAMP: f64 = 300.0;
/// Regenstandrohr Guss DN 100, 1,0 m lang, davon so viel über Gelände.
pub const STAND_ABOVE: f64 = 700.0;
const STAND_D: f64 = 110.0;
/// Notüberlauf: Unterkante über OK Dachhaut am Ablauf (Anstauhöhe), Höhe
/// der Öffnung (mm).
pub const SILL: f64 = 35.0;
pub const OVERFLOW_H: f64 = 100.0;
/// Wasserfangkasten am Notüberlauf: Breite (mm).
const OV_BOX_W: f64 = 400.0;
/// Notfallrohr endet so hoch über Gelände mit dem Auslaufbogen (mm).
pub const OUTLET_UP: f64 = 200.0;
pub const OUTLET_REACH: f64 = 200.0;
/// Wasserspeier: Auskragung vor der Fassade und Höhe (mm).
pub const SPOUT_OUT: f64 = 300.0;
const SPOUT_H: f64 = 80.0;
/// Kleinster Abstand zweier Schellen an einem kurzen Rohr (mm).
const CLAMP_GAP_MIN: f64 = 500.0;
/// Rohrschelle: Höhe und Wandstärke des Rings (mm).
const CLAMP_H: f64 = 30.0;
const CLAMP_T: f64 = 6.0;
/// Ecken eines runden Rohrs im Körper.
const ROUND: usize = 16;

/// Notentwässerung eines Ablaufs, absolut.
#[derive(Clone, Debug, PartialEq)]
pub struct OverflowRun {
    /// Mitte des Notüberlaufs auf der Außenfläche (z = 0).
    pub face: Vec3,
    /// Unterkante des Notüberlaufs.
    pub sill: f64,
    /// Notfallrohr (nur [`Overflow::Pipe`]): Kasten oben und unten, Ende
    /// des Rohrs (OK Auslaufbogen) und Schellen.
    pub box_top: f64,
    pub box_bot: f64,
    pub end: f64,
    pub clamps: Vec<f64>,
    /// Fläche unter dem Notfallrohr: Gelände oder ein tieferes Dach.
    pub foot: Foot,
    /// Abstand der Achse des Notfallrohrs von der Außenfläche (mm).
    pub reach: f64,
}

/// Fläche, auf der ein Rohr endet (Prüfung Geometrie #68): das Gelände
/// oder, bei zurückgesetzter Wand, das tiefere Dach bzw. die Dachterrasse
/// davor. Auf dem tieferen Dach endet das Rohr mit Auslaufbogen über dem
/// Belag statt im Standrohr (das Wasser nimmt dort dessen Entwässerung
/// auf, Flachdachrichtlinie: Ableitung auf tiefere Dachflächen).
///
/// Springt die Fassade darunter nur wenig vor (kleiner Rücksprung ohne
/// Dachterrasse, Nachprüfung Geometrie #68), läuft das Rohr mit größerem
/// Wandabstand an ihr vorbei bis ins Standrohr: `shift` ist der Zuschlag
/// zum Wandabstand, gehalten von Schellen mit längerer Stockschraube.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foot {
    /// OK Gelände bzw. OK Belag des tieferen Dachs.
    pub z: f64,
    pub lower: bool,
    /// Zuschlag zum Wandabstand (mm), 0 ohne Rücksprung.
    pub shift: f64,
    /// Richtung des Auslaufbogens auf dem tieferen Dach (waagerecht,
    /// Länge 1): nach außen oder, bei schmalem Belag, längs der Fassade.
    pub bend: Vec3,
    /// Dachterrasse, auf die das Rohr ausläuft: Wandzug darunter und Nummer
    /// des Terrassenumrisses (Kosten B31).
    pub onto: Option<(crate::element::RunId, usize)>,
}

/// Kleinster lichter Abstand eines Rohrs zu einer Fassade, an der es
/// vorbeiläuft (mm); darunter rückt es um den Rücksprung nach außen.
pub const MIN_GAP: f64 = 20.0;
/// Größter Zuschlag zum Wandabstand (mm). Springt die Fassade darunter
/// weiter vor (Decke ohne Terrasse), endet das Rohr über der Decke.
pub const MAX_SHIFT: f64 = 300.0;

/// Abstand der Rohrachse von der Außenfläche ohne Rücksprung (mm).
pub const REACH: f64 = WALL_GAP + PIPE_D / 2.0;

/// Ablauf einer Dachterrasse, auf die ein Fallrohr ausläuft (Kosten B31):
/// Nummer der Dachterrasse (für die Formel im LV) und sein Fallrohr.
#[derive(Clone, Debug, PartialEq)]
pub struct TerraceDrain {
    pub number: String,
    pub pipe: Downpipe,
}

/// Regenfallrohr eines Ablaufs, fertig ausgelegt (mm, absolut).
#[derive(Clone, Debug, PartialEq)]
pub struct Downpipe {
    pub spec: DownpipeSpec,
    /// Ablauf auf der Kante der Innenfläche und derselbe Punkt auf der
    /// Außenfläche (z = 0).
    pub drain: Vec3,
    pub face: Vec3,
    /// Richtung der Kante und nach außen (waagerecht, Länge 1).
    pub along: Vec3,
    pub out: Vec3,
    /// Achse des Auslaufs des Attikaablaufs.
    pub outlet: f64,
    pub box_top: f64,
    pub box_bot: f64,
    /// Abstand der Rohrachse von der Außenfläche (mm): [`REACH`], vor
    /// einem kleinen Rücksprung darunter mehr.
    pub reach: f64,
    /// Richtung des Auslaufbogens über einem tieferen Dach.
    pub bend: Vec3,
    /// Dachterrasse, auf die das Fallrohr ausläuft (siehe [`Foot::onto`]).
    pub onto: Option<(crate::element::RunId, usize)>,
    /// Terrassenablauf (Kosten B31, DIN 1986-100): Läuft das Rohr als
    /// erstes auf eine Dachterrasse aus, hat diese einen eigenen Ablauf in
    /// ihrer Attika mit Fallrohr und Notentwässerung, ausgelegt wie dieses.
    pub terrace: Option<Box<TerraceDrain>>,
    /// Unteres Ende des Fallrohrs (OK Standrohr bzw. OK Auslaufbogen über
    /// einem tieferen Dach) und die Fläche darunter.
    pub stand_top: f64,
    pub ground: f64,
    pub lower: bool,
    /// Höhen der Rohrschellen am Fallrohr.
    pub clamps: Vec<f64>,
    /// Achse des Kontrollablaufs, wenn es ihn gibt.
    pub control: Option<f64>,
    pub overflow: OverflowRun,
    /// Baustoff (Darstellungsschlüssel), wie das Attikablech.
    pub mat: u16,
}

/// Höhen der Schellen zwischen `top` (Unterkante oben) und `bot` (Ende
/// unten): die erste [`FIRST_CLAMP`] unter `top`, die letzte [`LAST_CLAMP`]
/// über `bot`, dazwischen gleichmäßig mit höchstens `spacing`.
pub fn clamp_heights(top: f64, bot: f64, spacing: f64) -> Vec<f64> {
    if top - bot < PIPE_D {
        return Vec::new();
    }
    let (z1, z0) = (top - FIRST_CLAMP, bot + LAST_CLAMP);
    // zu kurz für zwei Schellen mit Abstand (Review #68: nicht 1 mm
    // übereinander): eine in der Mitte
    if z1 - z0 < CLAMP_GAP_MIN {
        return vec![(top + bot) / 2.0];
    }
    let n = ((z1 - z0) / spacing.max(SPACING_MIN) - 1e-9)
        .ceil()
        .max(1.0) as usize;
    (0..=n)
        .map(|i| z1 - (z1 - z0) * i as f64 / n as f64)
        .collect()
}

impl Downpipe {
    /// Legt das Regenfallrohr am Ablauf `drain` des Flachdachs `r` aus;
    /// `foot` liefert die Fläche unter einer Rohrachse (Grundrisspunkt) vor
    /// der Fassade mit der Richtung nach außen (Gelände oder tieferes Dach).
    /// `None` ohne Umriss.
    pub fn layout(
        r: &FlatRoof,
        drain: Vec3,
        foot: impl Fn(Vec3, Vec3, Vec3) -> Foot,
        spec: DownpipeSpec,
    ) -> Option<Downpipe> {
        let n = r.outline.len();
        if n < 3 {
            return None;
        }
        // Kante der Aufkantung, an der der Ablauf liegt (wie im Gefälleplan);
        // Kanten ohne Länge zählen nicht, damit jeder Ablauf sein Rohr
        // bekommt und die Nummern der Abläufe nicht verrutschen
        let dist = |i: usize| {
            let (a, b) = (r.outline[i], r.outline[(i + 1) % n]);
            sk_math::dist_to_segment((drain.x, drain.y), (a.x, a.y), (b.x, b.y))
        };
        let lang = |&i: &usize| (r.outline[(i + 1) % n] - r.outline[i]).length() >= 1e-6;
        let i = (0..n)
            .filter(lang)
            .min_by(|&x, &y| dist(x).total_cmp(&dist(y)))?;
        let (a, b) = (r.outline[i], r.outline[(i + 1) % n]);
        let len = (b - a).length();
        let along = (b - a) * (1.0 / len);
        let t = (drain - a).dot(along).clamp(0.0, len);
        let dr = vec3(a.x + along.x * t, a.y + along.y * t, 0.0);
        let out = right_of(along);
        let face = dr + out * r.width;
        let top = r.band().1 + r.wedge_at(dr);
        let outlet = top - OUTLET_DROP;
        let control = spec.control.then_some(r.base + CONTROL_UP);
        // Kästen und Notüberlauf bleiben unter OK Aufkantung (unter dem
        // Blech; Prüfung Geometrie #71 an der niedrigen Terrassenattika)
        let box_top = (outlet + BOX_ABOVE).min(r.crown);
        let box_bot = control.map_or(box_top - BOX_H, |c| {
            (box_top - BOX_H).min(c - CONTROL_BELOW)
        });
        // Notüberlauf neben dem Ablauf, zur längeren Kantenhälfte hin (wie
        // das Zeichen im Gefälleplan); längs der Fassade biegen die
        // Auslaufbögen voneinander weg
        let dir = if len - t >= t { along } else { -along };
        let rohr = |face: Vec3| face + out * REACH;
        let unten = foot(rohr(face), out, -dir);
        let ground = unten.z;
        let stand_top = if unten.lower {
            ground + OUTLET_UP
        } else {
            ground + STAND_ABOVE
        }
        .min(box_bot);
        let clamps = clamp_heights(box_bot, stand_top, spec.spacing);
        let ov = dr + dir * (OVERFLOW_GAP + OVERFLOW_W / 2.0) + out * r.width;
        // Notüberlauf über der Dachhaut, bei niedriger Aufkantung so tief,
        // dass seine Öffnung unter OK Aufkantung bleibt, nie unter der
        // Dachhaut
        let sill = (top + SILL).min(r.crown - OVERFLOW_H).max(top);
        let ov_box_top = (sill + OVERFLOW_H + 20.0).min(r.crown);
        let ov_box_bot = ov_box_top - BOX_H;
        let ov_foot = foot(rohr(ov), out, dir);
        let end = (ov_foot.z + OUTLET_UP).min(ov_box_bot);
        let ov_clamps = match spec.overflow {
            Overflow::Pipe => clamp_heights(ov_box_bot, end, spec.spacing),
            Overflow::Spout => Vec::new(),
        };
        Some(Downpipe {
            spec,
            drain: dr,
            face,
            along,
            out,
            outlet,
            box_top,
            box_bot,
            reach: REACH + unten.shift,
            bend: unten.bend,
            onto: unten.onto,
            terrace: None,
            stand_top,
            ground,
            lower: unten.lower,
            clamps,
            control,
            overflow: OverflowRun {
                face: ov,
                sill,
                box_top: ov_box_top,
                box_bot: ov_box_bot,
                end,
                clamps: ov_clamps,
                foot: ov_foot,
                reach: REACH + ov_foot.shift,
            },
            mat: r.coping_mat,
        })
    }

    /// Länge des Fallrohrs in der Mittellinie (mm, ATV DIN 18339):
    /// Kastenstutzen bis Standrohr.
    pub fn pipe_length(&self) -> f64 {
        (self.box_bot - self.stand_top).max(0.0)
    }

    /// Länge des Notfallrohrs (mm), 0 beim Speier.
    pub fn overflow_length(&self) -> f64 {
        match self.spec.overflow {
            Overflow::Pipe => (self.overflow.box_bot - self.overflow.end).max(0.0),
            Overflow::Spout => 0.0,
        }
    }

    /// Rohrschellen an Fallrohr und Notfallrohr (Stück).
    pub fn clamp_count(&self) -> usize {
        self.clamps.len() + self.overflow.clamps.len()
    }

    /// Achse des Fallrohrs (z = 0).
    pub fn axis(&self) -> Vec3 {
        self.face + self.out * self.reach
    }

    fn ov_axis(&self) -> Vec3 {
        self.overflow.face + self.out * self.overflow.reach
    }

    /// Körper für 3D und Ansichten: Kasten, Fallrohr, Schellen, Standrohr,
    /// Notentwässerung.
    pub fn solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.mat,
            edge_kind: edge_kind::VIEW,
            ..Solid::default()
        };
        let r = PIPE_D / 2.0;
        // Wasserfangkasten an der Fassade, Rohrachse in seiner Mitte; mit
        // größerem Wandabstand reicht er bis an die Fassade
        let tief = |reach: f64| (reach + BOX_D / 2.0).max(BOX_D);
        let c = self.face + self.out * (tief(self.reach) / 2.0);
        self.cuboid(
            &mut s,
            c,
            BOX_W,
            tief(self.reach),
            self.box_bot,
            self.box_top,
        );
        // Fallrohr, Schellen und Standrohr
        let ax = self.axis();
        if self.box_bot - self.stand_top > 1.0 {
            cylinder(&mut s, ax, r, self.stand_top, self.box_bot);
        }
        for &z in &self.clamps {
            self.clamp(&mut s, ax, self.reach, z);
        }
        if self.lower {
            self.outlet_bend(&mut s, ax, self.bend, self.stand_top, self.ground);
        } else if self.stand_top - self.ground > 1.0 {
            cylinder(&mut s, ax, STAND_D / 2.0, self.ground, self.stand_top);
        }
        // Notentwässerung
        let o = &self.overflow;
        match self.spec.overflow {
            Overflow::Pipe => {
                let c = o.face + self.out * (tief(o.reach) / 2.0);
                self.cuboid(&mut s, c, OV_BOX_W, tief(o.reach), o.box_bot, o.box_top);
                let ax = self.ov_axis();
                if o.box_bot - o.end > 1.0 {
                    cylinder(&mut s, ax, r, o.end, o.box_bot);
                }
                for &z in &o.clamps {
                    self.clamp(&mut s, ax, o.reach, z);
                }
                self.outlet_bend(&mut s, ax, o.foot.bend, o.end, o.foot.z);
            }
            Overflow::Spout => {
                let c = o.face + self.out * (SPOUT_OUT / 2.0);
                self.cuboid(&mut s, c, OVERFLOW_W, SPOUT_OUT, o.sill - SPOUT_H, o.sill);
            }
        }
        if let Some(t) = &self.terrace {
            s.append(&t.pipe.solid());
        }
        s
    }

    /// Auslaufbogen am Rohrende `end` über der Fläche `ground`: kurzes
    /// Stück in Richtung `bend`, von der Fassade weg oder längs.
    fn outlet_bend(&self, s: &mut Solid, ax: Vec3, bend: Vec3, end: f64, ground: f64) {
        let c = ax + bend * (OUTLET_REACH / 2.0);
        let lo = (end - PIPE_D).max(ground);
        let lang = OUTLET_REACH + PIPE_D;
        let (w, d) = if bend.dot(self.out).abs() > 0.5 {
            (PIPE_D, lang)
        } else {
            (lang, PIPE_D)
        };
        if end - lo > 1.0 {
            self.cuboid(s, c, w, d, lo, end);
        }
    }

    /// Quader mit Mitte `c` (z = 0), `w` längs der Fassade, `d` nach
    /// außen, zwischen `z0` und `z1`.
    fn cuboid(&self, s: &mut Solid, c: Vec3, w: f64, d: f64, z0: f64, z1: f64) {
        let (u, v) = (self.along * (w / 2.0), self.out * (d / 2.0));
        let pts = sk_math::polygon::to_ccw(&[c - u - v, c + u - v, c + u + v, c - u + v]);
        prism(s, &pts, z0, z1);
    }

    /// Rohrschelle: Ring um das Rohr mit der Achse `ax` im Abstand `reach`
    /// vor der Fassade und Stift zur Fassade.
    fn clamp(&self, s: &mut Solid, ax: Vec3, reach: f64, z: f64) {
        let r = PIPE_D / 2.0;
        cylinder(s, ax, r + CLAMP_T, z - CLAMP_H / 2.0, z + CLAMP_H / 2.0);
        let stift = reach - r;
        let c = ax - self.out * (r + stift / 2.0);
        self.cuboid(s, c, 12.0, stift, z - 6.0, z + 6.0);
    }

    /// Grundriss: was unter der Schnitthöhe `cut` liegt, als Draufsicht
    /// (Kanten oben auf den Körpern).
    pub fn plan_below(&self, cut: f64) -> Solid {
        let s = self.solid();
        let mut out = Solid {
            mat: s.mat,
            ..Solid::default()
        };
        out.edges = s
            .edges
            .into_iter()
            .filter(|e| e.a.z <= cut && e.b.z <= cut && (e.a.z - e.b.z).abs() < 1e-6)
            .collect();
        out
    }
}

/// Senkrechtes Prisma über dem Umriss `pts` (gegen den Uhrzeigersinn) mit
/// Deckeln und Kanten oben, unten und an den Ecken.
fn prism(s: &mut Solid, pts: &[Vec3], z0: f64, z1: f64) {
    if z1 - z0 < 1e-6 || pts.len() < 3 {
        return;
    }
    s.sides(pts, z0, z1, true);
    s.cap(pts, z1, true);
    s.cap(pts, z0, false);
    let n = pts.len();
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        s.edge(at_z(a, z0), at_z(b, z0));
        s.edge(at_z(a, z1), at_z(b, z1));
        if n <= 4 {
            s.edge(at_z(a, z0), at_z(a, z1));
        }
    }
}

/// Senkrechtes rundes Rohr um die Achse `ax` (z = 0).
fn cylinder(s: &mut Solid, ax: Vec3, r: f64, z0: f64, z1: f64) {
    let pts: Vec<Vec3> = (0..ROUND)
        .map(|k| {
            let t = k as f64 / ROUND as f64 * std::f64::consts::TAU;
            vec3(ax.x + r * t.cos(), ax.y + r * t.sin(), 0.0)
        })
        .collect();
    prism(s, &pts, z0, z1);
}

// ---- Automatikmengen (`downpipe.*`, Konzept §4) ----

pub const BOX: &str = "downpipe.box";
pub const PIPE: &str = "downpipe.pipe";
pub const CLAMPS: &str = "downpipe.clamps";
pub const STAND: &str = "downpipe.stand";
pub const OV_BOX: &str = "downpipe.ovbox";
pub const OV_PIPE: &str = "downpipe.ovpipe";
pub const OUTLET: &str = "downpipe.outlet";
pub const SPOUT: &str = "downpipe.spout";
pub const CONTROL: &str = "downpipe.control";

/// Schlüssel des Regenfallrohrs (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 9] = [
    (BOX, "st", "Wasserfangkasten am Attikaablauf"),
    (
        PIPE,
        "m",
        "Regenfallrohr in der Mittellinie, Wasserfangkasten bis Standrohr",
    ),
    (CLAMPS, "st", "Rohrschellen an Fallrohr und Notfallrohr"),
    (STAND, "st", "Regenstandrohr mit Reinigungsöffnung"),
    (
        OV_BOX,
        "st",
        "Wasserfangkasten am Notüberlauf (Notentwässerung mit Notfallrohr)",
    ),
    (
        OV_PIPE,
        "m",
        "Notfallrohr in der Mittellinie, frei über Gelände endend",
    ),
    (
        OUTLET,
        "st",
        "Auslaufbogen am Notfallrohr und am Fallrohr über einem tieferen Dach",
    ),
    (SPOUT, "st", "Wasserspeier am Notüberlauf"),
    (
        CONTROL,
        "st",
        "Kontrollablauf der Dampfsperrebene in den Wasserfangkasten",
    ),
];

/// Kostengruppe der Dachentwässerung (DIN 276: 363 Dachbeläge).
pub const KG: u16 = 363;

/// Automatikmengen eines Regenfallrohrs; `vorlage` trägt Bauteil, Nummer,
/// Geschoss und Gebäude.
pub fn mengen(vorlage: &AutoMenge, d: &Downpipe) -> Vec<AutoMenge> {
    let m = |mm: f64| format!("{:.2} m", mm / 1000.0).replace('.', ",");
    let menge = |key, unit, value, formula| AutoMenge {
        key,
        unit,
        value,
        kg: Some(KG),
        formula,
        ..vorlage.clone()
    };
    let mut out = vec![menge(BOX, "st", 1.0, "1 Wasserfangkasten".into())];
    if d.pipe_length() > 0.0 {
        out.push(menge(
            PIPE,
            "m",
            d.pipe_length(),
            format!(
                "Kasten bis {} {}",
                if d.lower { "Auslaufbogen" } else { "Standrohr" },
                m(d.pipe_length())
            ),
        ));
    }
    let n = d.clamp_count();
    if n > 0 {
        out.push(menge(CLAMPS, "st", n as f64, {
            let mut f = format!("{n} Schellen, Abstand höchstens {}", m(d.spec.spacing));
            // vor einem kleinen Rücksprung mit längerer Stockschraube
            let reach = d.reach.max(d.overflow.reach);
            if reach > REACH + 0.5 {
                f += &format!(", Wandabstand bis {:.0} mm", reach - PIPE_D / 2.0);
            }
            f
        }));
    }
    if !d.lower {
        out.push(menge(STAND, "st", 1.0, "1 Standrohr".into()));
    }
    // Auslaufbögen: am Notfallrohr und am Fallrohr über einem tieferen Dach
    let boegen = usize::from(d.spec.overflow == Overflow::Pipe) + usize::from(d.lower);
    match d.spec.overflow {
        Overflow::Pipe => {
            out.push(menge(
                OV_BOX,
                "st",
                1.0,
                "1 Wasserfangkasten am Notüberlauf".into(),
            ));
            if d.overflow_length() > 0.0 {
                out.push(menge(
                    OV_PIPE,
                    "m",
                    d.overflow_length(),
                    format!("Kasten bis Auslaufbogen {}", m(d.overflow_length())),
                ));
            }
        }
        Overflow::Spout => out.push(menge(SPOUT, "st", 1.0, "1 Wasserspeier".into())),
    }
    if boegen > 0 {
        out.push(menge(
            OUTLET,
            "st",
            boegen as f64,
            format!("{boegen} Auslaufbogen"),
        ));
    }
    if d.control.is_some() {
        out.push(menge(CONTROL, "st", 1.0, "1 Kontrollablauf".into()));
    }
    // Terrassenablauf mit Notüberlauf in der Terrassenattika und seinem
    // Fallrohr: zu den Zeilen dieses Rohrs (ein Schlüssel je Zeile)
    if let Some(t) = &d.terrace {
        let mut dazu = vec![
            menge(crate::roof::DRAINS, "st", 1.0, "1 Attikaablauf".into()),
            menge(crate::roof::OVERFLOWS, "st", 1.0, "1 Notüberlauf".into()),
        ];
        dazu.extend(mengen(vorlage, &t.pipe));
        for z in dazu {
            let f = format!("Terrassenablauf an {}: {}", t.number, z.formula);
            match out.iter_mut().find(|o| o.key == z.key) {
                Some(o) => {
                    o.value += z.value;
                    o.formula = format!("{} + {f}", o.formula);
                }
                None => out.push(AutoMenge { formula: f, ..z }),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schellen_gleichmaessig_hoechstens_abstand() {
        // 6,0 m Rohr: 5,55 m zwischen erster und letzter Schelle → 3 Felder
        let z = clamp_heights(6000.0, 0.0, 2000.0);
        assert_eq!(z.len(), 4);
        assert!((z[0] - 5850.0).abs() < 1e-9);
        assert!((z[3] - 300.0).abs() < 1e-9);
        assert!(z.windows(2).all(|w| w[0] - w[1] <= 2000.0 + 1e-9));
        // genau ein Feld
        assert_eq!(clamp_heights(2450.0, 0.0, 2000.0).len(), 2);
        // zu kurz für zwei Schellen: eine in der Mitte (auch bei 451 mm,
        // nicht zwei 1 mm übereinander)
        assert_eq!(clamp_heights(400.0, 0.0, 2000.0), vec![200.0]);
        assert_eq!(clamp_heights(451.0, 0.0, 2000.0), vec![225.5]);
        // kein Rohr
        assert!(clamp_heights(50.0, 0.0, 2000.0).is_empty());
    }
}

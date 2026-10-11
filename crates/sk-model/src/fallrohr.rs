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
const OUTLET_REACH: f64 = 200.0;
/// Wasserspeier: Auskragung vor der Fassade und Höhe (mm).
pub const SPOUT_OUT: f64 = 300.0;
const SPOUT_H: f64 = 80.0;
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
    /// OK Standrohr (= unteres Ende des Fallrohrs) und OK Gelände.
    pub stand_top: f64,
    pub ground: f64,
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
    if z1 <= z0 {
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
    /// `ground` ist OK Gelände dort. `None` ohne Umriss.
    pub fn layout(r: &FlatRoof, drain: Vec3, ground: f64, spec: DownpipeSpec) -> Option<Downpipe> {
        let n = r.outline.len();
        if n < 3 {
            return None;
        }
        // Kante der Aufkantung, an der der Ablauf liegt (wie im Gefälleplan)
        let dist = |i: usize| {
            let (a, b) = (r.outline[i], r.outline[(i + 1) % n]);
            sk_math::dist_to_segment((drain.x, drain.y), (a.x, a.y), (b.x, b.y))
        };
        let i = (0..n).min_by(|&x, &y| dist(x).total_cmp(&dist(y)))?;
        let (a, b) = (r.outline[i], r.outline[(i + 1) % n]);
        let len = (b - a).length();
        if len < 1e-6 {
            return None;
        }
        let along = (b - a) * (1.0 / len);
        let t = (drain - a).dot(along).clamp(0.0, len);
        let dr = vec3(a.x + along.x * t, a.y + along.y * t, 0.0);
        let out = right_of(along);
        let face = dr + out * r.width;
        let top = r.band().1 + r.wedge_at(dr);
        let outlet = top - OUTLET_DROP;
        let control = spec.control.then_some(r.base + CONTROL_UP);
        let box_top = outlet + BOX_ABOVE;
        let box_bot = control.map_or(box_top - BOX_H, |c| {
            (box_top - BOX_H).min(c - CONTROL_BELOW)
        });
        let stand_top = (ground + STAND_ABOVE).min(box_bot);
        let clamps = clamp_heights(box_bot, stand_top, spec.spacing);
        // Notüberlauf neben dem Ablauf, zur längeren Kantenhälfte hin (wie
        // das Zeichen im Gefälleplan)
        let dir = if len - t >= t { along } else { -along };
        let ov = dr + dir * (OVERFLOW_GAP + OVERFLOW_W / 2.0) + out * r.width;
        let sill = top + SILL;
        let ov_box_top = sill + OVERFLOW_H + 20.0;
        let ov_box_bot = ov_box_top - BOX_H;
        let end = (ground + OUTLET_UP).min(ov_box_bot);
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
            stand_top,
            ground,
            clamps,
            control,
            overflow: OverflowRun {
                face: ov,
                sill,
                box_top: ov_box_top,
                box_bot: ov_box_bot,
                end,
                clamps: ov_clamps,
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
        self.face + self.out * (WALL_GAP + PIPE_D / 2.0)
    }

    fn ov_axis(&self) -> Vec3 {
        self.overflow.face + self.out * (WALL_GAP + PIPE_D / 2.0)
    }

    /// Körper für 3D und Ansichten: Kasten, Fallrohr, Schellen, Standrohr,
    /// Notentwässerung.
    pub fn solid(&self) -> Solid {
        let mut s = Solid {
            mat: self.mat,
            edge_kind: edge_kind::VIEW,
            ..Solid::default()
        };
        let depth = WALL_GAP + PIPE_D / 2.0;
        let r = PIPE_D / 2.0;
        // Wasserfangkasten an der Fassade, Rohrachse in seiner Mitte
        let c = self.face + self.out * (BOX_D / 2.0).max(depth);
        self.cuboid(&mut s, c, BOX_W, BOX_D, self.box_bot, self.box_top);
        // Fallrohr, Schellen und Standrohr
        let ax = self.axis();
        if self.box_bot - self.stand_top > 1.0 {
            cylinder(&mut s, ax, r, self.stand_top, self.box_bot);
        }
        for &z in &self.clamps {
            self.clamp(&mut s, ax, z);
        }
        if self.stand_top - self.ground > 1.0 {
            cylinder(&mut s, ax, STAND_D / 2.0, self.ground, self.stand_top);
        }
        // Notentwässerung
        let o = &self.overflow;
        match self.spec.overflow {
            Overflow::Pipe => {
                let c = o.face + self.out * (BOX_D / 2.0).max(depth);
                self.cuboid(&mut s, c, OV_BOX_W, BOX_D, o.box_bot, o.box_top);
                let ax = self.ov_axis();
                if o.box_bot - o.end > 1.0 {
                    cylinder(&mut s, ax, r, o.end, o.box_bot);
                }
                for &z in &o.clamps {
                    self.clamp(&mut s, ax, z);
                }
                // Auslaufbogen: kurzes Stück von der Fassade weg
                let c = ax + self.out * (OUTLET_REACH / 2.0);
                let lo = (o.end - PIPE_D).max(self.ground);
                if o.end - lo > 1.0 {
                    self.cuboid(&mut s, c, PIPE_D, OUTLET_REACH + PIPE_D, lo, o.end);
                }
            }
            Overflow::Spout => {
                let c = o.face + self.out * (SPOUT_OUT / 2.0);
                self.cuboid(&mut s, c, OVERFLOW_W, SPOUT_OUT, o.sill - SPOUT_H, o.sill);
            }
        }
        s
    }

    /// Quader mit Mitte `c` (z = 0), `w` längs der Fassade, `d` nach
    /// außen, zwischen `z0` und `z1`.
    fn cuboid(&self, s: &mut Solid, c: Vec3, w: f64, d: f64, z0: f64, z1: f64) {
        let (u, v) = (self.along * (w / 2.0), self.out * (d / 2.0));
        let pts = sk_math::polygon::to_ccw(&[c - u - v, c + u - v, c + u + v, c - u + v]);
        prism(s, &pts, z0, z1);
    }

    /// Rohrschelle: Ring um das Rohr und Stift zur Fassade.
    fn clamp(&self, s: &mut Solid, ax: Vec3, z: f64) {
        let r = PIPE_D / 2.0 + CLAMP_T;
        cylinder(s, ax, r, z - CLAMP_H / 2.0, z + CLAMP_H / 2.0);
        let c = self.face + self.out * (WALL_GAP / 2.0);
        self.cuboid(s, c, 12.0, WALL_GAP, z - 6.0, z + 6.0);
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
    (OUTLET, "st", "Auslaufbogen am Notfallrohr"),
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
            format!("Kasten bis Standrohr {}", m(d.pipe_length())),
        ));
    }
    let n = d.clamp_count();
    if n > 0 {
        out.push(menge(
            CLAMPS,
            "st",
            n as f64,
            format!("{n} Schellen, Abstand höchstens {}", m(d.spec.spacing)),
        ));
    }
    out.push(menge(STAND, "st", 1.0, "1 Standrohr".into()));
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
            out.push(menge(OUTLET, "st", 1.0, "1 Auslaufbogen".into()));
        }
        Overflow::Spout => out.push(menge(SPOUT, "st", 1.0, "1 Wasserspeier".into())),
    }
    if d.control.is_some() {
        out.push(menge(CONTROL, "st", 1.0, "1 Kontrollablauf".into()));
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
        // zu kurz für zwei Schellen: eine in der Mitte
        assert_eq!(clamp_heights(400.0, 0.0, 2000.0), vec![200.0]);
        // kein Rohr
        assert!(clamp_heights(50.0, 0.0, 2000.0).is_empty());
    }
}

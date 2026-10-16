//! Lageplan im Modell (Gelände G1c, plan-gelaendemodul.md §5 G1c und §6):
//! Der Lageplan liefert die Grundstücksgrenze und die Unterlage. In der
//! `.szo` stehen nur Pfad, Seite, Einpassung und die übernommene Grenze
//! (`[tline] kind=boundary en=…`, `[underlay]`); die Datei liest die App
//! wieder, wenn sie die Unterlage zeigt.
//!
//! Ohne Höhen bleibt das Gelände eben in Höhe OK Gelände, hat aber schon
//! die Form des Grundstücks. Kommen Höhen in derselben UTM-Zone dazu,
//! passen beide von selbst zueinander. Liegt der Lageplan nur im Maßstab
//! vor (oder in einem anderen System als das Aufmaß), passen ihn die
//! Grenzpunkte mit gleicher Nummer ins Aufmaß ein; das Haus bleibt, wo es
//! auf dem Grundstück steht.

use super::*;
use sk_gelaende::lageplan::lage::{Einpassung, Lage, PT};
use sk_gelaende::lageplan::{self, Bedeutung};

/// Affine Abbildung wie im PDF: x' = a x + c y + e, y' = b x + d y + f.
pub type Affine = [f64; 6];

/// Lageplan als Unterlage, wie er in der `.szo` steht.
#[derive(Clone, Debug, PartialEq)]
pub struct Underlay {
    /// Datei, wie gewählt; sie wird nicht in die `.szo` kopiert.
    pub src: String,
    pub page: u32,
    /// Blatt (pt) → Lagesystem des Geländes (m).
    pub fit: Affine,
    /// Übernommenes Flurstück.
    pub flst: String,
}

/// Was eine Linie der Unterlage ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnderlayKind {
    Boundary,
    Mark,
    Building,
    Other,
}

/// Ein gelesener Lageplan, wie die Leiste ihn anbietet.
#[derive(Clone, Debug, PartialEq)]
pub struct SitePlan {
    /// Lagesystem und Einpassung Blatt (pt) → Landeswert (m); `None`,
    /// solange die Lage fehlt.
    pub fit: Option<(Crs, Affine)>,
    /// Maßstab (500 heißt 1:500), wenn erkannt.
    pub scale: Option<f64>,
    /// Mittlerer Restfehler an den Gitterkreuzen (m), wenn die Lage aus
    /// ihnen kommt.
    pub residual: Option<f64>,
    /// Flurstück zur Nummer im Kopf: Nummer, Ecken auf dem Blatt (pt),
    /// Fläche (m²).
    pub parcel: Option<(String, Vec<[f64; 2]>, Option<f64>)>,
    /// Nummer im Kopf, auch wenn kein Flurstück dazu gefunden wurde.
    pub number: Option<String>,
    /// Grenzpunkte mit Nummer auf dem Blatt (pt).
    pub marks: Vec<(String, [f64; 2])>,
    /// Linien der Unterlage auf dem Blatt (pt): Punkte, geschlossen, Art.
    pub lines: Vec<(Vec<[f64; 2]>, bool, UnderlayKind)>,
    pub hints: Vec<String>,
}

/// `p` abgebildet.
pub fn affine_apply(m: &Affine, p: [f64; 2]) -> [f64; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// Umkehrung; `None`, wenn die Abbildung nicht umkehrbar ist.
fn umgekehrt(m: &Affine) -> Option<Affine> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-18 || !det.is_finite() {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// Erst `m`, dann `n`.
fn nacheinander(m: &Affine, n: &Affine) -> Affine {
    let [x, y] = affine_apply(n, [m[4], m[5]]);
    [
        n[0] * m[0] + n[2] * m[1],
        n[1] * m[0] + n[3] * m[1],
        n[0] * m[2] + n[2] * m[3],
        n[1] * m[2] + n[3] * m[3],
        x,
        y,
    ]
}

impl SitePlan {
    /// PDF lesen und auswerten (erste Seite). Nennt die Karte keine
    /// UTM-Zone, gilt `zone`. `Err` mit einem Satz.
    pub fn read(roh: &[u8], zone: u8) -> Result<SitePlan, String> {
        let plan = lageplan::lesen(roh)?;
        let (fit, scale, residual) = match &plan.lage {
            Lage::Gitter(e) => {
                let z = e.zone.filter(|z| location::UTM_ZONEN.contains(z));
                let crs = Crs::Utm(z.unwrap_or(zone));
                (Some((crs, e.matrix)), Some(e.massstab), Some(e.restfehler))
            }
            Lage::Massstab(s) => {
                let k = s * PT;
                (
                    Some((Crs::Local, [k, 0.0, 0.0, k, 0.0, 0.0])),
                    Some(*s),
                    None,
                )
            }
            Lage::Offen => (None, None, None),
        };
        let parcel = plan
            .flurstueck
            .as_ref()
            .map(|f| (f.nummer.clone(), f.ecken_pt.clone(), f.flaeche));
        let marks = plan
            .grenzpunkte
            .iter()
            .filter_map(|g| Some((g.nr.clone()?, g.lage)))
            .collect();
        let lines = lageplan::unterlage(&plan)
            .pfade
            .into_iter()
            .flat_map(|(p, b)| {
                let art = match b {
                    Bedeutung::Grenze => UnderlayKind::Boundary,
                    Bedeutung::Grenzpunkt => UnderlayKind::Mark,
                    Bedeutung::Gebaeude => UnderlayKind::Building,
                    _ => UnderlayKind::Other,
                };
                p.zuege
                    .into_iter()
                    .filter(|z| z.punkte.len() >= 2)
                    .map(move |z| (z.punkte, z.zu, art))
            })
            .collect();
        Ok(SitePlan {
            fit,
            scale,
            residual,
            parcel,
            number: plan.nummer.clone(),
            marks,
            lines,
            hints: plan.hinweise.clone(),
        })
    }

    /// Einpassung ins Aufmaß über Grenzpunkte mit gleicher Nummer
    /// (Ähnlichkeit, kleinste Quadrate) und die größte Abweichung an ihnen
    /// (m). `None` mit weniger als zwei gemeinsamen Nummern.
    pub fn fit_to(&self, points: &[TerrainPoint]) -> Option<(Affine, f64)> {
        let paare: Vec<([f64; 2], [f64; 2])> = self
            .marks
            .iter()
            .filter_map(|(nr, p)| {
                let q = points.iter().find(|q| &q.nr == nr && !q.off)?;
                Some((*p, [q.e, q.n]))
            })
            .collect();
        if paare.len() < 2 {
            return None;
        }
        let e = Einpassung::aehnlich(&paare)?;
        let fehler = paare
            .iter()
            .map(|(p, u)| e.abweichung(*p, *u))
            .fold(0.0, f64::max);
        Some((e.matrix, fehler))
    }
}

/// Grundstück und Unterlage des alten Geländes ins neue: Die Grenze aus
/// dem Lageplan ersetzt die aus Grenzpunkten, der Zuschnitt bleibt.
pub(super) fn plan_erben(t: &mut Terrain, alt: &Terrain) {
    if !alt.parcels.is_empty() {
        t.parcels = alt.parcels.clone();
        t.boundaries.clear();
    }
    t.underlay = alt.underlay.clone();
    t.show = alt.show;
}

impl Model {
    /// Das Flurstück eines Lageplans übernehmen (im offenen Schritt, G1c
    /// „Übernehmen“): Es wird die Grundstücksgrenze, der Lageplan die
    /// Unterlage. Ohne Aufmaß entsteht ein ebenes Gelände in Form des
    /// Grundstücks, mittig unter dem Haus (oder an seinem Platz, wenn die
    /// Lage in UTM bekannt ist). Mit Aufmaß muss der Lageplan in dessen
    /// System liegen (gleiche UTM-Zone, oder eingepasst über
    /// [`SitePlan::fit_to`] und als `fit` übergeben). `false` ohne Lage
    /// oder Flurstück oder wenn die Systeme nicht passen.
    pub fn import_parcel(
        &mut self,
        plan: &SitePlan,
        src: &str,
        fit: Option<(Crs, Affine)>,
    ) -> bool {
        let (Some((crs, m)), Some((nr, ecken, _))) = (fit.or(plan.fit), &plan.parcel) else {
            return false;
        };
        if ecken.len() < 3 {
            return false;
        }
        let ring: Vec<(f64, f64)> = ecken
            .iter()
            .map(|p| {
                let [e, n] = affine_apply(&m, *p);
                (e, n)
            })
            .collect();
        let u = Underlay {
            src: src.to_string(),
            page: 1,
            fit: m,
            flst: nr.clone(),
        };
        // ein angenommenes Gelände (I1) weicht dem Lageplan und wird in
        // dessen System neu angelegt
        let angenommen = self.terrain().is_some_and(Terrain::assumed);
        match self.terrain() {
            Some(alt) if !alt.points.is_empty() && !angenommen => {
                let gleich = alt.crs == crs && (matches!(crs, Crs::Utm(_)) || fit.is_some());
                if !gleich {
                    return false;
                }
                let mut t = alt.clone();
                t.parcels = vec![ring];
                t.boundaries.clear();
                t.underlay = Some(u);
                self.set_terrain(Some(t))
            }
            _ => {
                let mut t = Terrain::new(crs);
                t.parcels = vec![ring];
                t.underlay = Some(u);
                let neu = self.import_terrain(t);
                if neu && angenommen {
                    self.seed_flat_terrain(&[]);
                }
                neu
            }
        }
    }

    /// Ein Höhenaufmaß übernehmen, das nicht im System des Lageplans
    /// liegt: `fit` passt das Blatt ins Aufmaß ein ([`SitePlan::fit_to`]).
    /// Grundstück und Unterlage werden umgerechnet, und das Aufmaß fällt so
    /// unter das Haus, dass es auf dem Grundstück bleibt, wo es stand (im
    /// offenen Schritt). Ohne Lageplan wie [`Model::import_terrain`].
    pub fn import_terrain_fitted(&mut self, mut t: Terrain, fit: Affine) -> bool {
        let vorher = self.foundation_terrain();
        let Some(rahmen) = self.plan_umrechnen(t.crs, fit) else {
            return self.import_terrain(t);
        };
        if let Some(alt) = self.terrain() {
            plan_erben(&mut t, alt);
            t.href = alt.href;
        }
        t.local = None;
        self.put_survey(Some(t));
        self.rahmen_schreiben(rahmen);
        self.terrain_nhn_merken();
        self.resettle_foundations(vorher);
        true
    }

    /// Das Gelände (nur Lageplan, ohne Punkte) ins System `crs` mit der
    /// Einpassung `fit` umrechnen: Grenzen, Unterlage und System neu, der
    /// Rahmen so, dass jeder Grenzpunkt im Modell bleibt, wo er war.
    /// `None` ohne Lageplan, mit Punkten oder ohne Lage.
    fn plan_umrechnen(&mut self, crs: Crs, fit: Affine) -> Option<SurveyFrame> {
        let alt = self.terrain()?;
        let f = self.survey_frame()?;
        let u = alt.underlay.as_ref()?;
        if !alt.points.is_empty() {
            return None;
        }
        // alt → neu: zurück aufs Blatt, dann ins neue System
        let a = nacheinander(&umgekehrt(&u.fit)?, &fit);
        let dreh = a[1].atan2(a[0]);
        let o = affine_apply(&a, [f.e0, f.n0]);
        let mut t = alt.clone();
        t.crs = crs;
        t.local = None;
        t.parcels = t
            .parcels
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| {
                        let [e, n] = affine_apply(&a, [p.0, p.1]);
                        (e, n)
                    })
                    .collect()
            })
            .collect();
        t.underlay = Some(Underlay { fit, ..u.clone() });
        self.put_survey(Some(t));
        Some(SurveyFrame {
            e0: o[0],
            n0: o[1],
            g: f.g + dreh,
        })
    }

    /// Linien eines Lageplans im Modell (mm), wenn er die Unterlage des
    /// Geländes ist (gleiche Datei) und die Lage bekannt ist.
    pub fn underlay_lines(&self, plan: &SitePlan) -> Vec<(Vec<[f64; 2]>, bool, UnderlayKind)> {
        let (Some(t), Some(f)) = (self.terrain(), self.survey_frame()) else {
            return Vec::new();
        };
        let Some(u) = &t.underlay else {
            return Vec::new();
        };
        plan.lines
            .iter()
            .map(|(p, zu, art)| {
                let q = p
                    .iter()
                    .map(|p| {
                        let [e, n] = affine_apply(&u.fit, *p);
                        f.to_model(e, n)
                    })
                    .collect();
                (q, *zu, *art)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN: &[u8] = include_bytes!("../../sk-gelaende/tests/daten/lageplan-lindenhof-7.pdf");
    const CSV: &[u8] =
        include_bytes!("../../sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");

    /// Haus des Beispiels: Außenfläche x 7 bis 17 m, y 6 bis 14 m.
    fn haus() -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            sk_math::vec3(7000.0, 6000.0, 0.0),
            sk_math::vec3(7000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m
    }

    fn mitte(r: &[[f64; 2]]) -> [f64; 2] {
        let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
        for p in r {
            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
        [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
    }

    #[test]
    fn umkehrung_und_folge() {
        let m = [2.0, 0.5, -0.3, 1.5, 10.0, -4.0];
        let u = umgekehrt(&m).unwrap();
        let p = [3.0, 7.0];
        let q = affine_apply(&u, affine_apply(&m, p));
        assert!((q[0] - p[0]).abs() < 1e-12 && (q[1] - p[1]).abs() < 1e-12);
        let n = [0.0, 1.0, -1.0, 0.0, 5.0, 5.0];
        let a = affine_apply(&nacheinander(&m, &n), p);
        let b = affine_apply(&n, affine_apply(&m, p));
        assert!((a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    }

    /// Lageplan 1:500 ohne Gitterkreuze: Flurstück 123/4 mit 822,87 m²,
    /// eben und mittig unter dem Haus, zugeschnitten. Danach das Aufmaß in
    /// UTM: eingepasst über die Grenzpunkte 1 bis 4, das Grundstück bleibt,
    /// wo es stand, und die Grenze liegt auf den Grenzpunkten des Aufmaßes.
    #[test]
    fn lageplan_dann_hoehen() {
        let plan = SitePlan::read(PLAN, 32).unwrap();
        assert_eq!(plan.scale, Some(500.0));
        assert!(matches!(plan.fit, Some((Crs::Local, _))));
        let (nr, ecken, flaeche) = plan.parcel.clone().unwrap();
        assert_eq!((nr.as_str(), ecken.len()), ("123/4", 4));
        assert!((flaeche.unwrap() - 822.87).abs() < 0.1);
        assert_eq!(plan.marks.len(), 4);
        assert!(plan.lines.iter().any(|l| l.2 == UnderlayKind::Boundary));

        let mut m = haus();
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, "lageplan.pdf", None));
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.points.is_empty() && t.show() == Show::Parcel);
        assert!((t.parcel_area() - 822.87).abs() < 0.1);
        let r = m.terrain_boundaries();
        assert_eq!(r.len(), 1);
        // Nullpunkt auf volle Meter
        let c = mitte(&r[0]);
        assert!(
            (c[0] - 12000.0).abs() < 750.0 && (c[1] - 10000.0).abs() < 750.0,
            "{c:?}"
        );
        assert!(m.terrain_triangles().is_empty(), "ohne Höhen kein Netz");
        assert!(!m.underlay_lines(&plan).is_empty());

        // Höhen in UTM: eingepasst über gleiche Nummern
        let (aufmass, ..) = Terrain::lesen(CSV, "a.csv", "2026-10-12", 32).unwrap();
        let (fit, fehler) = plan.fit_to(&aufmass.points).unwrap();
        assert!(fehler < 0.02, "{fehler} m");
        let vorher = r[0].clone();
        m.begin("Höhen");
        assert!(m.import_terrain_fitted(aufmass, fit));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.crs, Crs::Utm(32));
        assert!(t.boundaries.is_empty() && t.parcels.len() == 1);
        assert_eq!(t.underlay.as_ref().unwrap().flst, "123/4");
        let nachher = m.terrain_boundaries();
        for (a, b) in vorher.iter().zip(&nachher[0]) {
            assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 20.0, "{a:?} {b:?}");
        }
        // Die Grenze liegt auf den Grenzpunkten des Aufmaßes (±2 cm)
        for (e, n) in &t.parcels[0] {
            let d = t
                .points
                .iter()
                .filter(|p| p.code == "GR")
                .map(|p| (p.e - e).hypot(p.n - n))
                .fold(f64::MAX, f64::min);
            assert!(d < 0.02, "{d} m");
        }
        assert!(!m.terrain_triangles().is_empty());
    }

    /// Ein angenommenes Gelände (I1) weicht dem Lageplan: Grenze und Lage
    /// kommen aus dem Plan, die Punkte von Hand neu in seinem System, so
    /// dass die Fläche darauf weiter Mengen hat.
    #[test]
    fn lageplan_auf_angenommenem_gelaende() {
        let plan = SitePlan::read(PLAN, 32).unwrap();
        let mut m = haus();
        let ring = vec![[20000.0, 6000.0], [24000.0, 6000.0], [24000.0, 9000.0]];
        m.begin("Fläche");
        assert!(m.seed_flat_terrain(&ring));
        assert!(m
            .add_site_patch(crate::TerrainPatch::flat(ring, -100.0))
            .is_some());
        m.commit();
        assert_eq!(m.terrain().unwrap().crs, Crs::Local);
        m.begin("Lageplan");
        assert!(m.import_parcel(&plan, "lageplan.pdf", None));
        m.commit();
        let t = m.terrain().unwrap();
        assert_eq!(t.parcels.len(), 1);
        assert_eq!(t.underlay.as_ref().unwrap().flst, "123/4");
        assert!(t.assumed(), "{:?}", t.points);
        // die Fläche liegt im neuen Gelände
        let f = m.survey_frame().unwrap();
        let (e, n) = f.to_survey(24000.0, 9000.0);
        assert!(t.height(e, n).is_some());
        assert!(m.planned_terrain().is_some());
    }

    /// Erst das Aufmaß in UTM, dann der Lageplan eingepasst: die Grenze aus
    /// dem Lageplan ersetzt die aus den Grenzpunkten; ohne gemeinsame
    /// Nummern übernimmt Skizzeo nichts.
    #[test]
    fn hoehen_dann_lageplan() {
        let plan = SitePlan::read(PLAN, 32).unwrap();
        let mut m = haus();
        let (aufmass, ..) = Terrain::lesen(CSV, "a.csv", "2026-10-12", 32).unwrap();
        m.begin("Höhen");
        assert!(m.import_terrain(aufmass));
        m.commit();
        let flaeche = m.terrain().unwrap().parcel_area();
        m.begin("Lageplan");
        assert!(
            !m.import_parcel(&plan, "lageplan.pdf", None),
            "Systeme passen nicht"
        );
        let (fit, _) = plan.fit_to(&m.terrain().unwrap().points).unwrap();
        assert!(m.import_parcel(&plan, "lageplan.pdf", Some((Crs::Utm(32), fit))));
        m.commit();
        let t = m.terrain().unwrap();
        assert!(t.boundaries.is_empty() && t.parcels.len() == 1);
        assert!((t.parcel_area() - flaeche).abs() < 0.5);
        let mut ohne = t.points.clone();
        for p in &mut ohne {
            p.nr = format!("x{}", p.nr);
        }
        assert!(plan.fit_to(&ohne).is_none());
    }
}

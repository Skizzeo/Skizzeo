//! Erdmengen über das Geländenetz (G2, plan-gelaendemodul.md §9.1): statt
//! einer Ebene in mittlerer Geländehöhe die Integrale über die Dreiecke des
//! Aufmaßes, Abtrag und Auftrag punktweise getrennt. Auflagen der
//! VOB-Prüfung (vob/pruefung-gelaende-g2.md): A1 nie ein Saldo, A2 der
//! Ansatz nennt das Verfahren, A3 die Marke [`AUS_AUFMASS`] nur an der
//! Böschung, A4 Hinweis bei Gelände steiler als 1:10.
//!
//! N = Gelände − Oberboden; Baugrube = Platte plus Arbeitsraum (Sohle),
//! innen = innerhalb der Frostschürze, Ring = Schürze. Alles in mm.

use super::{zahl, Boden, AUS_AUFMASS, BACKFILL, DISPOSAL, EXCAVATION, FILL, SENKRECHT_BIS, SLOPE};
use super::{ErdBasis, ErdZone, TRENCH};
use crate::{Model, RunId, SurveyFrame, TerrainMesh};
use sk_gelaende::volumen::Erdmenge;
use sk_math::Vec3;
use std::sync::Arc;

/// Der Ansatz mit Geländemodell nennt das Verfahren (Auflage A2); das LV
/// setzt daran die Vorbemerkung zum Ermittlungsverfahren.
pub const DREIECKSPRISMEN: &str = "aus Geländemodell, Dreiecksprismen (REB-VB 22.013)";

/// Zusatz am ersten Aushub, wenn das Gelände an der Baugrube steiler
/// ansteigt als [`STEIL_AB`] (Auflage A4); [`steil_hinweis`] macht daraus
/// den Hinweis.
pub const STEIL: &str = "; Gelände steigt an der Baugrube mit ";

/// Bis zu diesem Gefälle darf eine Baugrube bis 1,25 m senkrecht sein
/// (DIN 4124 4.2.2).
pub const STEIL_AB: f64 = 0.1;

/// Mittlere Höhe der Menge `v` (mm³) über der Fläche `f` (mm²); Kleinstmengen
/// am Rand des Netzes zeigen „unter 1 cm“ statt 0,00 m neben einer Menge über
/// null.
fn mittel(v: f64, f: f64) -> String {
    let h = if f > 0.0 { v / f } else { 0.0 };
    if h > 0.0 && h < 5.0 {
        "unter 1 cm".to_string()
    } else {
        format!("{} m", zahl(h, 1e3))
    }
}

/// Hinweis zu einem Ansatz mit [`STEIL`]; `None` ohne.
pub fn steil_hinweis(ansatz: &str) -> Option<String> {
    let rest = &ansatz[ansatz.find(STEIL)? + STEIL.len()..];
    let x = &rest[..rest.find(" % an")?];
    Some(format!(
        "Gelände steigt an der Baugrube mit {x} % an: senkrechte Wand nach DIN 4124 nur bis \
         1:10 bzw. bei steifem bindigem Boden bis 1:2; Böschung oder Verbau prüfen"
    ))
}

/// Das Höhenaufmaß, wie die Erdmengen einer Gründung es brauchen: Netz,
/// Lage und Höhe von ±0,00, dazu der Umriss innerhalb der Frostschürze.
/// Gleich, wenn es dasselbe Netz in derselben Lage ist.
#[derive(Clone)]
pub struct ErdNetz {
    mesh: Arc<TerrainMesh>,
    frame: SurveyFrame,
    /// Höhe von ±0,00 in m ü. NHN.
    h0: f64,
    /// „Aufmaß <Datei>, <Datum>“.
    aufmass: String,
    /// Innerhalb der Frostschürze, gegen den Uhrzeigersinn; leer, wenn die
    /// Schürze die ganze Platte füllt.
    pub innen: Vec<Vec3>,
}

impl PartialEq for ErdNetz {
    fn eq(&self, o: &ErdNetz) -> bool {
        Arc::ptr_eq(&self.mesh, &o.mesh)
            && (self.frame, self.h0, &self.aufmass, &self.innen)
                == (o.frame, o.h0, &o.aufmass, &o.innen)
    }
}

impl std::fmt::Debug for ErdNetz {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ErdNetz({} Dreiecke)", self.dreiecke())
    }
}

impl ErdNetz {
    /// Das Aufmaß des Modells unter der Gründung `run`; `None` ohne
    /// Aufmaß, Lage, Höhenbezug oder Gründung.
    pub fn neu(m: &Model, run: RunId) -> Option<ErdNetz> {
        let t = m.terrain()?;
        let f = m.foundation(run)?.ok()?;
        let innen = match &f.footing {
            crate::foundation::FootingShape::Ring(i) => i.pts.clone(),
            crate::foundation::FootingShape::Full(_) => Vec::new(),
        };
        Some(ErdNetz {
            frame: m.survey_frame()?,
            h0: m.terrain_nhn()?,
            mesh: t.mesh(),
            aufmass: if t.read.is_empty() {
                format!("Aufmaß {}", t.src)
            } else {
                format!("Aufmaß {}, {}", t.src, t.read)
            },
            innen,
        })
    }

    fn dreiecke(&self) -> usize {
        self.mesh.netz.dreiecke.len()
    }

    /// Punkt im Modell (mm) im Netz (m relativ zu seinem Bezug).
    fn im_netz(&self, p: Vec3) -> [f64; 2] {
        let (e, n) = self.frame.to_survey(p.x, p.y);
        [e - self.mesh.bezug.0, n - self.mesh.bezug.1]
    }

    /// Gelände über `umriss` gegen die Höhe `z` über ±0,00: Fläche im Netz
    /// (mm²), darüber und darunter (mm³), größtes Gefälle.
    fn menge(&self, umriss: &[Vec3], z: f64) -> Erdmenge {
        self.mengen(umriss, &[z])[0]
    }

    /// [`ErdNetz::menge`] je Höhe in `zs`, der Umriss einmal geschnitten
    /// (Review #164 H-164b).
    fn mengen<const N: usize>(&self, umriss: &[Vec3], zs: &[f64; N]) -> [Erdmenge; N] {
        let r: Vec<[f64; 2]> = umriss.iter().map(|&p| self.im_netz(p)).collect();
        let bezuege = zs.map(|z| [self.h0 + z / 1000.0, 0.0, 0.0]);
        let e = self.mesh.abfrage.erdmengen(&r, &bezuege);
        std::array::from_fn(|i| Erdmenge {
            flaeche: e[i].flaeche * 1e6,
            ueber: e[i].ueber * 1e9,
            unter: e[i].unter * 1e9,
            flaeche_ueber: e[i].flaeche_ueber * 1e6,
            flaeche_unter: e[i].flaeche_unter * 1e6,
            steil: e[i].steil,
        })
    }

    /// Geländehöhe über ±0,00 an `p` (mm).
    fn hoehe(&self, p: Vec3) -> Option<f64> {
        let q = self.im_netz(p);
        Some((self.mesh.abfrage.hoehe(q[0], q[1])? - self.h0) * 1000.0)
    }

    /// Profil von `a` nach `b`: (Abstand, Geländehöhe über ±0,00) in mm an
    /// jedem Schnitt mit einer Netzkante; dazwischen geradlinig.
    fn profil(&self, a: Vec3, b: Vec3) -> Vec<[f64; 2]> {
        self.mesh
            .abfrage
            .profil(self.im_netz(a), self.im_netz(b))
            .into_iter()
            .map(|[s, h]| [s * 1000.0, (h - self.h0) * 1000.0])
            .collect()
    }
}

/// tan(φ/2) je Ecke mit dem Außenwinkel φ (links herum positiv): ein
/// Umriss, um `x` nach außen versetzt, wächst je Ecke um tan(φ/2)·x².
pub(super) fn ecken_tan(pts: &[Vec3]) -> Vec<f64> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            let (u, v) = ((b.x - a.x, b.y - a.y), (c.x - b.x, c.y - b.y));
            if u.0.hypot(u.1) <= 0.0 || v.0.hypot(v.1) <= 0.0 {
                return 0.0;
            }
            let phi = (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1);
            (phi / 2.0).tan()
        })
        .collect()
}

/// Böschung über dem Sohlrand (Auflage A3, Empfehlung der VOB-Prüfung
/// Frage 3): je Randstück mit seiner örtlichen Tiefe h(s) = N − UK Platte,
/// nur wo das Gelände mehr als 1,25 m über UK Platte liegt. Der Keil ist
/// ∮ k·h²/2 ds plus je Ecke tan(φ/2)·k²·h³/3, die schräge Fläche
/// (∮ h ds + Σ tan(φ/2)·k·h²)/sin β. Angenähert bleibt, dass das Gelände
/// über die Breite der Böschung weiter steigt (G5).
#[derive(Default)]
struct Boeschung {
    keil: f64,
    flaeche: f64,
    /// Länge des Sohlrands mit Böschung, größte Tiefe dort.
    lang: f64,
    tief: f64,
}

fn boeschung(n: &ErdNetz, sohle: &[Vec3], z_s: f64, o: f64, beta: f64) -> Boeschung {
    let mut b = Boeschung::default();
    let beta = beta.clamp(1.0, 90.0).to_radians();
    let (k, sin) = (1.0 / beta.tan(), beta.sin());
    // h = N − z_s; Gelände − z_s > 1,25 m
    let schwelle = SENKRECHT_BIS - o;
    let m = sohle.len();
    for i in 0..m {
        let p = n.profil(sohle[i], sohle[(i + 1) % m]);
        for w in p.windows(2) {
            let (mut s0, mut h0) = (w[0][0], w[0][1] - o - z_s);
            let (mut s1, mut h1) = (w[1][0], w[1][1] - o - z_s);
            if h0 <= schwelle && h1 <= schwelle {
                continue;
            }
            // nur der Teil über der Schwelle; h ist dazwischen linear
            if h0 < schwelle {
                s0 += (s1 - s0) * (schwelle - h0) / (h1 - h0);
                h0 = schwelle;
            } else if h1 < schwelle {
                s1 = s0 + (s1 - s0) * (h0 - schwelle) / (h0 - h1);
                h1 = schwelle;
            }
            let l = s1 - s0;
            b.keil += k * l * (h0 * h0 + h0 * h1 + h1 * h1) / 6.0;
            b.flaeche += l * (h0 + h1) / 2.0 / sin;
            b.lang += l;
            b.tief = b.tief.max(h0).max(h1);
        }
    }
    for (p, tan) in sohle.iter().zip(ecken_tan(sohle)) {
        let Some(h) = n.hoehe(*p).map(|g| g - o - z_s) else {
            continue;
        };
        if h > schwelle {
            b.keil += tan * k * k * h * h * h / 3.0;
            b.flaeche += tan * k * h * h / sin;
        }
    }
    b
}

/// Erdmengen der Gründung über das Netz `n`, dazu Baufeld und Baugrube.
/// `None`, wenn das Netz die Baugrube nicht ganz deckt oder der
/// Arbeitsraum sich nicht versetzen lässt; dann rechnet
/// [`super::erd_mengen`] mit der mittleren Höhe.
pub(super) fn mengen(
    b: &ErdBasis,
    n: &ErdNetz,
    boden: &Boden,
    menge: &mut dyn FnMut(&'static str, f64, String),
) -> Option<ErdZone> {
    let o = boden.oberboden.max(0.0);
    let a = boden.arbeitsraum.max(0.0);
    let g = boden.tragschicht.max(0.0);
    let fl = b.slab_area;
    let z_s = b.slab_bottom_z;
    let z_b = b.insulation_bottom_z.min(z_s);
    let z_p = z_b - g;
    let z_f = b.footing_bottom_z;
    let sohle = if a > 0.0 {
        sk_math::flaeche::versatz(&b.outline, &vec![-a; b.outline.len()])?
    } else {
        b.outline.clone()
    };
    let sohle_fl = sk_math::polygon::area(&sohle);
    let grube = n.menge(&sohle, z_s + o);
    if sohle_fl <= 0.0 || grube.flaeche < sohle_fl * (1.0 - 1e-6) {
        return None;
    }
    let [platte, platte_f] = n.mengen(&b.outline, &[z_s + o, z_f + o]).map(|e| e.ueber);
    let (innen_fl, i_p, i_s, i_f) = if n.innen.len() >= 3 {
        let [i_p, i_s, i_f] = n.mengen(&n.innen, &[z_p + o, z_s + o, z_f + o]);
        (sk_math::polygon::area(&n.innen), i_p, i_s.ueber, i_f.ueber)
    } else {
        (0.0, Erdmenge::default(), 0.0, 0.0)
    };
    let ring_fl = (fl - innen_fl).max(0.0);
    // ∫innen (min(z_s, N) − z_p)⁺ = ∫(N − z_p)⁺ − ∫(N − z_s)⁺, ebenso der Ring
    let bett = (i_p.ueber - i_s).max(0.0);
    let graben = ((platte_f - platte) - (i_f - i_s)).max(0.0);
    let auf = i_p.unter;
    let bo = boeschung(n, &sohle, z_s, o, boden.boeschung);
    let verfahren = format!(
        "; {DREIECKSPRISMEN}, {} Dreiecke, {}",
        n.dreiecke(),
        n.aufmass
    );
    let breite = |x: f64| format!("{} m", zahl(x, 1e3));

    // Oberboden über das Baufeld, mindestens bis zum Rand der Böschung
    let beta = boden.boeschung.clamp(1.0, 90.0).to_radians();
    let x_ob = boden.rand.max(a + bo.tief / beta.tan());
    super::oberboden(b, x_ob, o, menge);
    // Hinweis A4 am ersten Aushub, wenn es einen gibt
    let mut steil = (grube.steil > STEIL_AB).then(|| {
        format!(
            "{STEIL}{} % an",
            format!("{:.1}", grube.steil * 100.0).replace('.', ",")
        )
    });
    let mut steil_dazu = |s: String| s + &steil.take().unwrap_or_default();
    if grube.ueber > 0.0 {
        menge(
            EXCAVATION,
            grube.ueber,
            steil_dazu(format!(
                "Sohle {} m² (Platte + Arbeitsraum {}) bis UK Platte, im Mittel {}{verfahren}",
                zahl(sohle_fl, 1e6),
                breite(a),
                mittel(grube.ueber, sohle_fl)
            )),
        );
    }
    if bo.keil > 0.0 {
        menge(
            EXCAVATION,
            bo.keil,
            format!(
                "Böschung {}° auf {} Sohlrand, Tiefe bis {}{AUS_AUFMASS}, Böschung am Gelände \
                 angenähert",
                boden.boeschung.round(),
                breite(bo.lang),
                breite(bo.tief)
            ),
        );
    }
    if bett > 0.0 {
        let wofuer = if z_b < z_s {
            "Dämmung und Schicht"
        } else {
            "Schicht"
        };
        menge(
            EXCAVATION,
            bett,
            format!(
                "für {wofuer} {} m², im Mittel {}{verfahren}",
                zahl(innen_fl, 1e6),
                mittel(bett, innen_fl)
            ),
        );
    }
    if graben > 0.0 {
        menge(
            TRENCH,
            graben,
            steil_dazu(format!(
                "Ring {} m² bis UK Schürze, im Mittel {}{verfahren}",
                zahl(ring_fl, 1e6),
                mittel(graben, ring_fl)
            )),
        );
    }
    super::planum(innen_fl, g, menge);
    if auf > 0.0 {
        menge(
            FILL,
            auf,
            format!(
                "{} m² bis UK Schicht, im Mittel {}{verfahren}",
                zahl(innen_fl, 1e6),
                mittel(auf, innen_fl)
            ),
        );
    }
    // Arbeitsraum: Baugrube samt Böschung außerhalb der Platte; die
    // Böschung ist angenähert, die Abfuhr nicht (sie steht in Aushub und
    // Verfüllung gleich)
    let gesamt = grube.ueber + bo.keil;
    let verfuellen = (gesamt - platte).max(0.0);
    if verfuellen > 0.0 {
        let marke = if bo.keil > 0.0 { AUS_AUFMASS } else { "" };
        menge(
            BACKFILL,
            verfuellen,
            format!(
                "Baugrube {} m³ − Platte {} m³ bis UK Platte{verfahren}{marke}",
                zahl(gesamt, 1e9),
                zahl(platte, 1e9)
            ),
        );
    }
    let aushub = gesamt + bett + graben;
    let abfuhr = aushub - verfuellen;
    if abfuhr > 0.0 {
        menge(
            DISPOSAL,
            abfuhr,
            format!(
                "Aushub {} m³ − Verfüllung {} m³",
                zahl(aushub, 1e9),
                zahl(verfuellen, 1e9)
            ),
        );
    }
    if bo.flaeche > 0.0 {
        menge(
            SLOPE,
            bo.flaeche,
            format!(
                "Böschung auf {} Sohlrand, Tiefe bis {}{AUS_AUFMASS}, Böschung am Gelände angenähert",
                breite(bo.lang),
                breite(bo.tief)
            ),
        );
    }
    Some(ErdZone {
        baufeld: x_ob,
        sohle: if grube.ueber > 0.0 { sohle } else { Vec::new() },
        b: z_s,
        keil: bo.keil > 0.0,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn mittel_unter_1_cm() {
        assert_eq!(super::mittel(30e6, 10e6), "unter 1 cm");
        assert_eq!(super::mittel(50e6, 10e6), "0,01 m");
        assert_eq!(super::mittel(1.2e9, 1e6), "1,20 m");
        assert_eq!(super::mittel(0.0, 1e6), "0,00 m");
        assert_eq!(super::mittel(1.0, 0.0), "0,00 m");
    }
}

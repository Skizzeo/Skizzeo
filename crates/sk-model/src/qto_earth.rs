//! Erdarbeiten als Automatikmengen (VOB/C ATV DIN 18300, DIN 4124): aus der
//! Gründung eines Gebäudes und den Bodenkennwerten des Projekts. Nur Lesen,
//! keine Körper. Die Kosten rechnen damit über Bauleistungen mit
//! `auto=earth.…` (kosten/erdarbeiten-recherche.md).
//!
//! Höhen relativ zu ±0,00 (OK Sohlplatte), nach oben positiv:
//! OK Gelände → (Oberboden) → gewachsener Boden → UK Platte bzw. UK Dämmung
//! → (kapillarbrechende Schicht innerhalb der Frostschürze) → UK Schürze.
//! Die Baugrube reicht bis UK Dämmung und um den Arbeitsraum über den
//! Plattenumriss hinaus; die Frostschürze ist erdgeschalt, ihr Graben so
//! breit wie sie selbst.

use crate::foundation::Foundation;
use crate::qto::AutoMenge;
use sk_math::Vec3;

/// Bodenkennwerte des Projekts (mm, Grad); stehen am Projekt
/// ([`crate::Project::soil`]) und in der Datei nur, wenn sie von der
/// Vorgabe abweichen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Boden {
    /// Oberboden: abtragen und seitlich lagern (DIN 18915, ATV DIN 18320).
    pub oberboden: f64,
    /// Baufeld: so weit um den Plattenumriss wird der Oberboden abgetragen
    /// (Arbeitsraum, Gerüst, Wege); mindestens bis zum Rand der Baugrube.
    pub rand: f64,
    /// Arbeitsraum um den Plattenumriss (DIN 4124: mindestens 0,50 m).
    pub arbeitsraum: f64,
    /// Kapillarbrechende Schicht unter der Platte innerhalb der Schürze.
    pub tragschicht: f64,
    /// Böschungswinkel β, wenn die Baugrube tiefer ist als
    /// [`SENKRECHT_BIS`] (DIN 4124: 45° nichtbindig oder weich bindig, 60°
    /// steif bindig, 80° Fels).
    pub boeschung: f64,
}

impl Default for Boden {
    fn default() -> Boden {
        Boden {
            oberboden: 300.0,
            rand: 1500.0,
            arbeitsraum: 500.0,
            tragschicht: 150.0,
            boeschung: 45.0,
        }
    }
}

impl Boden {
    /// Schlüssel an `[project]`, Bezeichnung und erlaubter Bereich (mm bzw.
    /// Grad), in der Reihenfolge der Felder.
    pub const FELDER: [(&'static str, &'static str, f64, f64); 5] = [
        ("topsoil", "Oberboden", 0.0, 1000.0),
        ("strip", "Baufeld um die Platte", 0.0, 10000.0),
        ("workspace", "Arbeitsraum", 0.0, 2000.0),
        ("capillary", "Kapillarbrechende Schicht", 0.0, 1000.0),
        ("slope", "Böschungswinkel", 30.0, 90.0),
    ];

    /// Die Werte in der Reihenfolge von [`Boden::FELDER`].
    pub fn werte(&self) -> [f64; 5] {
        [
            self.oberboden,
            self.rand,
            self.arbeitsraum,
            self.tragschicht,
            self.boeschung,
        ]
    }

    /// Aus Werten in der Reihenfolge von [`Boden::FELDER`]; `None`, wenn
    /// einer außerhalb seines Bereichs liegt.
    pub fn aus_werten(w: [f64; 5]) -> Option<Boden> {
        let ok = w
            .iter()
            .zip(Boden::FELDER)
            .all(|(v, f)| v.is_finite() && *v >= f.2 && *v <= f.3);
        ok.then_some(Boden {
            oberboden: w[0],
            rand: w[1],
            arbeitsraum: w[2],
            tragschicht: w[3],
            boeschung: w[4],
        })
    }
}

/// Bis zu dieser Tiefe (mm ab Gelände) darf die Baugrube ohne Böschung
/// und ohne Verbau senkrecht sein (DIN 4124, 4.2.3).
pub const SENKRECHT_BIS: f64 = 1250.0;

/// Kostengruppe der Erdarbeiten (DIN 276: 311 Herstellung der Baugrube).
pub const KG: u16 = 311;

/// Schlüssel der Erdmengen (`[service] auto=`), Einheit und Bedeutung.
pub const SCHLUESSEL: [(&str, &str, &str); 9] = [
    (TOPSOIL, "m3", "Oberboden abtragen und seitlich lagern"),
    (
        EXCAVATION,
        "m3",
        "Baugrube ausheben bis UK Platte bzw. Dämmung",
    ),
    (TRENCH, "m3", "Graben der Frostschürze ausheben"),
    (
        SUBGRADE,
        "m2",
        "Planum unter der kapillarbrechenden Schicht",
    ),
    (GRAVEL, "m3", "kapillarbrechende Schicht unter der Platte"),
    (
        FILL,
        "m3",
        "Auffüllung bis UK Schicht, wenn die Platte höher liegt",
    ),
    (BACKFILL, "m3", "Arbeitsraum verfüllen"),
    (DISPOSAL, "m3", "überschüssigen Aushub laden und abfahren"),
    (
        SLOPE,
        "m2",
        "Böschungsfläche abdecken (nur über 1,25 m Tiefe)",
    ),
];

pub const TOPSOIL: &str = "earth.topsoil";
pub const EXCAVATION: &str = "earth.excavation";
pub const TRENCH: &str = "earth.trench";
pub const SUBGRADE: &str = "earth.subgrade";
pub const GRAVEL: &str = "earth.gravel";
pub const FILL: &str = "earth.fill";
pub const BACKFILL: &str = "earth.backfill";
pub const DISPOSAL: &str = "earth.disposal";
pub const SLOPE: &str = "earth.slope";

/// Höhen und Flächen einer Gründung (mm, mm²), wie die Erdmengen sie
/// brauchen.
#[derive(Clone, Debug, PartialEq)]
pub struct ErdBasis {
    /// OK Gelände.
    pub terrain_z: f64,
    /// UK Perimeterdämmung, ohne Dämmung UK Platte.
    pub insulation_bottom_z: f64,
    /// UK Frostschürze.
    pub footing_bottom_z: f64,
    /// Plattenumriss gegen den Uhrzeigersinn.
    pub outline: Vec<Vec3>,
    pub slab_area: f64,
    pub slab_perimeter: f64,
    /// Ring der Frostschürze im Grundriss.
    pub footing_area: f64,
}

impl ErdBasis {
    /// Aus den Grundlagen der Erdarbeiten des Modells (Gelände, Dämmung,
    /// Gründung; gelaende/schnittstelle.md).
    pub fn aus(g: &crate::GroundBasis) -> ErdBasis {
        ErdBasis {
            terrain_z: g.terrain_z,
            insulation_bottom_z: g.insulation_bottom_z,
            footing_bottom_z: g.footing_bottom_z,
            outline: g.outline.clone(),
            slab_area: g.slab_area,
            slab_perimeter: g.slab_perimeter,
            footing_area: g.footing_area,
        }
    }

    /// Aus der Gründung, mit dem Gelände in Höhe `terrain_z` und ohne
    /// Perimeterdämmung.
    pub fn aus_gruendung(f: &Foundation, terrain_z: f64) -> ErdBasis {
        let p = f.params;
        ErdBasis {
            terrain_z,
            insulation_bottom_z: -p.slab_thickness,
            footing_bottom_z: -p.slab_thickness - p.footing_depth,
            outline: f.outline.clone(),
            slab_area: f.slab_area(),
            slab_perimeter: f.slab_perimeter(),
            footing_area: f.footing_area(),
        }
    }
}

/// Summe von tan(φ/2) über die Außenwinkel φ der Ecken: Ein Umriss, um `x`
/// nach außen versetzt (Ecken auf Gehrung), hat die Fläche
/// `A + U·x + T·x²`. Rechteck: 4.
fn eckzahl(pts: &[Vec3]) -> f64 {
    let n = pts.len();
    let mut t = 0.0;
    for i in 0..n {
        let a = pts[(i + n - 1) % n];
        let b = pts[i];
        let c = pts[(i + 1) % n];
        let (u, v) = ((b.x - a.x, b.y - a.y), (c.x - b.x, c.y - b.y));
        let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
        if lu <= 0.0 || lv <= 0.0 {
            continue;
        }
        let kreuz = u.0 * v.1 - u.1 * v.0;
        let punkt = u.0 * v.0 + u.1 * v.1;
        // Außenwinkel, links herum positiv (gegen den Uhrzeigersinn)
        let phi = kreuz.atan2(punkt);
        t += (phi / 2.0).tan();
    }
    t
}

/// Zahl mit zwei Nachkommastellen und Komma, aus mm (`teiler` 1e3), mm²
/// (1e6) oder mm³ (1e9).
fn zahl(x: f64, teiler: f64) -> String {
    format!("{:.2}", x / teiler).replace('.', ",")
}

/// Erdmengen einer Gründung (Bauteil `element`, Nummer `number` der
/// Sohlplatte). Mengen ≤ 0 entstehen nicht.
pub fn erd_mengen(b: &ErdBasis, boden: &Boden, mut menge: impl FnMut(&'static str, f64, String)) {
    let o = boden.oberboden.max(0.0);
    let a = boden.arbeitsraum.max(0.0);
    let g = boden.tragschicht.max(0.0);
    let (fl, u) = (b.slab_area, b.slab_perimeter);
    let ring = b.footing_area.min(fl).max(0.0);
    let innen = (fl - ring).max(0.0);
    let t = eckzahl(&b.outline);
    // gewachsener Boden unter dem Oberboden
    let z_n = b.terrain_z - o;
    let z_b = b.insulation_bottom_z;
    let z_p = z_b - g;
    // Baugrube bis UK Dämmung; Böschung, wenn sie tiefer reicht als 1,25 m
    let h = (z_n - z_b).max(0.0);
    let boeschung = b.terrain_z - z_b > SENKRECHT_BIS && boden.boeschung > 0.0;
    let k = if boeschung {
        1.0 / boden.boeschung.clamp(1.0, 90.0).to_radians().tan()
    } else {
        0.0
    };
    let x_oben = a + k * h;
    // Oberboden über das Baufeld, mindestens über die ganze Baugrube
    let x_ob = boden.rand.max(x_oben);
    let ob_flaeche = fl + u * x_ob + t * x_ob * x_ob;
    let grube = h * fl
        + u * (a * h + k * h * h / 2.0)
        + t * (a * a * h + a * k * h * h + k * k * h * h * h / 3.0);
    let breite = |x: f64| format!("{} m", zahl(x, 1e3));

    if o > 0.0 {
        menge(
            TOPSOIL,
            ob_flaeche * o,
            format!(
                "Platte {} m² + Baufeld {}: {} m² × {}",
                zahl(fl, 1e6),
                breite(x_ob),
                zahl(ob_flaeche, 1e6),
                breite(o)
            ),
        );
    }
    let mut aushub = 0.0;
    if grube > 0.0 {
        let wie = if boeschung {
            format!(
                "Platte {} m² + Arbeitsraum {}, Böschung {}°, Tiefe {}",
                zahl(fl, 1e6),
                breite(a),
                boden.boeschung.round(),
                breite(h)
            )
        } else {
            format!(
                "Platte {} m² + Arbeitsraum {} × Tiefe {}",
                zahl(fl, 1e6),
                breite(a),
                breite(h)
            )
        };
        menge(EXCAVATION, grube, wie);
        aushub += grube;
    }
    // Schicht innerhalb der Schürze, soweit unter dem gewachsenen Boden
    let bett = innen * (z_b.min(z_n) - z_p).max(0.0);
    if bett > 0.0 {
        menge(
            EXCAVATION,
            bett,
            format!(
                "für Schicht {} m² × {}",
                zahl(innen, 1e6),
                breite(z_b.min(z_n) - z_p)
            ),
        );
        aushub += bett;
    }
    let graben_h = (z_b.min(z_n) - b.footing_bottom_z).max(0.0);
    let graben = ring * graben_h;
    if graben > 0.0 {
        menge(
            TRENCH,
            graben,
            format!("Ring {} m² × {}", zahl(ring, 1e6), breite(graben_h)),
        );
        aushub += graben;
    }
    if innen > 0.0 {
        menge(
            SUBGRADE,
            innen,
            format!("innerhalb der Frostschürze {} m²", zahl(innen, 1e6)),
        );
    }
    if g > 0.0 && innen > 0.0 {
        menge(
            GRAVEL,
            innen * g,
            format!("{} m² × {}", zahl(innen, 1e6), breite(g)),
        );
    }
    let auf_h = (z_p - z_n).max(0.0);
    if auf_h > 0.0 && innen > 0.0 {
        menge(
            FILL,
            innen * auf_h,
            format!("{} m² × {} bis UK Schicht", zahl(innen, 1e6), breite(auf_h)),
        );
    }
    // Arbeitsraum: Baugrube außerhalb des Plattenumrisses
    let verfuellen = (grube - fl * h).max(0.0);
    if verfuellen > 0.0 {
        menge(
            BACKFILL,
            verfuellen,
            format!(
                "Baugrube {} m³ − Platte {} m² × {}",
                zahl(grube, 1e9),
                zahl(fl, 1e6),
                breite(h)
            ),
        );
    }
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
    if boeschung {
        // Schräge Fläche: Umfang auf halber Höhe × Böschungslänge
        let x_m = a + k * h / 2.0;
        let lang = h / boden.boeschung.clamp(1.0, 90.0).to_radians().sin();
        let flaeche = (u + 2.0 * t * x_m) * lang;
        menge(
            SLOPE,
            flaeche,
            format!(
                "Umfang {} × Böschung {}",
                breite(u + 2.0 * t * x_m),
                breite(lang)
            ),
        );
    }
}

/// Erdmengen der Gründung als Automatikmengen am Bauteil der Sohlplatte.
pub(crate) fn auto_mengen(vorlage: &AutoMenge, b: &ErdBasis, boden: &Boden) -> Vec<AutoMenge> {
    let mut out = Vec::new();
    erd_mengen(b, boden, |key, value, formula| {
        let unit = SCHLUESSEL.iter().find(|s| s.0 == key).map_or("m3", |s| s.1);
        out.push(AutoMenge {
            key,
            unit,
            value,
            kg: Some(KG),
            formula,
            ..vorlage.clone()
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rechteck(w: f64, d: f64) -> Vec<Vec3> {
        vec![
            sk_math::vec3(0.0, 0.0, 0.0),
            sk_math::vec3(w, 0.0, 0.0),
            sk_math::vec3(w, d, 0.0),
            sk_math::vec3(0.0, d, 0.0),
        ]
    }

    /// 10 × 8 m, Platte 200, Schürze 350 breit und 600 tief, Gelände = OK
    /// Platte.
    fn basis(terrain_z: f64) -> ErdBasis {
        let (w, d, s) = (10_000.0, 8_000.0, 350.0);
        let ring = w * d - (w - 2.0 * s) * (d - 2.0 * s);
        ErdBasis {
            terrain_z,
            insulation_bottom_z: -200.0,
            footing_bottom_z: -800.0,
            outline: rechteck(w, d),
            slab_area: w * d,
            slab_perimeter: 2.0 * (w + d),
            footing_area: ring,
        }
    }

    fn mengen(b: &ErdBasis, boden: &Boden) -> Vec<(&'static str, f64)> {
        let mut v: Vec<(&'static str, f64)> = Vec::new();
        erd_mengen(b, boden, |k, x, _| match v.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 += x,
            None => v.push((k, x)),
        });
        v
    }

    fn wert(v: &[(&str, f64)], k: &str) -> f64 {
        v.iter().find(|e| e.0 == k).map_or(0.0, |e| e.1)
    }

    #[test]
    fn eckzahl_rechteck_und_l() {
        assert!((eckzahl(&rechteck(5.0, 3.0)) - 4.0).abs() < 1e-9);
        // L-Form: 5 konvexe, 1 einspringende Ecke
        let l = [
            (0.0, 0.0),
            (6.0, 0.0),
            (6.0, 2.0),
            (2.0, 2.0),
            (2.0, 5.0),
            (0.0, 5.0),
        ]
        .map(|(x, y)| sk_math::vec3(x, y, 0.0));
        assert!((eckzahl(&l) - 4.0).abs() < 1e-9);
    }

    /// Gelände = OK Platte: Oberboden 0,30 und Platte 0,20 ergeben keine
    /// Baugrube, nur Schicht, Graben und Auffüllung bis UK Schicht.
    #[test]
    fn gelaende_auf_ok_platte() {
        let v = mengen(&basis(0.0), &Boden::default());
        // Baufeld 1,50 m um die Platte
        let oben = 80.0 + 36.0 * 1.5 + 4.0 * 2.25; // m²
        assert!((wert(&v, TOPSOIL) / 1e9 - oben * 0.3).abs() < 1e-9);
        // gewachsener Boden −300 unter UK Platte −200: keine Baugrube
        assert_eq!(wert(&v, BACKFILL), 0.0);
        // Schicht −200 … −350, davon unter −300 ausheben: 0,05 m
        let innen = (10.0 - 0.7) * (8.0 - 0.7);
        assert!((wert(&v, EXCAVATION) / 1e9 - innen * 0.05).abs() < 1e-9);
        // Graben −300 … −800
        let ring = 80.0 - innen;
        assert!((wert(&v, TRENCH) / 1e9 - ring * 0.5).abs() < 1e-9);
        assert!((wert(&v, GRAVEL) / 1e9 - innen * 0.15).abs() < 1e-9);
        assert_eq!(wert(&v, FILL), 0.0);
        assert!((wert(&v, SUBGRADE) / 1e6 - innen).abs() < 1e-9);
        assert_eq!(wert(&v, SLOPE), 0.0);
        let aushub = innen * 0.05 + ring * 0.5;
        assert!((wert(&v, DISPOSAL) / 1e9 - aushub).abs() < 1e-9);
    }

    /// Platte 0,50 m unter Gelände: Baugrube mit Arbeitsraum, Verfüllung
    /// außerhalb der Platte.
    #[test]
    fn platte_tiefer() {
        let v = mengen(&basis(500.0), &Boden::default());
        // gewachsen +200, UK Platte −200: 0,40 m
        let grube = 0.4 * 80.0 + 36.0 * 0.5 * 0.4 + 4.0 * 0.25 * 0.4;
        let innen = 9.3 * 7.3;
        let ring = 80.0 - innen;
        assert!((wert(&v, EXCAVATION) / 1e9 - (grube + innen * 0.15)).abs() < 1e-9);
        assert!((wert(&v, TRENCH) / 1e9 - ring * 0.6).abs() < 1e-9);
        let verf = grube - 80.0 * 0.4;
        assert!((wert(&v, BACKFILL) / 1e9 - verf).abs() < 1e-9);
        let aushub = grube + innen * 0.15 + ring * 0.6;
        assert!((wert(&v, DISPOSAL) / 1e9 - (aushub - verf)).abs() < 1e-9);
    }

    /// Platte 0,40 m über Gelände: Auffüllung bis UK Schicht.
    #[test]
    fn platte_hoeher() {
        let v = mengen(&basis(-400.0), &Boden::default());
        let innen = 9.3 * 7.3;
        // gewachsen −700, UK Schicht −350
        assert!((wert(&v, FILL) / 1e9 - innen * 0.35).abs() < 1e-9);
        assert_eq!(wert(&v, EXCAVATION), 0.0);
        assert!((wert(&v, TRENCH) / 1e9 - (80.0 - innen) * 0.1).abs() < 1e-9);
    }

    /// Über 1,25 m Tiefe: Böschung, die Baugrube wird oben breiter.
    #[test]
    fn boeschung_ab_125() {
        let b = basis(1_500.0);
        let v = mengen(&b, &Boden::default());
        // gewachsen +1200, UK −200: h = 1,40 m, k = 1 (45°)
        let (h, a, k) = (1.4, 0.5, 1.0);
        let grube = h * 80.0
            + 36.0 * (a * h + k * h * h / 2.0)
            + 4.0 * (a * a * h + a * k * h * h + k * k * h * h * h / 3.0);
        let innen = 9.3 * 7.3;
        assert!((wert(&v, EXCAVATION) / 1e9 - (grube + innen * 0.15)).abs() < 1e-6);
        assert!(wert(&v, SLOPE) > 0.0);
        // Die Baugrube reicht oben 1,90 m über die Platte, weiter als das
        // Baufeld: Oberboden bis zu ihrem Rand
        let x = a + k * h;
        let oben = 80.0 + 36.0 * x + 4.0 * x * x;
        assert!((wert(&v, TOPSOIL) / 1e9 - oben * 0.3).abs() < 1e-6);
        let senkrecht = mengen(&basis(1_000.0), &Boden::default());
        assert_eq!(wert(&senkrecht, SLOPE), 0.0);
    }

    /// Bereiche der Bodenkennwerte; die Vorgabe liegt darin.
    #[test]
    fn boden_bereiche() {
        let d = Boden::default();
        assert_eq!(Boden::aus_werten(d.werte()), Some(d));
        let mut w = d.werte();
        w[4] = 20.0;
        assert_eq!(Boden::aus_werten(w), None);
        w[4] = 60.0;
        w[0] = -1.0;
        assert_eq!(Boden::aus_werten(w), None);
        w[0] = f64::NAN;
        assert_eq!(Boden::aus_werten(w), None);
    }
}

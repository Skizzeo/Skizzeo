//! Erdmengen über das Geländenetz (G2, plan-gelaendemodul.md §9.1): statt
//! einer Ebene in mittlerer Geländehöhe die Integrale über die Dreiecke des
//! Aufmaßes, Abtrag und Auftrag punktweise getrennt. Auflagen der
//! VOB-Prüfung (vob/pruefung-gelaende-g2.md): A1 nie ein Saldo, A2 der
//! Ansatz nennt das Verfahren, A3 die Marke [`AUS_AUFMASS`] nur an der
//! Böschung, A4 Hinweis bei Gelände steiler als 1:10.
//!
//! N = Gelände − Oberboden; Baugrube = Auskofferung der Sandplatte, Platte
//! plus Überstand (Sohle), innen = innerhalb der Frostschürze, Ring =
//! Schürze. Alles in mm.

use super::{zahl, Boden, AUS_AUFMASS, BACKFILL, DISPOSAL, EXCAVATION, SAND, SENKRECHT_BIS, SLOPE};
use super::{ErdBasis, ErdZone, Sandplatte, GRABEN_AB, TRENCH, TRENCH_SAND, UEBERSTAND};
use crate::{Model, RunId, SurveyFrame, TerrainMesh};
use sk_gelaende::volumen::Erdmenge;
use sk_math::Vec3;
use std::sync::Arc;

/// Der Ansatz mit Geländemodell nennt das Verfahren (Auflage A2); das LV
/// setzt daran die Vorbemerkung zum Ermittlungsverfahren.
pub const DREIECKSPRISMEN: &str = "aus Geländemodell, Dreiecksprismen (REB-VB 22.013)";

/// Verfahren und Herkunft im Ansatz: „; aus Geländemodell, Dreiecksprismen
/// (REB-VB 22.013) aus dem Höhenaufmaß x.csv vom 12.10.2026“. Die
/// Dreieckszahl steht nicht je Zeile, sondern einmal in der Vorbemerkung
/// des LV ([`netz_herkunft`]; VOB-Spruch zu A2 vom 15.10.).
pub(crate) fn verfahren(t: &crate::Terrain) -> String {
    if t.assumed() {
        return format!("; {DREIECKSPRISMEN} {ANGENOMMEN}");
    }
    let hand = von_hand(t);
    match datum(&t.read) {
        Some(d) => format!(
            "; {DREIECKSPRISMEN} aus dem Höhenaufmaß {} vom {d}{hand}",
            t.src
        ),
        None => format!("; {DREIECKSPRISMEN} aus dem Höhenaufmaß {}{hand}", t.src),
    }
}

/// Ein Aufmaß mit eigenen Höhenpunkten (I4) sagt, wie viele davon von
/// Hand sind (VOB-Hinweis a zu #229), mit geänderten Messpunkten (I5) auch
/// diese: „, davon 3 Punkte von Hand ergänzt, 1 Messpunkt geändert“; jeder
/// Teil nur, wenn er nicht null ist (VOB-Spruch zu I5), ohne beide leer.
fn von_hand(t: &crate::Terrain) -> String {
    let neu = t.points.iter().filter(|p| p.hand && !p.off).count();
    let geaendert = t.points.iter().filter(|p| messpunkt_geaendert(p)).count();
    let mut teile = Vec::new();
    match neu {
        0 => {}
        1 => teile.push("1 Punkt von Hand ergänzt".to_string()),
        n => teile.push(format!("{} Punkte von Hand ergänzt", tausend(n))),
    }
    match geaendert {
        0 => {}
        1 => teile.push("1 Messpunkt geändert".to_string()),
        n => teile.push(format!("{} Messpunkte geändert", tausend(n))),
    }
    if teile.is_empty() {
        return String::new();
    }
    format!(", davon {}", teile.join(", "))
}

/// Ein Messpunkt, den der Nutzer verschoben hat und der im Gelände ist
/// (I5). Geändert und ausgeschaltet zählt nur als „nicht verwendet“
/// (VOB-Spruch zu I5).
fn messpunkt_geaendert(p: &crate::TerrainPoint) -> bool {
    !p.hand && !p.off && p.gemessen.is_some()
}

/// „12 von 24,342 auf 24,100 m“, mit Versatz in der Lage über 0,5 mm „,
/// Lage um 0,35 m versetzt“ (VOB-Spruch zu I5: alte und neue Höhe, damit
/// der Prüfer nachrechnen kann; in Worten statt Pfeil).
fn aenderung(p: &crate::TerrainPoint) -> String {
    let Some(&[e, n, h]) = p.gemessen.as_deref() else {
        return p.nr.to_string();
    };
    let mut out = p.nr.to_string();
    if (p.h - h).abs() >= 5e-4 {
        out += &format!(" von {} auf {} m", komma(h, 3), komma(p.h, 3));
    }
    let lage = (p.e - e).hypot(p.n - n);
    if lage >= 5e-4 {
        out += &format!(", Lage um {} versetzt", versatz(lage));
    }
    out
}

/// Lageversatz in m: unter 1 cm in ganzen mm („3 mm“), sonst auf cm
/// („0,35 m“) (VOB H-237a).
pub fn versatz(m: f64) -> String {
    if m < 0.0095 {
        format!("{} mm", (m * 1000.0).round())
    } else {
        format!("{} m", komma(m, 2))
    }
}

/// Zahl mit Komma und `stellen` Nachkommastellen.
fn komma(v: f64, stellen: usize) -> String {
    format!("{:.*}", stellen, v + 0.0).replace('.', ",")
}

/// Herkunft eines Geländes nur aus Punkten von Hand (I1): Es ist nicht
/// vermessen, sondern angenommen, zuerst eben in Höhe OK Gelände.
pub const ANGENOMMEN: &str = "aus dem angenommenen Gelände, nicht vermessen";

/// Dreieckszahl des angenommenen Geländes (I1), „1.834“; `None` ohne
/// Gelände oder mit vermessenen Punkten. Das LV setzt dann seine eigene
/// Vorbemerkung (VOB-Spruch zu I1).
pub fn netz_angenommen(m: &Model) -> Option<String> {
    let t = m.terrain().filter(|t| t.assumed())?;
    Some(tausend(t.mesh().netz.dreiecke.len()))
}

/// Herkunft des Netzes für die Vorbemerkung des LV: „Höhenaufmaß x.csv vom
/// 12.10.2026, 1.834 Dreiecke“, damit der Prüfer des AG sieht, dass er
/// dasselbe Netz nachrechnet (VOB/B § 14 (1)). `None` ohne Aufmaß und beim
/// angenommenen Gelände ([`netz_angenommen`]).
pub fn netz_herkunft(m: &Model) -> Option<String> {
    let t = m.terrain().filter(|t| !t.assumed())?;
    let n = t.mesh().netz.dreiecke.len();
    let vom = datum(&t.read).map_or(String::new(), |d| format!(" vom {d}"));
    Some(format!(
        "Höhenaufmaß {}{vom}, {} {}{}{}{}",
        t.src,
        tausend(n),
        if n == 1 { "Dreieck" } else { "Dreiecke" },
        klammer(t, |p| p.hand && !p.off, "von Hand ergänzt"),
        geaenderte(t),
        klammer(t, nicht_verwendet, "nicht verwendet"),
    ))
}

/// „, davon 2 Messpunkte geändert (12 von 24,342 auf 24,100 m; 15 von
/// 24,518 auf 24,600 m)“, immer alle (wie VOB H-234a); leer ohne solche.
fn geaenderte(t: &crate::Terrain) -> String {
    let teile: Vec<String> = t
        .points
        .iter()
        .filter(|p| messpunkt_geaendert(p))
        .map(aenderung)
        .collect();
    let punkte = match teile.len() {
        0 => return String::new(),
        1 => "1 Messpunkt".to_string(),
        n => format!("{} Messpunkte", tausend(n)),
    };
    format!(", davon {punkte} geändert ({})", teile.join("; "))
}

/// Ein Messpunkt, den der Nutzer aus dem Gelände genommen hat. Ein eigener
/// Punkt, der in einer Bruchkante bleibt und darum nur `off` wird, ist kein
/// Messpunkt und zählt nicht (VOB H-234b).
fn nicht_verwendet(p: &crate::TerrainPoint) -> bool {
    p.off && !p.hand
}

/// „, davon 3 Punkte von Hand ergänzt (H1, H2, H3)“, leer ohne solche.
/// Immer alle Nummern: Solange es keine Ausgabe der Punktliste gibt, darf
/// die Vorbemerkung nicht auf sie verweisen (VOB H-234a, VOB/A § 7 (1)).
fn klammer(t: &crate::Terrain, welche: impl Fn(&crate::TerrainPoint) -> bool, was: &str) -> String {
    let nr: Vec<&str> = t
        .points
        .iter()
        .filter(|p| welche(p))
        .map(|p| p.nr.as_str())
        .collect();
    let punkte = match nr.len() {
        0 => return String::new(),
        1 => "1 Punkt".to_string(),
        n => format!("{} Punkte", tausend(n)),
    };
    format!(", davon {punkte} {was} ({})", nr.join(", "))
}

/// Von Hand geänderte Punkte eines vermessenen Aufmaßes: (ergänzt,
/// geändert, nicht verwendet), in der Reihenfolge der Vorbemerkung
/// (VOB-Spruch zu I5); `None` ohne Aufmaß, beim angenommenen Gelände oder
/// wenn keiner geändert ist. Kostenblatt und LV geben dazu den Befund
/// [`HAND_GEAENDERT`] (VOB-Spruch zu I4).
///
/// „Nicht verwendet“ zählt jeden Messpunkt mit `off`, auch einen, der schon so in
/// der Datei steht (`off=1`): `off` setzt nur der Nutzer (Entf oder von Hand
/// in der Datei), nie das Einlesen; Punkte mit einem Code „nicht ins
/// Gelände“ haben kein `off` und zählen nicht. Die Mengen lassen den
/// gemessenen Punkt weg, also muss der AG das wissen, egal wann und wie er
/// ausgeschaltet wurde.
pub fn netz_geaendert(m: &Model) -> Option<(usize, usize, usize)> {
    let t = m.terrain().filter(|t| !t.assumed())?;
    let neu = t.points.iter().filter(|p| p.hand && !p.off).count();
    let geaendert = t.points.iter().filter(|p| messpunkt_geaendert(p)).count();
    let weg = t.points.iter().filter(|p| nicht_verwendet(p)).count();
    (neu + geaendert + weg > 0).then_some((neu, geaendert, weg))
}

/// Befund zu [`netz_geaendert`]: kein Sperren.
pub fn hand_geaendert((neu, geaendert, weg): (usize, usize, usize)) -> String {
    format!(
        "Höhenaufmaß von Hand geändert ({neu} ergänzt, {geaendert} geändert, {weg} nicht \
         verwendet): vor der Ausschreibung vom Vermesser bestätigen lassen."
    )
}

/// Anfang des Befunds [`hand_geaendert`], zum Erkennen.
pub const HAND_GEAENDERT: &str = "Höhenaufmaß von Hand geändert";

/// Datum des Einlesens deutsch („2026-10-12“ → „12.10.2026“); `None` ohne,
/// unverändert, wenn es keines in dieser Form ist.
fn datum(iso: &str) -> Option<String> {
    let s = iso.trim();
    if s.is_empty() {
        return None;
    }
    let t: Vec<&str> = s.split('-').collect();
    Some(match t.as_slice() {
        [j, m, d] if j.len() == 4 && m.len() == 2 && d.len() == 2 => format!("{d}.{m}.{j}"),
        _ => s.to_string(),
    })
}

/// Ganze Zahl mit Tausenderpunkt.
fn tausend(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

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
    /// Streifen der Schürze unter einer Haustrennwand und OK Gelände der
    /// eigenen Gründung (mm über ±0,00): Dort zählt für den Graben die
    /// eigene Höhe, nicht das Netz im Fugenstreifen zum Nachbarn
    /// (VOB-Spruch zu H-245c, Weg b).
    trennwand: Vec<Vec<Vec3>>,
    ok: f64,
}

impl PartialEq for ErdNetz {
    fn eq(&self, o: &ErdNetz) -> bool {
        Arc::ptr_eq(&self.mesh, &o.mesh)
            && (self.frame, self.h0, &self.aufmass, &self.innen)
                == (o.frame, o.h0, &o.aufmass, &o.innen)
            && (&self.trennwand, self.ok) == (&o.trennwand, o.ok)
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
        let trennwand: Vec<Vec<Vec3>> = m
            .trennwand_streifen(run)
            .into_iter()
            .map(|q| {
                q.into_iter()
                    .map(|p| sk_math::vec3(p[0], p[1], 0.0))
                    .collect()
            })
            .collect();
        let ok = if trennwand.is_empty() {
            0.0
        } else {
            m.terrain_z_of_level(m.foundation_level_of(m.run(run)?.storey)?)
        };
        Some(ErdNetz {
            frame: m.survey_frame()?,
            h0: m.terrain_nhn()?,
            mesh: t.mesh(),
            aufmass: verfahren(t),
            innen,
            trennwand,
            ok,
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

    /// Gelände über `umriss` gegen die Höhen `zs` über ±0,00: Fläche im
    /// Netz (mm²), darüber und darunter (mm³), größtes Gefälle; der Umriss
    /// einmal geschnitten (Review #164 H-164b).
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

/// Keil vor einem Rand (Auflage A3, Empfehlung der VOB-Prüfung Frage 3):
/// je Randstück mit seiner örtlichen Höhe h(s) aus der Geländehöhe, nur wo
/// h über `schwelle` liegt. Der Keil ist ∮ k·h²/2 ds plus je Ecke
/// tan(φ/2)·k²·h³/3 mit k = cot β, die schräge Fläche (∮ h ds + Σ
/// tan(φ/2)·k·h²)/sin β. Angenähert bleibt, dass das Gelände über die
/// Breite des Keils weiter steigt oder fällt (G5).
#[derive(Default)]
struct Keil {
    voll: f64,
    /// Der Teil bis zur Höhe `deckel` über dem Rand.
    unter: f64,
    flaeche: f64,
    /// Länge des Rands mit Keil, größte Höhe dort.
    lang: f64,
    tief: f64,
}

/// ∫ min(h, d)² ds über eine Strecke der Länge `l`, auf der h linear von
/// `h0` nach `h1` läuft.
fn quadrat(h0: f64, h1: f64, l: f64, d: f64) -> f64 {
    let voll = |a: f64, b: f64, l: f64| l * (a * a + a * b + b * b) / 3.0;
    if h0 <= d && h1 <= d {
        return voll(h0, h1, l);
    }
    if h0 >= d && h1 >= d {
        return l * d * d;
    }
    let t = (d - h0) / (h1 - h0);
    if h0 < d {
        voll(h0, d, l * t) + l * (1.0 - t) * d * d
    } else {
        l * t * d * d + voll(d, h1, l * (1.0 - t))
    }
}

/// Keil vor dem Ring `rand` (gegen den Uhrzeigersinn, nach außen); `h`
/// macht aus der Geländehöhe über ±0,00 die Höhe des Keils am Rand.
fn keil(
    n: &ErdNetz,
    rand: &[Vec3],
    h: &dyn Fn(f64) -> f64,
    schwelle: f64,
    beta: f64,
    deckel: f64,
) -> Keil {
    let mut b = Keil::default();
    let beta = beta.clamp(1.0, 89.0).to_radians();
    let (k, sin) = (1.0 / beta.tan(), beta.sin());
    let m = rand.len();
    for i in 0..m {
        let p = n.profil(rand[i], rand[(i + 1) % m]);
        for w in p.windows(2) {
            let (mut s0, mut h0) = (w[0][0], h(w[0][1]));
            let (mut s1, mut h1) = (w[1][0], h(w[1][1]));
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
            b.voll += k * quadrat(h0, h1, l, f64::INFINITY) / 2.0;
            b.unter += k * quadrat(h0, h1, l, deckel) / 2.0;
            b.flaeche += l * (h0 + h1) / 2.0 / sin;
            b.lang += l;
            b.tief = b.tief.max(h0).max(h1);
        }
    }
    for (p, tan) in rand.iter().zip(ecken_tan(rand)) {
        let Some(h) = n.hoehe(*p).map(h) else {
            continue;
        };
        if h > schwelle {
            b.voll += tan * k * k * h * h * h / 3.0;
            b.unter += tan * k * k * h.min(deckel).powi(3) / 3.0;
            b.flaeche += tan * k * h * h / sin;
        }
    }
    b
}

/// Höhen der Gründung unter der Platte: UK Dämmung (ohne UK Platte), UK
/// Schicht und die Sandplatte, mit dem Gelände in mittlerer Höhe.
fn lage(b: &ErdBasis, boden: &Boden) -> (f64, f64, Sandplatte) {
    let z_b = b.insulation_bottom_z.min(b.slab_bottom_z);
    let z_p = z_b - boden.schicht();
    let n = b.terrain_z - boden.oberboden.max(0.0);
    (z_b, z_p, Sandplatte::neu(b.terrain_z, n, z_b, z_p, boden))
}

/// So hoch über dem gewachsenen Boden reicht der Füllsand um die Platte
/// höchstens: bis unter den angedeckten Oberboden.
fn deckel(b: &ErdBasis, boden: &Boden) -> f64 {
    let o = boden.oberboden.max(0.0);
    o - b.andecken.clamp(0.0, o)
}

/// Sohle der Auskofferung (Platte um den Überstand versetzt), ihre Fläche
/// und die Mengen über ihr gegen die Sohle, UK Dämmung und [`deckel`]
/// darunter; `None`, wenn das
/// Netz sie nicht ganz deckt oder der Überstand sich nicht versetzen lässt.
fn baugrube(b: &ErdBasis, n: &ErdNetz, boden: &Boden) -> Option<(Vec<Vec3>, f64, [Erdmenge; 3])> {
    let o = boden.oberboden.max(0.0);
    let (z_b, _, sp) = lage(b, boden);
    let sohle = if sp.w > 0.0 {
        sk_math::flaeche::versatz(&b.outline, &vec![-sp.w; b.outline.len()])?
    } else {
        b.outline.clone()
    };
    let sohle_fl = sk_math::polygon::area(&sohle);
    let e = n.mengen(&sohle, &[sp.ka + o, z_b + o, z_b - deckel(b, boden) + o]);
    if sohle_fl <= 0.0 || e[0].flaeche < sohle_fl * (1.0 - 1e-6) {
        return None;
    }
    Some((sohle, sohle_fl, e))
}

/// Das Netz `n` deckt die Baugrube; sonst rechnen die Erdmengen mit der
/// mittleren Höhe (Bedienbarkeit 42.5).
pub(super) fn deckt(b: &ErdBasis, n: &ErdNetz, boden: &Boden) -> bool {
    baugrube(b, n, boden).is_some()
}

/// Erdmengen der Gründung über das Netz `n`, dazu Baufeld und Baugrube,
/// mit der Sandplatte wie in der Ebene ([`super::erd_mengen`]): Je Stelle
/// (N gewachsener Boden) wird bis zur Sohle ausgekoffert, (N − Sohle)⁺;
/// Füllsand kommt bis UK Dämmung bzw. UK Schicht, unter der Sohle
/// zusätzlich (Sohle − N)⁺, wo das Gelände tiefer liegt; die Gräben gehen
/// ab OK Sandplatte, im gewachsenen Boden (min(N, Sohle) − UK Schürze)⁺.
/// `None`, wenn das Netz die Baugrube nicht ganz deckt oder der Überstand
/// sich nicht versetzen lässt; dann rechnet [`super::erd_mengen`] mit der
/// mittleren Höhe.
pub(super) fn mengen(
    b: &ErdBasis,
    n: &ErdNetz,
    boden: &Boden,
    menge: &mut dyn FnMut(&'static str, f64, String),
) -> Option<ErdZone> {
    let o = boden.oberboden.max(0.0);
    let g = boden.schicht();
    let fl = b.slab_area;
    let z_s = b.slab_bottom_z;
    let z_f = b.footing_bottom_z;
    let (z_b, z_p, sp) = lage(b, boden);
    let ka = sp.ka;
    let (sohle, sohle_fl, [grube, grube_b, grube_c]) = baugrube(b, n, boden)?;
    let c = deckel(b, boden);
    let [platte, platte_b, platte_f, platte_c] =
        n.mengen(&b.outline, &[ka + o, z_b + o, z_f + o, z_b - c + o]);
    let (innen_fl, [innen, innen_f]) = if n.innen.len() >= 3 {
        let e = n.mengen(&n.innen, &[ka + o, z_f + o]);
        (sk_math::polygon::area(&n.innen), e)
    } else {
        (0.0, [Erdmenge::default(); 2])
    };
    let ring_fl = (fl - innen_fl).max(0.0);
    // über dem gewachsenen Boden bis zum Überstand, darüber hinaus der
    // Keil; je Stelle (min(UK Dämmung, N + c) − N)⁺ = (UK Dämmung − N)⁺ −
    // (UK Dämmung − c − N)⁺, im Keil ebenso
    let ueberstand = if (sp.w - UEBERSTAND).abs() < 1e-9 {
        None
    } else {
        sk_math::flaeche::versatz(&b.outline, &vec![-UEBERSTAND; b.outline.len()])
    };
    let auf_innen = match &ueberstand {
        Some(r) => {
            let [b_, c_] = n.mengen(r, &[z_b + o, z_b - c + o]);
            b_.unter - c_.unter
        }
        None => grube_b.unter - grube_c.unter,
    };
    let beta = boden.boeschung;
    let rand = ueberstand.as_deref().unwrap_or(&sohle);
    let auf_keil = keil(n, rand, &|gl| z_b - (gl - o), 0.0, beta, f64::INFINITY);
    let auf_ueber = keil(n, rand, &|gl| z_b - c - (gl - o), 0.0, beta, f64::INFINITY);
    let auf_keil_v = (auf_keil.voll - auf_ueber.voll).max(0.0);
    // Auskofferung: Keil ab der Sohle, wo das Gelände mehr als 1,25 m über
    // ihr liegt; bis UK Dämmung Füllsand, darüber Verfüllung
    let bo = keil(
        n,
        &sohle,
        &|gl| gl - o - ka,
        SENKRECHT_BIS - o,
        beta,
        z_b - ka,
    );
    // Graben im gewachsenen Boden: ∫ring (min(N, Sohle) − UK Schürze)⁺
    let mut graben = if ka - z_f > GRABEN_AB {
        (platte_f.ueber - platte.ueber) - (innen_f.ueber - innen.ueber)
    } else {
        0.0
    };
    // unter der Haustrennwand mit der eigenen Höhe statt dem Netz
    let h = n.ok - o;
    for q in &n.trennwand {
        if ka - z_f <= GRABEN_AB {
            break;
        }
        let [s, f] = n.mengen(q, &[ka + o, z_f + o]).map(|e| e.ueber);
        let eigen = (h - z_f).max(0.0) - (h - ka).max(0.0);
        graben += sk_math::polygon::area(q) * eigen - (f - s);
    }
    let graben = graben.max(0.0);
    let im_sand = (ring_fl * (z_b - z_f) - graben).max(0.0);
    let verfahren = n.aufmass.clone();
    let breite = |x: f64| format!("{} m", zahl(x, 1e3));
    let bis = if z_b < z_s {
        "UK Dämmung"
    } else {
        "UK Platte"
    };

    // Oberboden über das Baufeld, mindestens bis zum Rand der Auskofferung
    // und der Sandplatte
    let tan = sp.tan;
    let x_ob = boden
        .rand
        .max(sp.w + bo.tief / tan)
        .max(UEBERSTAND + auf_keil.tief / tan);
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
        // VOB B-BA-1: der Oberboden steht gesondert, hier nur der Boden
        menge(
            EXCAVATION,
            grube.ueber,
            steil_dazu(format!(
                "Auskofferung {} ab OK Gelände im Mittel = Oberboden {} (gesondert) + Boden: Sohle \
                 {} m² (Platte + Überstand {}), im Mittel {}{verfahren}",
                breite(b.terrain_z - ka),
                breite(o),
                zahl(sohle_fl, 1e6),
                breite(sp.w),
                mittel(grube.ueber, sohle_fl)
            )),
        );
    }
    if bo.voll > 0.0 {
        menge(
            EXCAVATION,
            bo.voll,
            format!(
                "Böschung {}° auf {} Sohlrand, Tiefe bis {}{AUS_AUFMASS}, Böschung am Gelände \
                 angenähert",
                beta.round(),
                breite(bo.lang),
                breite(bo.tief)
            ),
        );
    }
    // Füllsand (VOB B-BA-2)
    let kern = innen_fl * (z_p - ka).max(0.0) + innen.unter;
    if kern > 0.0 {
        menge(
            SAND,
            kern,
            format!(
                "innerhalb der Schürze {} m² bis {}, im Mittel {}{verfahren}",
                zahl(innen_fl, 1e6),
                super::aufgefuellt_bis(z_b < z_s, boden.kies),
                mittel(kern, innen_fl)
            ),
        );
    }
    let unter = ring_fl * (z_b - ka).max(0.0) + (platte.unter - innen.unter).max(0.0);
    if unter > 0.0 {
        menge(
            SAND,
            unter,
            format!(
                "unter der Schürze {} m² bis {bis}, im Mittel {}{verfahren}",
                zahl(ring_fl, 1e6),
                mittel(unter, ring_fl)
            ),
        );
    }
    let um_fl = sohle_fl - fl;
    let um = ((grube.ueber - grube_b.ueber) - (platte.ueber - platte_b.ueber)).max(0.0);
    if um > 0.0 {
        menge(
            SAND,
            um,
            format!(
                "um die Platte {} m² (Überstand {}) bis {bis}, im Mittel {}{verfahren}",
                zahl(um_fl, 1e6),
                breite(sp.w),
                mittel(um, um_fl)
            ),
        );
    }
    if bo.unter > 0.0 {
        menge(
            SAND,
            bo.unter,
            format!(
                "Böschung {}°: Füllsand bis {bis}{AUS_AUFMASS}, Böschung am Gelände angenähert",
                beta.round()
            ),
        );
    }
    let auf = (auf_innen - (platte_b.unter - platte_c.unter)).max(0.0);
    if auf > 0.0 {
        let fl_auf = sk_math::polygon::area(rand) - fl;
        menge(
            SAND,
            auf,
            format!(
                "über dem gewachsenen Boden um die Platte {} m² (Überstand {}) bis {bis}, \
                 höchstens unter den angedeckten Oberboden, im Mittel {}{verfahren}",
                zahl(fl_auf, 1e6),
                breite(UEBERSTAND),
                mittel(auf, fl_auf)
            ),
        );
    }
    if auf_keil_v > 0.0 {
        menge(
            SAND,
            auf_keil_v,
            format!(
                "Böschung {}° der Sandplatte auf {} Rand, Höhe bis {}{AUS_AUFMASS}, Böschung am \
                 Gelände angenähert",
                beta.round(),
                breite(auf_keil.lang),
                breite(auf_keil.tief.min(c))
            ),
        );
    }
    // Gräben der Schürze ab OK Sandplatte (VOB B-BA-3)
    if im_sand > 0.0 {
        menge(
            TRENCH_SAND,
            im_sand,
            steil_dazu(format!(
                "Ring {} m² ab OK Sandplatte bis UK Schürze bzw. Sohle der Auskofferung, im \
                 Mittel {}{verfahren}",
                zahl(ring_fl, 1e6),
                mittel(im_sand, ring_fl)
            )),
        );
    }
    if graben > 0.0 {
        menge(
            TRENCH,
            graben,
            steil_dazu(format!(
                "Ring {} m² unter der Sohle der Auskofferung bis UK Schürze, im Mittel \
                 {}{verfahren}",
                zahl(ring_fl, 1e6),
                mittel(graben, ring_fl)
            )),
        );
    }
    super::planum(innen_fl, g, boden.kies, menge);
    // über der Sandplatte bis OK gewachsener Boden, außerhalb der Platte;
    // im Keil angenähert, die Abfuhr nicht (sie steht in Aushub und
    // Verfüllung gleich)
    let ueber = (grube_b.ueber - platte_b.ueber).max(0.0);
    let verfuellen = ueber + (bo.voll - bo.unter).max(0.0);
    if verfuellen > 0.0 {
        let marke = if bo.voll > 0.0 { AUS_AUFMASS } else { "" };
        menge(
            BACKFILL,
            verfuellen,
            format!(
                "über der Sandplatte: Sohle {} m³ − Platte {} m³ über {bis}{verfahren}{marke}",
                zahl(grube_b.ueber + bo.voll - bo.unter, 1e9),
                zahl(platte_b.ueber, 1e9)
            ),
        );
    }
    let aushub = grube.ueber + bo.voll + graben + im_sand;
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
        b: z_b,
        keil: bo.voll > 0.0,
    })
}

#[cfg(test)]
mod tests {
    /// Lage unter 1 cm in mm, mit Komma hinter der Nummer (VOB H-237a/b).
    #[test]
    fn lage_in_mm_und_cm() {
        assert_eq!(super::versatz(0.003), "3 mm");
        assert_eq!(super::versatz(0.0006), "1 mm");
        assert_eq!(super::versatz(0.35), "0,35 m");
        let p = crate::TerrainPoint {
            nr: "12".into(),
            e: 10.003,
            n: 20.0,
            h: 24.342,
            code: Default::default(),
            off: false,
            hand: false,
            gemessen: Some(Box::new([10.0, 20.0, 24.342])),
        };
        assert_eq!(super::aenderung(&p), "12, Lage um 3 mm versetzt");
        let p = crate::TerrainPoint { h: 24.1, ..p };
        assert_eq!(
            super::aenderung(&p),
            "12 von 24,342 auf 24,100 m, Lage um 3 mm versetzt"
        );
    }

    #[test]
    fn mittel_unter_1_cm() {
        assert_eq!(super::mittel(30e6, 10e6), "unter 1 cm");
        assert_eq!(super::mittel(50e6, 10e6), "0,01 m");
        assert_eq!(super::mittel(1.2e9, 1e6), "1,20 m");
        assert_eq!(super::mittel(0.0, 1e6), "0,00 m");
        assert_eq!(super::mittel(1.0, 0.0), "0,00 m");
    }
}

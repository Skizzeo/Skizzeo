//! Sonnenstands-System (Sonnenstand S4, Analyse §3.1, §5, §8 09:25): Die
//! Kachel unter „Projektdaten“ und ein Klick auf den Nordpfeil schalten es
//! an und aus. In 3D stehen dann über dem Gebäude die Tagesbahn des
//! eingestellten Tags mit Marken zu jeder vollen Stunde, fein die Bahnen
//! vom 21.06. und 21.12. und die Sonnenscheibe. Ziehen an der Sonne ändert
//! nur die Uhrzeit: Sie bleibt auf der Tagesbahn und hält bei Auf- und
//! Untergang. Oben in der Ansicht steht eine kleine Leiste mit Datum,
//! Uhrzeit (MEZ bzw. MESZ) und Schnellwahl. Ohne Wand zeigt ein Würfel von
//! 10 m die Sonne (nur Anzeige). Das Licht in 3D kommt von der Sonne.
//! Am Boden sitzt ein Griff an der Schattenspitze (S6): Ziehen an ihm
//! führt ihn entlang der Tageskurve der Spitze, die Sonne folgt.
//!
//! Datum, Uhrzeit und Schalter stehen im Modell als Ansichtszustand
//! ([`Sun`], `[sun]`), ohne Rückgängig-Schritt. Die Entscheidungen stehen
//! als reine Funktionen oben; die Sonnenrechnung selbst ist
//! [`sk_math::sonne`].

use crate::camera::Camera;
use sk_math::sonne::{self, Datum, Lage, Sonnenstand, Zeitpunkt};
use sk_math::{vec3, Vec3};
#[cfg(test)]
use sk_model::edge_kind;
use sk_model::{Location, Sun};
use sk_paint::Canvas;
#[cfg(test)]
use sk_paint::{Path, Rgba};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::schatten;
#[cfg(test)]
use sk_render::MeshData;
use sk_render::{Helper, SunLight, SOLID};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};

/// Kantenlänge des Würfels (mm): seit S9 nur noch Testkörper, die App
/// zeigt ohne Gebäude keine Sonne.
#[cfg(test)]
pub const WUERFEL: f64 = 10_000.0;
/// Hinweis an Kachel und Pfeil, wenn die Sonne ohne Gebäude an soll (S9).
pub const OHNE_HAUS: &str = "Sonne und Schatten erscheinen, sobald das Gebäude gezeichnet ist.";
/// Kachel oder Klick auf den Pfeil (S9): der neue Schalter; ohne Gebäude
/// geht die Sonne nicht an, sondern es kommt der Hinweis [`OHNE_HAUS`].
pub fn umschalten(an: bool, haus: bool) -> Result<bool, &'static str> {
    if !an && !haus {
        Err(OHNE_HAUS)
    } else {
        Ok(!an)
    }
}

/// Statt der Leiste in 3D, wenn die Sonne an ist, das Gebäude aber fehlt
/// (S9).
pub const KEIN_HAUS: &str = "Kein Gebäude – Sonne erscheint mit dem Gebäude";
/// Schnellwahl der Leiste: Monat und Tag.
pub const SCHNELL: [(u32, u32); 4] = [(3, 21), (6, 21), (9, 23), (12, 21)];
/// Hinweis der Leiste, wenn die Sonne nicht über dem Horizont steht.
pub const UNTER: &str = "Sonne unter dem Horizont";
/// Bahnpunkte alle fünf Minuten (s).
const SCHRITT: i64 = 300;
/// Kleinster Halbmesser der Himmelskuppel (mm).
const KUPPEL_MIN: f64 = 15_000.0;
/// Halbmesser der Kuppel in Vielfachen des halben Hüllquaders.
const KUPPEL_K: f64 = 1.5;
/// Sonnenscheibe, ihr dunklerer Rand, Bahnen und Stundenmarken (dip).
const SCHEIBE_PX: f32 = 22.0;
const RAND_PX: f32 = 2.5;
const BAHN_PX: f32 = 2.0;
const GRENZ_PX: f32 = 1.0;
const MARKE_PX: f32 = 6.0;
/// Greifabstand um die Scheibe (dip).
const GREIF_PX: f64 = 6.0;
/// Erst ab dieser Strecke (dip) gilt ein Druck auf die Scheibe als Ziehen
/// (wie ein Klick in der Auswahl).
const ZUG_PX: f64 = 4.0;
/// Griff an der Schattenspitze und die Kurve der Spitze (dip).
const GRIFF_PX: f32 = 14.0;
const KURVE_PX: f32 = 1.5;
/// So weit (dip) über ein Ende der Schattenkurve hinaus gezogen gilt
/// Sonnenauf- bzw. -untergang.
const ENDE_PX: f64 = 24.0;

const GELB: [f32; 4] = [1.0, 0.80, 0.16, 1.0];
const RAND: [f32; 4] = [0.70, 0.43, 0.0, 1.0];
const BAHN: [f32; 4] = [0.90, 0.58, 0.08, 0.95];
const GRENZ: [f32; 4] = [0.90, 0.58, 0.08, 0.5];
const GRIFF: [f32; 4] = [0.20, 0.20, 0.22, 1.0];

// ===== Reine Funktionen =====

/// Der Zeitpunkt zu Datum und gesetzlicher Uhrzeit.
pub fn zeitpunkt(s: &Sun) -> Zeitpunkt {
    Zeitpunkt::ortszeit(s.date, s.minutes / 60, s.minutes % 60)
}

/// „MEZ“ oder „MESZ“ zu Datum und Uhrzeit.
pub fn zone(s: &Sun) -> &'static str {
    if zeitpunkt(s).ist_sommerzeit() {
        "MESZ"
    } else {
        "MEZ"
    }
}

/// Der Bauort: Breite und Länge aus den Projektdaten, sonst Ganderkesee.
pub fn bauort(l: &Location) -> Lage {
    l.lage(Lage::GANDERKESEE)
}

/// Zeitpunkt aus der Uhr des Rechners.
pub fn uhr() -> Zeitpunkt {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    Zeitpunkt(s)
}

/// „Jetzt“: Datum und Uhrzeit zum Zeitpunkt `t` (auf die Minute).
pub fn jetzt(t: Zeitpunkt, on: bool) -> Sun {
    let o = t.in_ortszeit();
    Sun {
        date: o.datum,
        minutes: o.minuten,
        on,
    }
}

/// Datum und Uhrzeit so, wie gerechnet wird: eine Uhrzeit in der beim
/// Umstellen auf MESZ übersprungenen Stunde (29.03., 02:00 bis 02:59)
/// steht danach eine Stunde später da (Hinweis Y).
pub fn gueltig(s: Sun) -> Sun {
    jetzt(zeitpunkt(&s), s.on)
}

/// Stand beim ersten Einschalten in einer Datei: heute um 12:00.
pub fn anfang(t: Zeitpunkt) -> Sun {
    Sun {
        minutes: 12 * 60,
        ..jetzt(t, true)
    }
}

/// Schnellwahl `i` (21.03., 21.06., 23.09., 21.12.) im Jahr des Datums; die
/// Uhrzeit bleibt.
pub fn schnell(s: Sun, i: usize) -> Sun {
    let (m, d) = SCHNELL[i.min(SCHNELL.len() - 1)];
    Sun {
        date: Datum::new(s.date.jahr, m, d).unwrap_or(s.date),
        ..s
    }
}

/// Datum in der Leiste: „21.06.2026“.
pub fn datum_text(d: Datum) -> String {
    format!("{:02}.{:02}.{}", d.tag, d.monat, d.jahr)
}

/// Uhrzeit in der Leiste: „12:00“.
pub fn zeit_text(minuten: u32) -> String {
    format!("{:02}:{:02}", minuten / 60, minuten % 60)
}

/// Getipptes Datum: „21.6.“, „21.06.2026“, „21.6.26“, „2106“, „21062026“;
/// ohne Jahr gilt `jahr`. `None`, wenn es den Tag nicht gibt.
pub fn datum_lesen(t: &str, jahr: i32) -> Option<Datum> {
    let t = t.trim();
    let teile: Vec<&str> = t.split(['.', ',']).filter(|s| !s.is_empty()).collect();
    let zahl = |s: &str| {
        (!s.is_empty() && s.len() <= 4 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (tag, monat, j) = match teile.as_slice() {
        [z] if z.len() == 4 || z.len() == 6 || z.len() == 8 => {
            let (a, b, c) = (zahl(&z[..2])?, zahl(&z[2..4])?, &z[4..]);
            (a, b, (!c.is_empty()).then_some(c))
        }
        [a, b] => (zahl(a)?, zahl(b)?, None),
        [a, b, c] => (zahl(a)?, zahl(b)?, Some(*c)),
        _ => return None,
    };
    let jahr = match j {
        None => jahr,
        Some(c) if c.len() == 2 => 2000 + zahl(c)? as i32,
        Some(c) if c.len() == 4 => zahl(c)? as i32,
        _ => return None,
    };
    (jahr > 0).then(|| Datum::new(jahr, monat, tag)).flatten()
}

/// Getippte Uhrzeit: „12“, „9.30“, „12,05“, „12:00“, „930“, „1230“.
pub fn zeit_lesen(t: &str) -> Option<u32> {
    let t = t.trim();
    let zahl = |s: &str| {
        (!s.is_empty() && s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
    };
    let (h, m) = match t.split_once([':', '.', ',']) {
        Some((h, m)) => (zahl(h)?, if m.is_empty() { 0 } else { zahl(m)? }),
        None if t.len() >= 3 && t.len() <= 4 => {
            let (h, m) = t.split_at(t.len() - 2);
            (zahl(h)?, zahl(m)?)
        }
        None => (zahl(t)?, 0),
    };
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Ob die Sonne über dem Horizont steht (ihr oberer Rand, wie bei Auf- und
/// Untergang).
pub fn ueber(st: &Sonnenstand) -> bool {
    st.hoehe_geometrisch > -0.833
}

/// Lichtrichtung in 3D (zur Sonne, Modellkoordinaten): `None`, wenn sie
/// nicht über dem Horizont steht; dann gilt die feste Richtung. Unter
/// [`schatten::MIN_HOEHE`] streift das Licht mit dieser Höhe, darüber geht
/// es genau zur Sonne, wie der Schatten (S5).
pub fn licht(l: &Location, s: &Sun) -> Option<[f32; 3]> {
    let d = zur_sonne(l, s)?;
    let min = schatten::min_sinus();
    if d.z >= min {
        return Some(d.to_f32());
    }
    // Am Horizont etwas angehoben bleiben die Dächer hell
    let waag = vec3(d.x, d.y, 0.0).normalized() * (1.0 - min * min).sqrt();
    Some((waag + vec3(0.0, 0.0, min)).to_f32())
}

/// Richtung zur Sonne (Modell, Einheitsvektor), solange sie über dem
/// Horizont steht.
pub fn zur_sonne(l: &Location, s: &Sun) -> Option<Vec3> {
    let st = sonne::sonnenstand(bauort(l), zeitpunkt(s));
    ueber(&st).then(|| st.richtung_modell(l.north_deg()))
}

/// Umgebungsanteil der Flächen bei Sonne (S5): die Schattenseite und der
/// Schatten liegen 40 % unter der Sonnenseite.
pub const UMGEBUNG: f32 = 0.6;

/// Sonne für den Renderer: Richtung und Umgebungsanteil, solange sie über
/// dem Horizont steht; Schatten wirft sie ab [`schatten::MIN_HOEHE`].
pub fn sonnenlicht(l: &Location, s: &Sun) -> Option<SunLight> {
    let d = zur_sonne(l, s)?;
    Some(SunLight {
        zur_sonne: [d.x, d.y, d.z],
        ambient: UMGEBUNG,
    })
}

/// Hüllquader des Würfels: Ecke im Ursprung, nach +x, +y und oben.
#[cfg(test)]
pub fn wuerfel_quader() -> (Vec3, Vec3) {
    (vec3(0.0, 0.0, 0.0), vec3(WUERFEL, WUERFEL, WUERFEL))
}

/// Himmelskuppel um das Gebäude (bzw. den Würfel): Mitte am Boden und
/// Halbmesser (mm), das 1,5-fache des Abstands zur fernsten Ecke, die
/// ganze Höhe über (oder unter) dem Boden gerechnet (Befund C, §8 10:20).
pub fn kuppel((lo, hi): (Vec3, Vec3)) -> (Vec3, f64) {
    let mitte = vec3((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, 0.0);
    let (dx, dy) = ((hi.x - lo.x) * 0.5, (hi.y - lo.y) * 0.5);
    let dz = hi.z.abs().max(lo.z.abs());
    let ecke = (dx * dx + dy * dy + dz * dz).sqrt();
    (mitte, (KUPPEL_K * ecke).max(KUPPEL_MIN))
}

/// Punkt der Kuppel in Richtung des Sonnenstands; knapp unter dem
/// Horizont (Auf- und Untergang) auf dem Boden.
fn auf_kuppel(st: &Sonnenstand, nord: f64, (m, r): (Vec3, f64)) -> Vec3 {
    let mut d = st.richtung_modell(nord);
    if d.z < 0.0 {
        d.z = 0.0;
        d = d.normalized();
    }
    m + d * r
}

/// Was am Himmel steht: Tagesbahn (Zeitpunkt und Punkt), die Bahnen vom
/// 21.06. und 21.12., die vollen Stunden und die Scheibe.
#[derive(Clone, Debug, PartialEq)]
pub struct Himmel {
    pub tag: Vec<(Zeitpunkt, Vec3)>,
    pub sommer: Vec<Vec3>,
    pub winter: Vec<Vec3>,
    pub stunden: Vec<Vec3>,
    /// `None` unter dem Horizont.
    pub sonne: Option<Vec3>,
}

/// Bahn am Tag `d` über dem Horizont, alle fünf Minuten.
fn bahn(lage: Lage, nord: f64, d: Datum, k: (Vec3, f64)) -> Vec<(Zeitpunkt, Vec3)> {
    sonne::tagesbahn(lage, d, SCHRITT)
        .into_iter()
        .map(|(t, st)| (t, auf_kuppel(&st, nord, k)))
        .collect()
}

/// Der Himmel zu Lage, Datum und Uhrzeit um den Quader `q`.
pub fn himmel(l: &Location, s: &Sun, q: (Vec3, Vec3)) -> Himmel {
    let (lage, nord) = (bauort(l), l.north_deg());
    let k = kuppel(q);
    let tag = bahn(lage, nord, s.date, k);
    let jahr = |m, d| Datum::new(s.date.jahr, m, d).unwrap_or(s.date);
    let nur = |v: Vec<(Zeitpunkt, Vec3)>| v.into_iter().map(|p| p.1).collect();
    // Volle Stunden der gesetzlichen Zeit zwischen Auf- und Untergang
    let mut stunden = Vec::new();
    if let (Some(a), Some(b)) = (tag.first(), tag.last()) {
        for h in 0..24 {
            let t = Zeitpunkt::ortszeit(s.date, h, 0);
            if a.0 <= t && t <= b.0 {
                stunden.push(auf_kuppel(&sonne::sonnenstand(lage, t), nord, k));
            }
        }
    }
    let st = sonne::sonnenstand(lage, zeitpunkt(s));
    Himmel {
        sommer: nur(bahn(lage, nord, jahr(6, 21), k)),
        winter: nur(bahn(lage, nord, jahr(12, 21), k)),
        tag,
        stunden,
        sonne: ueber(&st).then(|| auf_kuppel(&st, nord, k)),
    }
}

/// Wie weit entlang des Bildschirmzugs `pts` (Bruchteil des Index) der
/// Punkt `m` am nächsten liegt, gesucht ab der Stelle `von` (Stetigkeit vor
/// Nähe, §8 09:25): Es geht nur weiter, solange der Abstand sinkt, so
/// springt die Sonne nicht vom Vormittag in den Nachmittag, wo sich beide
/// Äste im Bild überdecken. An den Enden hält sie an.
pub fn naechste_stelle(pts: &[(f64, f64)], m: (f64, f64), von: f64) -> f64 {
    let n = pts.len();
    if n < 2 {
        return 0.0;
    }
    let seg = |i: usize| {
        let (a, b) = (pts[i], pts[i + 1]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 > 1e-12 {
            (((m.0 - a.0) * dx + (m.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p = (a.0 + t * dx, a.1 + t * dy);
        (t, (p.0 - m.0).hypot(p.1 - m.1))
    };
    let mut i = (von.max(0.0).floor() as usize).min(n - 2);
    let (mut t, mut d) = seg(i);
    loop {
        if t >= 1.0 && i + 2 < n {
            let (t2, d2) = seg(i + 1);
            if d2 < d - 1e-9 {
                (i, t, d) = (i + 1, t2, d2);
                continue;
            }
        }
        if t <= 0.0 && i > 0 {
            let (t2, d2) = seg(i - 1);
            if d2 < d - 1e-9 {
                (i, t, d) = (i - 1, t2, d2);
                continue;
            }
        }
        break;
    }
    i as f64 + t
}

/// Zeitpunkt an der Stelle `k` der Tagesbahn (zwischen zwei Punkten
/// geteilt).
pub fn zeit_bei(tag: &[(Zeitpunkt, Vec3)], k: f64) -> Option<Zeitpunkt> {
    let n = tag.len();
    let i = (k.max(0.0).floor() as usize).min(n.checked_sub(1)?);
    let a = tag[i].0;
    let b = tag.get(i + 1).map_or(a, |p| p.0);
    let f = (k - i as f64).clamp(0.0, 1.0);
    Some(Zeitpunkt(a.0 + ((b.0 - a.0) as f64 * f).round() as i64))
}

/// Stelle der Tagesbahn zum Zeitpunkt `t` (Bruchteil des Index).
pub fn stelle_bei(tag: &[(Zeitpunkt, Vec3)], t: Zeitpunkt) -> f64 {
    let Some(i) = tag.iter().rposition(|p| p.0 <= t) else {
        return 0.0;
    };
    match tag.get(i + 1) {
        Some(b) if b.0 > tag[i].0 => {
            i as f64 + (t.0 - tag[i].0 .0) as f64 / (b.0 .0 - tag[i].0 .0) as f64
        }
        _ => i as f64,
    }
}

/// Uhrzeit (Minuten) zum Zeitpunkt `t` am Tag der Bahn, auf die Minute
/// innerhalb von Auf- und Untergang gerundet.
pub fn minuten_bei(tag: &[(Zeitpunkt, Vec3)], t: Zeitpunkt) -> Option<u32> {
    let (a, b) = (tag.first()?.0, tag.last()?.0);
    let m = |z: Zeitpunkt| z.in_ortszeit().minuten;
    let r = Zeitpunkt(((t.0 + 30).div_euclid(60)) * 60);
    let r = if r < a {
        Zeitpunkt(r.0 + 60)
    } else if r > b {
        Zeitpunkt(r.0 - 60)
    } else {
        r
    };
    Some(m(r))
}

// ===== Am Schatten ziehen (S6) =====

/// Ob `m` mehr als `weit` über ein Ende des Bildschirmzugs `pts` hinaus
/// liegt, an dem die Stelle `k` steht: `Some(false)` über den Anfang,
/// `Some(true)` über das Ende.
pub fn ueber_ende(pts: &[(f64, f64)], m: (f64, f64), k: f64, weit: f64) -> Option<bool> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let (e, v, hinten) = if k <= 0.0 {
        (pts[0], pts[1], false)
    } else if k >= (n - 1) as f64 {
        (pts[n - 1], pts[n - 2], true)
    } else {
        return None;
    };
    let (dx, dy) = (e.0 - v.0, e.1 - v.1);
    let l = dx.hypot(dy);
    let raus = ((m.0 - e.0) * dx + (m.1 - e.1) * dy) / l;
    (l > 1e-9 && raus > weit).then_some(hinten)
}

/// Schatten des Punkts `p` am Boden (z = 0) bei der Richtung `d` zur Sonne
/// (`d.z > 0`).
pub fn am_boden(p: Vec3, d: Vec3) -> Vec3 {
    let q = p - d * (p.z / d.z);
    vec3(q.x, q.y, 0.0)
}

/// Eckpunkte der Flächen über dem Boden, je Millimeter einmal: aus ihnen
/// wird die Schattenspitze gewählt.
pub fn ecken(faces: &[[f32; 9]]) -> Vec<Vec3> {
    let mut v: Vec<[i64; 3]> = faces
        .iter()
        .filter(|f| f[2] > 1.0)
        .map(|f| [0, 1, 2].map(|i| f[i].round() as i64))
        .collect();
    v.sort_unstable();
    v.dedup();
    v.into_iter()
        .map(|p| vec3(p[0] as f64, p[1] as f64, p[2] as f64))
        .collect()
}

/// Der Punkt aus `punkte`, dessen Schatten am weitesten in Schattenrichtung
/// fällt (die Spitze); `None` unter [`schatten::MIN_HOEHE`].
pub fn spitze(punkte: &[Vec3], d: Vec3) -> Option<Vec3> {
    if d.z.is_nan() || d.z < schatten::min_sinus() {
        return None;
    }
    let weg = vec3(-d.x, -d.y, 0.0);
    let weit = |p: &Vec3| am_boden(*p, d).dot(weg);
    punkte
        .iter()
        .filter(|p| p.z > 1.0)
        .max_by(|a, b| weit(a).total_cmp(&weit(b)))
        .copied()
}

/// Tageskurve des Schattens von `p` am Tag von `s`: Zeitpunkt und
/// Bodenpunkt alle fünf Minuten, von [`schatten::MIN_HOEHE`] morgens bis
/// abends, die Enden auf die Sekunde genau dort.
pub fn schattenkurve(l: &Location, s: &Sun, p: Vec3) -> Vec<(Zeitpunkt, Vec3)> {
    let (lage, nord) = (bauort(l), l.north_deg());
    let min = schatten::min_sinus();
    let d = |t: Zeitpunkt| sonne::sonnenstand(lage, t).richtung_modell(nord);
    let bahn: Vec<Zeitpunkt> = sonne::tagesbahn(lage, s.date, SCHRITT)
        .into_iter()
        .map(|b| b.0)
        .collect();
    let hoch = |t: &Zeitpunkt| d(*t).z >= min;
    let (Some(a), Some(b)) = (bahn.iter().position(hoch), bahn.iter().rposition(hoch)) else {
        return Vec::new();
    };
    // Grenze zwischen einem Zeitpunkt darüber und einem darunter
    let grenze = |mut drin: i64, mut drauss: i64| {
        while (drin - drauss).abs() > 1 {
            let m = (drin + drauss) / 2;
            if hoch(&Zeitpunkt(m)) {
                drin = m;
            } else {
                drauss = m;
            }
        }
        Zeitpunkt(drin)
    };
    let mut ts: Vec<Zeitpunkt> = Vec::new();
    let mut dazu = |t: Zeitpunkt| {
        if ts.last().is_none_or(|l| *l < t) {
            ts.push(t);
        }
    };
    if a > 0 {
        dazu(grenze(bahn[a].0, bahn[a - 1].0));
    }
    bahn[a..=b].iter().for_each(|t| dazu(*t));
    if let Some(n) = bahn.get(b + 1) {
        dazu(grenze(bahn[b].0, n.0));
    }
    ts.into_iter().map(|t| (t, am_boden(p, d(t)))).collect()
}

/// Griff an der Schattenspitze: der Netzpunkt, sein Schatten und dessen
/// Tageskurve.
#[derive(Clone, Debug, PartialEq)]
pub struct Griff {
    pub punkt: Vec3,
    pub boden: Vec3,
    pub kurve: Vec<(Zeitpunkt, Vec3)>,
}

/// Der Griff zu Lage, Datum und Uhrzeit unter den Punkten `punkte` (Ecken
/// des Gebäudes bzw. des Würfels); `None` ohne Schatten.
pub fn griff(l: &Location, s: &Sun, punkte: &[Vec3]) -> Option<Griff> {
    let d = zur_sonne(l, s)?;
    let punkt = spitze(punkte, d)?;
    Some(Griff {
        punkt,
        boden: am_boden(punkt, d),
        kurve: schattenkurve(l, s, punkt),
    })
}

/// Netz des Würfels (Darstellung ohne Baustoff, Kanten wie die Ansicht).
#[cfg(test)]
pub fn wuerfel_netz() -> MeshData {
    quader_netz(wuerfel_quader())
}

/// Quader als Testkörper-Netz: sechs Seiten, Schlüssel 0, zwölf Kanten.
#[cfg(test)]
pub fn quader_netz((lo, hi): (Vec3, Vec3)) -> MeshData {
    let mut m = MeshData::default();
    let c = |i: usize| {
        vec3(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        )
    };
    // Je Seite vier Ecken, gegen den Uhrzeigersinn von außen, und Normale
    let seiten: [([usize; 4], [f32; 3]); 6] = [
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([3, 2, 6, 7], [0.0, 1.0, 0.0]),
        ([2, 0, 4, 6], [-1.0, 0.0, 0.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
        ([2, 3, 1, 0], [0.0, 0.0, -1.0]),
    ];
    for (q, n) in seiten {
        for i in [0, 1, 2, 0, 2, 3] {
            let p = c(q[i]).to_f32();
            m.faces
                .push([p[0], p[1], p[2], n[0], n[1], n[2], 0.0, 0.0, 0.0]);
        }
    }
    let kind = edge_kind::VIEW as f32;
    for (a, b) in [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        m.edges.push(([c(a).to_f32(), c(b).to_f32()], kind));
    }
    m
}

/// Ein Strich am Himmel: Punkte, Breite (dip), Farbe.
type Strich = (Vec3, Vec3, f32, [f32; 4]);

/// Alles am Himmel als Striche und Punkte (gleiche Punkte), in
/// Zeichenreihenfolge; die Scheibe zuletzt, mit Rand. `heiss`: beim
/// Darüberfahren und Ziehen etwas größer.
fn striche(h: &Himmel, heiss: bool) -> Vec<Strich> {
    let mut out = Vec::new();
    let zug = |out: &mut Vec<Strich>, p: &[Vec3], b: f32, c: [f32; 4]| {
        out.extend(p.windows(2).map(|w| (w[0], w[1], b, c)));
    };
    zug(&mut out, &h.sommer, GRENZ_PX, GRENZ);
    zug(&mut out, &h.winter, GRENZ_PX, GRENZ);
    let tag: Vec<Vec3> = h.tag.iter().map(|p| p.1).collect();
    zug(&mut out, &tag, BAHN_PX, BAHN);
    out.extend(h.stunden.iter().map(|&p| (p, p, MARKE_PX, BAHN)));
    if let Some(p) = h.sonne {
        let d = if heiss { 4.0 } else { 0.0 };
        out.push((p, p, SCHEIBE_PX + d, RAND));
        out.push((p, p, SCHEIBE_PX + d - 2.0 * RAND_PX, GELB));
    }
    out
}

/// Hilfslinien für den Renderer (Pixel bei Skalierung `s`); hinter dem
/// Gebäude blass.
pub fn helpers(h: &Himmel, heiss: bool, s: f32) -> Vec<Helper> {
    als_helper(striche(h, heiss), s)
}

/// Griff und Kurve am Schatten als Striche: der Griff an der Spitze, die
/// Kurve beim Darüberfahren und Ziehen. Beim Ziehen gilt der gegriffene
/// Punkt; wirft er keinen Schatten, steht der Griff am Ende der Kurve.
fn schatten_striche(sys: &Sonnensystem, g: Option<&Griff>, d: Option<Vec3>) -> Vec<Strich> {
    let (kurve, spitze, heiss) = match sys.am_schatten() {
        Some(z) => {
            // Unter 2° bleibt der Griff am Ende der Kurve auf der Seite der
            // Sonne (§8 11:30)
            let fuss = vec3(z.punkt.x, z.punkt.y, 0.0);
            let ende = |d: Vec3| {
                let weg = vec3(-d.x, -d.y, 0.0);
                let zu = |p: &(Zeitpunkt, Vec3)| (p.1 - fuss).normalized().dot(weg);
                let (a, b) = (z.kurve.first()?, z.kurve.last()?);
                Some(if zu(a) >= zu(b) { a.1 } else { b.1 })
            };
            let spitze = d.and_then(|d| {
                if d.z >= schatten::min_sinus() {
                    Some(am_boden(z.punkt, d))
                } else {
                    ende(d)
                }
            });
            (Some(&z.kurve), spitze, true)
        }
        None => (
            g.filter(|_| sys.ueber_schatten).map(|g| &g.kurve),
            g.map(|g| g.boden),
            sys.ueber_schatten,
        ),
    };
    let mut out: Vec<Strich> = kurve
        .into_iter()
        .flat_map(|k| k.windows(2).map(|w| (w[0].1, w[1].1, KURVE_PX, BAHN)))
        .collect();
    if let Some(p) = spitze {
        let d = if heiss { 4.0 } else { 0.0 };
        out.push((p, p, GRIFF_PX + d, BAHN));
        out.push((p, p, GRIFF_PX + d - 2.0 * RAND_PX, GRIFF));
    }
    out
}

/// Griff und Kurve am Schatten für den Renderer (Pixel bei Skalierung
/// `s`); `d`: Richtung zur Sonne.
pub fn schatten_helpers(
    sys: &Sonnensystem,
    g: Option<&Griff>,
    d: Option<Vec3>,
    s: f32,
) -> Vec<Helper> {
    als_helper(schatten_striche(sys, g, d), s)
}

fn als_helper(st: Vec<Strich>, s: f32) -> Vec<Helper> {
    st.into_iter()
        .map(|(a, b, w, c)| Helper {
            a: a.to_f32(),
            b: b.to_f32(),
            color: c,
            width: w * s,
            dash: 0.0,
            pattern: SOLID,
            occlude: true,
            round: true,
        })
        .collect()
}

/// Dasselbe mit dem Pinsel auf ein Bild (`w` × `h`), ohne Verdecken (für
/// Ist-Bilder ohne Grafikkarte).
#[cfg(test)]
pub fn malen(c: &mut Canvas, h: &Himmel, cam: &Camera, (w, hh): (f64, f64), s: f32) {
    let farbe = |f: [f32; 4]| {
        let k = |v: f32| (v * 255.0).round() as u8;
        Rgba(k(f[0]), k(f[1]), k(f[2]), k(f[3]))
    };
    for (a, b, breite, f) in striche(h, false) {
        let (Some(p), Some(q)) = (cam.project(a, w, hh), cam.project(b, w, hh)) else {
            continue;
        };
        let (p, q) = ((p.0 as f32, p.1 as f32), (q.0 as f32, q.1 as f32));
        let r = breite * s * 0.5;
        let mut path = Path::new();
        if p != q {
            path.segment(p, q, 2.0 * r);
            c.fill(&path, farbe(f));
            path = Path::new();
        }
        for e in [p, q] {
            path.rounded_rect(e.0 - r, e.1 - r, 2.0 * r, 2.0 * r, r);
        }
        c.fill(&path, farbe(f));
    }
}

// ===== Leiste =====

/// Teil der Leiste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Teil {
    Datum,
    Uhrzeit,
    /// Schnellwahl 0 … 3, 4 ist „Jetzt“.
    Schnell(usize),
}

/// Maße der Leiste (dip).
const PAD: f32 = 8.0;
const GAP: f32 = 6.0;
const DATUM_W: f32 = 96.0;
const ZEIT_W: f32 = 98.0;
const KNOPF_W: f32 = 54.0;

/// Leiste oben in der Mitte der Ansicht (Pixel, Ansichtskoordinaten): die
/// Fläche und ihre Teile. `unter`: mit dem Hinweis rechts.
pub fn leiste(fonts: &Fonts, vw: f64, unter: bool, s: f32, t: &Theme) -> (Rect, Vec<(Teil, Rect)>) {
    let h = t.size.field_height;
    let hinweis = if unter {
        let px = t.size.font_small;
        GAP + fonts.regular.as_ref().map_or(150.0, |f| f.width(UNTER, px)) + 4.0
    } else {
        0.0
    };
    // Mittig ohne Hinweis; der Hinweis hängt rechts an, die Felder bleiben
    let breite = 2.0 * PAD + DATUM_W + GAP + ZEIT_W + 2.0 * GAP + 5.0 * KNOPF_W + 4.0 * GAP;
    let r = |x: f32, y: f32, w: f32, h: f32| {
        Rect::new(
            (x * s).round(),
            (y * s).round(),
            (w * s).round(),
            (h * s).round(),
        )
    };
    let x0 = ((vw as f32 / s - breite) * 0.5).max(0.0);
    let breite = breite + hinweis;
    let y0 = t.size.panel_margin;
    let mut teile = vec![
        (Teil::Datum, r(x0 + PAD, y0 + PAD, DATUM_W, h)),
        (
            Teil::Uhrzeit,
            r(x0 + PAD + DATUM_W + GAP, y0 + PAD, ZEIT_W, h),
        ),
    ];
    let mut x = x0 + PAD + DATUM_W + GAP + ZEIT_W + 2.0 * GAP;
    for i in 0..5 {
        teile.push((Teil::Schnell(i), r(x, y0 + PAD, KNOPF_W, h)));
        x += KNOPF_W + GAP;
    }
    (r(x0, y0, breite, h + 2.0 * PAD), teile)
}

/// Beschriftung eines Schnellwahlknopfs.
pub fn knopf_text(i: usize) -> String {
    match SCHNELL.get(i) {
        Some((m, d)) => format!("{d:02}.{m:02}."),
        None => "Jetzt".into(),
    }
}

/// Getippter Wert in einem Feld der Leiste.
#[derive(Clone, Debug, PartialEq)]
pub struct Eingabe {
    pub teil: Teil,
    pub edit: TextEdit,
    pub falsch: bool,
}

/// Was die Leiste zeigt; gleich: kein neues Bild.
#[derive(Clone, Debug, PartialEq)]
pub struct LeistenBild {
    pub sun: Sun,
    pub unter: bool,
    /// Ohne Gebäude: nur [`KEIN_HAUS`] (S9).
    pub ohne_haus: bool,
    pub eingabe: Option<Eingabe>,
    pub hover: Option<Teil>,
    pub vw: u32,
    pub scale: u32,
}

/// Leiste zeichnen: Bild und Lage in der Ansicht (Pixel), mit Platz für den
/// Schatten.
pub fn leiste_malen(b: &LeistenBild, fonts: &Fonts, t: &Theme) -> (Canvas, i32, i32) {
    let s = f32::from_bits(b.scale);
    if b.ohne_haus {
        return kein_haus_malen(b.vw as f64, fonts, s, t);
    }
    let (r, teile) = leiste(fonts, b.vw as f64, b.unter, s, t);
    let sh = (t.size.panel_shadow * s).ceil();
    let (w, h) = ((r.w + 2.0 * sh) as usize, (r.h + 2.0 * sh) as usize);
    let mut c = Canvas::new(w, h);
    let (dx, dy) = (r.x - sh, r.y - sh);
    let ab = |q: Rect| Rect::new(q.x - dx, q.y - dy, q.w, q.h);
    widgets::panel(&mut c, ab(r), s, t);
    // So, wie gerechnet wird: auch eine Datei mit 29.03. 02:30 zeigt
    // 03:30 MESZ (Hinweis zu Y, Test und Review 3by)
    let sun = gueltig(b.sun);
    let zone = zone(&sun);
    for (teil, q) in teile {
        let q = ab(q);
        let hover = b.hover == Some(teil);
        let ein = b.eingabe.as_ref().filter(|e| e.teil == teil);
        let (text, unit) = match teil {
            Teil::Datum => (datum_text(sun.date), ""),
            Teil::Uhrzeit => (zeit_text(sun.minutes), zone),
            Teil::Schnell(i) => {
                let aktiv = SCHNELL
                    .get(i)
                    .is_some_and(|&(m, d)| (sun.date.monat, sun.date.tag) == (m, d));
                let st = ButtonState {
                    hover,
                    active: aktiv,
                    ..Default::default()
                };
                widgets::button(&mut c, fonts, q, &knopf_text(i), st, s, t);
                continue;
            }
        };
        let st = match ein {
            Some(e) => FieldState {
                text: &e.edit.text,
                unit,
                focus: true,
                invalid: e.falsch,
                caret: Some(e.edit.caret),
                select: Some(e.edit.selection()),
                ..Default::default()
            },
            None => FieldState {
                text: &text,
                unit,
                hover,
                ..Default::default()
            },
        };
        widgets::field(&mut c, fonts, q, &st, s, t);
    }
    if b.unter {
        if let Some(f) = fonts.regular.as_ref() {
            let px = t.size.font_small * s;
            let x = r.x + r.w - (PAD + 2.0) * s - f.width(UNTER, px);
            let y = r.y + (r.h + f.cap_height(px)) * 0.5;
            f.draw(
                &mut c,
                UNTER,
                px,
                (x - dx).round(),
                (y - dy).round(),
                t.ui.text_dim,
            );
        }
    }
    (c, dx as i32, dy as i32)
}

/// Statt der Leiste: [`KEIN_HAUS`] in einem Feld oben in der Mitte, so
/// hoch wie die Leiste (S9).
fn kein_haus_malen(vw: f64, fonts: &Fonts, s: f32, t: &Theme) -> (Canvas, i32, i32) {
    let px = t.size.font_small * s;
    let f = fonts.regular.as_ref();
    let tw = f.map_or(300.0 * s, |f| f.width(KEIN_HAUS, px));
    let w = (tw + 2.0 * (PAD + 4.0) * s).round();
    let h = ((t.size.field_height + 2.0 * PAD) * s).round();
    let r = Rect::new(
        ((vw as f32 - w) * 0.5).max(0.0).round(),
        (t.size.panel_margin * s).round(),
        w,
        h,
    );
    let sh = (t.size.panel_shadow * s).ceil();
    let mut c = Canvas::new((r.w + 2.0 * sh) as usize, (r.h + 2.0 * sh) as usize);
    let (dx, dy) = (r.x - sh, r.y - sh);
    widgets::panel(&mut c, Rect::new(sh, sh, r.w, r.h), s, t);
    if let Some(f) = f {
        let y = sh + (r.h + f.cap_height(px)) * 0.5;
        let x = sh + (r.w - tw) * 0.5;
        f.draw(&mut c, KEIN_HAUS, px, x.round(), y.round(), t.ui.text_dim);
    }
    (c, dx as i32, dy as i32)
}

// ===== Zustand und Ereignisse =====

/// Bedienzustand des Systems (Datum, Uhrzeit und Schalter stehen im
/// Modell).
#[derive(Clone, Debug, Default)]
pub struct Sonnensystem {
    /// Maus über der Scheibe, dem Griff am Schatten bzw. über einem Teil
    /// der Leiste.
    pub ueber_sonne: bool,
    pub ueber_schatten: bool,
    pub hover: Option<Teil>,
    /// Gedrückt auf der Scheibe oder dem Griff: Ort; ab [`ZUG_PX`] wird
    /// gezogen.
    unten: Option<(f64, f64)>,
    /// Beim Ziehen: Stelle auf der Tagesbahn bzw. der Schattenkurve.
    zug: Option<f64>,
    /// Gegriffen am Schatten: der Griff von da an, mit fester Kurve bis zum
    /// Loslassen.
    am_schatten: Option<Griff>,
    pub eingabe: Option<Eingabe>,
}

/// Ergebnis eines Ereignisses.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// Neuer Stand für das Modell.
    pub sun: Option<Sun>,
}

/// Was das System zum Ereignis braucht.
pub struct Lagebild<'a> {
    pub sun: Sun,
    pub himmel: &'a Himmel,
    /// Griff an der Schattenspitze, solange es Schatten gibt.
    pub griff: Option<&'a Griff>,
    pub cam: &'a Camera,
    pub wh: (f64, f64),
    pub scale: f64,
    pub fonts: &'a Fonts,
    pub theme: &'a Theme,
}

impl Sonnensystem {
    /// Wird gerade die Sonne gezogen oder eine Eingabe getippt.
    pub fn is_busy(&self) -> bool {
        self.zug.is_some() || self.unten.is_some()
    }

    /// Alles Angefangene vergessen (System aus, andere Ansicht).
    pub fn reset(&mut self) {
        *self = Sonnensystem::default();
    }

    fn scheibe(&self, lb: &Lagebild, m: (f64, f64)) -> bool {
        let Some(p) = lb
            .himmel
            .sonne
            .and_then(|p| lb.cam.project(p, lb.wh.0, lb.wh.1))
        else {
            return false;
        };
        (p.0 - m.0).hypot(p.1 - m.1) <= (SCHEIBE_PX as f64 * 0.5 + GREIF_PX) * lb.scale
    }

    fn am_griff(&self, lb: &Lagebild, m: (f64, f64)) -> bool {
        let Some(p) = lb
            .griff
            .and_then(|g| lb.cam.project(g.boden, lb.wh.0, lb.wh.1))
        else {
            return false;
        };
        (p.0 - m.0).hypot(p.1 - m.1) <= (GRIFF_PX as f64 * 0.5 + GREIF_PX) * lb.scale
    }

    /// Der feste Griff beim Ziehen am Schatten.
    pub fn am_schatten(&self) -> Option<&Griff> {
        self.am_schatten.as_ref()
    }

    /// Ziehen am Schatten: Stelle `k` auf der Kurve von `g` zur Maus `m`,
    /// über die Enden hinaus Auf- bzw. Untergang. Gezogen wird nur auf dem
    /// Teil im Fenster (§8 11:30): Punkte hinter der Kamera oder außerhalb
    /// zählen nicht, dort bleibt der Zug am letzten sichtbaren Punkt stehen.
    fn schatten_ziehen(g: &Griff, lb: &Lagebild, m: (f64, f64), k: f64) -> (f64, Option<u32>) {
        let (w, h) = lb.wh;
        let pts: Vec<(f64, f64)> = g
            .kurve
            .iter()
            .map(|p| {
                lb.cam
                    .project(p.1, w, h)
                    .filter(|q| (0.0..=w).contains(&q.0) && (0.0..=h).contains(&q.1))
                    .unwrap_or((f64::NAN, f64::NAN))
            })
            .collect();
        let k = naechste_stelle(&pts, m, k);
        let tag = &lb.himmel.tag;
        let min = match ueber_ende(&pts, m, k, ENDE_PX * lb.scale) {
            Some(hinten) => {
                let t = if hinten { tag.last() } else { tag.first() };
                t.and_then(|t| minuten_bei(tag, t.0))
            }
            None => zeit_bei(&g.kurve, k).and_then(|t| minuten_bei(&g.kurve, t)),
        };
        (k, min)
    }

    fn teil(&self, lb: &Lagebild, m: (f64, f64)) -> (bool, Option<Teil>) {
        let unter = lb.himmel.sonne.is_none();
        let (r, teile) = leiste(lb.fonts, lb.wh.0, unter, lb.scale as f32, lb.theme);
        let teil = teile
            .iter()
            .find(|(_, q)| q.contains(m.0, m.1))
            .map(|p| p.0);
        (r.contains(m.0, m.1), teil)
    }

    /// Mausereignis in der Ansicht (ohne Titelleiste).
    pub fn handle(&mut self, e: &Event, lb: &Lagebild) -> Ausgang {
        let mut out = Ausgang::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some(u) = self.unten {
                    let weit = (x - u.0).hypot(y - u.1) >= ZUG_PX * lb.scale;
                    if self.zug.is_none() && weit {
                        let t = zeitpunkt(&lb.sun);
                        let weg = self
                            .am_schatten
                            .as_ref()
                            .map_or(&lb.himmel.tag, |g| &g.kurve);
                        self.zug = Some(stelle_bei(weg, t));
                    }
                    if let (Some(k), Some(g)) = (self.zug, &self.am_schatten) {
                        let (k, m) = Self::schatten_ziehen(g, lb, (x, y), k);
                        self.zug = Some(k);
                        if let Some(m) = m.filter(|m| *m != lb.sun.minutes) {
                            out.sun = Some(Sun {
                                minutes: m,
                                ..lb.sun
                            });
                        }
                        out.redraw = true;
                    } else if let Some(k) = self.zug {
                        let pts: Vec<(f64, f64)> = lb
                            .himmel
                            .tag
                            .iter()
                            .map(|p| {
                                lb.cam
                                    .project(p.1, lb.wh.0, lb.wh.1)
                                    .unwrap_or((f64::NAN, f64::NAN))
                            })
                            .collect();
                        let k = naechste_stelle(&pts, (x, y), k);
                        self.zug = Some(k);
                        let m = zeit_bei(&lb.himmel.tag, k)
                            .and_then(|t| minuten_bei(&lb.himmel.tag, t));
                        if let Some(m) = m.filter(|m| *m != lb.sun.minutes) {
                            out.sun = Some(Sun {
                                minutes: m,
                                ..lb.sun
                            });
                        }
                        out.redraw = true;
                    }
                    out.consumed = true;
                    return out;
                }
                let (ueber_leiste, teil) = self.teil(lb, (x, y));
                let sonne = !ueber_leiste && self.scheibe(lb, (x, y));
                let schatten = !ueber_leiste && !sonne && self.am_griff(lb, (x, y));
                out.redraw = teil != self.hover
                    || sonne != self.ueber_sonne
                    || schatten != self.ueber_schatten;
                (self.hover, self.ueber_sonne, self.ueber_schatten) = (teil, sonne, schatten);
                out.consumed = ueber_leiste;
            }
            Event::MouseLeave => {
                out.redraw = self.hover.take().is_some()
                    | std::mem::take(&mut self.ueber_sonne)
                    | std::mem::take(&mut self.ueber_schatten);
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                let (ueber_leiste, teil) = self.teil(lb, (x, y));
                if ueber_leiste {
                    out.consumed = true;
                    out.redraw = true;
                    match teil {
                        Some(t @ (Teil::Datum | Teil::Uhrzeit)) => {
                            if self.eingabe.as_ref().is_none_or(|e| e.teil != t) {
                                let text = match t {
                                    Teil::Datum => datum_text(lb.sun.date),
                                    _ => zeit_text(lb.sun.minutes),
                                };
                                self.eingabe = Some(Eingabe {
                                    teil: t,
                                    edit: TextEdit::new(&text),
                                    falsch: false,
                                });
                            }
                        }
                        Some(Teil::Schnell(i)) => {
                            self.eingabe = None;
                            out.sun = Some(if i < SCHNELL.len() {
                                schnell(lb.sun, i)
                            } else {
                                jetzt(uhr(), true)
                            });
                        }
                        None => self.eingabe = None,
                    }
                    return out;
                }
                if self.eingabe.take().is_some() {
                    out.redraw = true;
                }
                if self.scheibe(lb, (x, y)) {
                    self.unten = Some((x, y));
                    out.consumed = true;
                    out.redraw = true;
                } else if self.am_griff(lb, (x, y)) {
                    self.unten = Some((x, y));
                    self.am_schatten = lb.griff.cloned();
                    out.consumed = true;
                    out.redraw = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } if self.unten.take().is_some() => {
                self.zug = None;
                self.am_schatten = None;
                out.consumed = true;
                out.redraw = true;
            }
            _ => {}
        }
        out
    }

    /// Taste in einem Feld der Leiste; `true`, wenn verbraucht.
    pub fn key(&mut self, key: Key, mods: Modifiers, sun: Sun, out: &mut Ausgang) -> bool {
        let Some(e) = self.eingabe.as_mut() else {
            return false;
        };
        let ed = &mut e.edit;
        match key {
            Key::Char(c) if c.is_ascii_digit() || matches!(c, '.' | ',' | ':') => {
                ed.insert(&c.to_string());
                e.falsch = false;
            }
            Key::Backspace => ed.backspace(),
            Key::Delete => ed.delete(),
            Key::Left => ed.left(mods.shift),
            Key::Right => ed.right(mods.shift),
            Key::Home => ed.home(mods.shift),
            Key::End => ed.end(mods.shift),
            Key::Escape => self.eingabe = None,
            Key::Enter => {
                let neu = match e.teil {
                    Teil::Datum => {
                        datum_lesen(&ed.text, sun.date.jahr).map(|date| Sun { date, ..sun })
                    }
                    _ => zeit_lesen(&ed.text).map(|minutes| Sun { minutes, ..sun }),
                };
                match neu {
                    Some(s) => {
                        out.sun = Some(gueltig(s));
                        self.eingabe = None;
                    }
                    None => e.falsch = true,
                }
            }
            // Strg+Z und anderes bleiben im Feld, ohne Wirkung auf das Modell
            Key::Char(_) => {}
            _ => return false,
        }
        out.redraw = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sun(j: i32, m: u32, d: u32, h: u32, min: u32) -> Sun {
        Sun {
            date: Datum::new(j, m, d).unwrap(),
            minutes: h * 60 + min,
            on: true,
        }
    }

    #[test]
    fn eingaben_lesen() {
        let d = |j, m, t| Datum::new(j, m, t);
        assert_eq!(datum_lesen("21.6.", 2026), d(2026, 6, 21));
        assert_eq!(datum_lesen("21.06.2027", 2026), d(2027, 6, 21));
        assert_eq!(datum_lesen("1.1.26", 2030), d(2026, 1, 1));
        assert_eq!(datum_lesen("2106", 2026), d(2026, 6, 21));
        assert_eq!(datum_lesen("21122026", 2020), d(2026, 12, 21));
        assert_eq!(datum_lesen("31.2.", 2026), None);
        assert_eq!(datum_lesen("21.6.123", 2026), None);
        assert_eq!(datum_lesen("", 2026), None);
        assert_eq!(zeit_lesen("12"), Some(720));
        assert_eq!(zeit_lesen("9.30"), Some(570));
        assert_eq!(zeit_lesen("12,05"), Some(725));
        assert_eq!(zeit_lesen("12:00"), Some(720));
        assert_eq!(zeit_lesen("930"), Some(570));
        assert_eq!(zeit_lesen("2359"), Some(1439));
        assert_eq!(zeit_lesen("24"), None);
        assert_eq!(zeit_lesen("12.60"), None);
        assert_eq!(zeit_lesen("x"), None);
        // Hinweis Y: die übersprungene Stunde am Umstelltag
        let s = |t, m, mi| Sun {
            date: d(2026, t, m).unwrap(),
            minutes: mi,
            on: true,
        };
        assert_eq!(gueltig(s(3, 29, 150)), s(3, 29, 210));
        assert_eq!(zone(&gueltig(s(3, 29, 150))), "MESZ");
        for x in [s(3, 29, 90), s(3, 29, 180), s(6, 21, 150), s(10, 25, 150)] {
            assert_eq!(gueltig(x), x);
        }
        assert_eq!(datum_text(d(2026, 6, 1).unwrap()), "01.06.2026");
        assert_eq!(zeit_text(545), "09:05");
    }

    /// MEZ und MESZ nach dem Datum; die Schnellwahl behält Jahr und Uhrzeit.
    #[test]
    fn zone_und_schnellwahl() {
        assert_eq!(zone(&sun(2026, 6, 21, 12, 0)), "MESZ");
        assert_eq!(zone(&sun(2026, 12, 21, 12, 0)), "MEZ");
        let s = schnell(sun(2027, 1, 5, 9, 30), 2);
        assert_eq!((s.date, s.minutes), (Datum::new(2027, 9, 23).unwrap(), 570));
        assert_eq!(knopf_text(0), "21.03.");
        assert_eq!(knopf_text(4), "Jetzt");
        // 21.06.2026 12:00 MESZ ist 10:00 Weltzeit
        let t = zeitpunkt(&sun(2026, 6, 21, 12, 0));
        assert_eq!(
            t,
            Zeitpunkt::utc(Datum::new(2026, 6, 21).unwrap(), 10, 0, 0)
        );
        assert_eq!(jetzt(t, false).minutes, 720);
    }

    /// Licht von der Sonne: mittags von Süden oben, nachts die feste
    /// Richtung; mit Nord 90° (Nord ist +x) kommt die Mittagssonne von −x.
    #[test]
    fn licht_folgt_der_sonne() {
        let l = Location::default();
        let a = licht(&l, &sun(2026, 6, 21, 13, 27)).unwrap();
        assert!(a[1] < -0.4 && a[2] > 0.8 && a[0].abs() < 0.05, "{a:?}");
        assert_eq!(licht(&l, &sun(2026, 6, 21, 1, 0)), None);
        let l = Location {
            north: Some(90.0),
            ..l
        };
        let a = licht(&l, &sun(2026, 6, 21, 13, 27)).unwrap();
        assert!(a[0] < -0.4 && a[1].abs() < 0.05, "{a:?}");
    }

    /// Befund C (§8 10:20): Die Kuppel fasst auch einen Turm; jede Ecke
    /// liegt innerhalb, mit Luft, und die Bahn läuft nicht durchs Gebäude.
    #[test]
    fn kuppel_um_einen_turm() {
        let q = (vec3(0.0, 0.0, 0.0), vec3(10000.0, 10000.0, 40000.0));
        let (m, r) = kuppel(q);
        assert_eq!(m, vec3(5000.0, 5000.0, 0.0));
        let fernste = vec3(10000.0, 10000.0, 40000.0) - m;
        assert!((r - 1.5 * fernste.length()).abs() < 1e-6, "{r}");
        assert!(r > 60000.0);
        let h = himmel(&Location::default(), &sun(2026, 6, 21, 13, 27), q);
        let innen = |p: Vec3| {
            (q.0.x..=q.1.x).contains(&p.x)
                && (q.0.y..=q.1.y).contains(&p.y)
                && (q.0.z..=q.1.z).contains(&p.z)
        };
        assert!(h.tag.iter().all(|p| !innen(p.1)));
        // Klein bleibt es bei 15 m
        assert_eq!(
            kuppel((vec3(0.0, 0.0, 0.0), vec3(3000.0, 3000.0, 3000.0))).1,
            KUPPEL_MIN
        );
    }

    /// Die Bahnen stehen über dem Horizont; die Sonne liegt auf der
    /// Tagesbahn, mittags im Süden, 21.06. höher als 21.12.
    #[test]
    fn himmel_um_den_wuerfel() {
        let q = wuerfel_quader();
        let (m, r) = kuppel(q);
        // 1,5 · √(5² + 5² + 10²) m
        assert!((r - 1.5 * 150.0e6_f64.sqrt()).abs() < 1e-6, "{r}");
        assert!((18370.0..18380.0).contains(&r));
        let h = himmel(&Location::default(), &sun(2026, 6, 21, 13, 27), q);
        for p in h.tag.iter().map(|p| p.1).chain(h.sommer.iter().copied()) {
            assert!(p.z >= 0.0 && ((p - m).length() - r).abs() < 1e-6);
        }
        let s = h.sonne.unwrap();
        assert!(s.y < m.y - 0.4 * r && s.z > 0.8 * r, "{s:?}");
        let hoch = |v: &[Vec3]| v.iter().map(|p| p.z).fold(0.0, f64::max);
        assert!(hoch(&h.sommer) > hoch(&h.winter) + 0.3 * r);
        // Volle Stunden 05:00 … 21:00 MESZ (Aufgang 05:00, Untergang 21:56)
        assert!((16..=17).contains(&h.stunden.len()), "{}", h.stunden.len());
        let nachts = himmel(&Location::default(), &sun(2026, 6, 21, 23, 30), q);
        assert_eq!(nachts.sonne, None);
        assert_eq!(nachts.tag, h.tag);
    }

    /// Stetigkeit vor Nähe: Liegen Vor- und Nachmittag im Bild übereinander,
    /// bleibt die Sonne auf ihrem Ast; an den Enden hält sie.
    #[test]
    fn ziehen_bleibt_auf_dem_ast() {
        // Hin und zurück auf derselben Linie: 0…10 hin, 10…20 zurück
        let pts: Vec<(f64, f64)> = (0..=20)
            .map(|i| {
                let x = if i <= 10 { i } else { 20 - i } as f64 * 10.0;
                (x, if i <= 10 { 0.0 } else { 2.0 })
            })
            .collect();
        let k = naechste_stelle(&pts, (32.0, 1.2), 2.0);
        assert!((k - 3.2).abs() < 1e-9, "{k}");
        let k = naechste_stelle(&pts, (32.0, 1.0), 16.0);
        assert!((k - 16.8).abs() < 1e-9, "{k}");
        assert_eq!(naechste_stelle(&pts, (-50.0, 0.0), 3.0), 0.0);
        assert_eq!(naechste_stelle(&pts, (-50.0, 2.0), 17.0), 20.0);
        // Über die Spitze hinaus zum anderen Ast, wenn man ihm folgt
        let k = naechste_stelle(&pts, (100.0, 1.0), 9.0);
        assert!((9.0..=11.0).contains(&k), "{k}");
    }

    /// Ziehen über den Tag: Minuten innerhalb von Auf- und Untergang.
    #[test]
    fn zeit_auf_der_bahn() {
        let h = himmel(
            &Location::default(),
            &sun(2026, 6, 21, 12, 0),
            wuerfel_quader(),
        );
        let n = h.tag.len() as f64;
        let auf = minuten_bei(&h.tag, zeit_bei(&h.tag, 0.0).unwrap()).unwrap();
        let unter = minuten_bei(&h.tag, zeit_bei(&h.tag, n).unwrap()).unwrap();
        assert!((300..=301).contains(&auf), "{auf}");
        assert!((1315..=1316).contains(&unter), "{unter}");
        let t = zeitpunkt(&sun(2026, 6, 21, 12, 0));
        let k = stelle_bei(&h.tag, t);
        assert_eq!(minuten_bei(&h.tag, zeit_bei(&h.tag, k).unwrap()), Some(720));
    }

    #[test]
    fn wuerfel_ist_geschlossen() {
        let m = wuerfel_netz();
        assert_eq!((m.faces.len(), m.edges.len()), (36, 12));
        // Jede Normale zeigt aus dem Würfel heraus
        for f in &m.faces {
            let c = WUERFEL as f32 * 0.5;
            let d = [f[0] - c, f[1] - c, f[2] - c];
            assert!(d[0] * f[3] + d[1] * f[4] + d[2] * f[5] > 0.0);
        }
    }

    fn ohne_schrift() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    /// Bedienung: Sonne greifen und über den Tag ziehen (nur die Uhrzeit,
    /// stetig, hält am Abend), Schnellwahl und Datum per Zahl + Enter.
    #[test]
    fn ziehen_und_leiste() {
        let (fonts, theme) = (ohne_schrift(), Theme::dark());
        let (w, h) = (1200.0, 800.0);
        let q = wuerfel_quader();
        let cam = Camera::looking_at(
            vec3(5000.0, -45000.0, 30000.0),
            vec3(5000.0, 5000.0, 8000.0),
            45.0,
        );
        let l = Location::default();
        let mut sun = sun(2026, 6, 21, 12, 0);
        let mut sys = Sonnensystem::default();
        let ev = |sys: &mut Sonnensystem, sun: &mut Sun, e: Event| {
            let hi = himmel(&l, sun, q);
            let lb = Lagebild {
                sun: *sun,
                himmel: &hi,
                griff: None,
                cam: &cam,
                wh: (w, h),
                scale: 1.0,
                fonts: &fonts,
                theme: &theme,
            };
            let out = sys.handle(&e, &lb);
            if let Some(s) = out.sun {
                *sun = s;
            }
            out
        };
        let bild = |sun: &Sun| {
            let hi = himmel(&l, sun, q);
            cam.project(hi.sonne.unwrap(), w, h).unwrap()
        };
        let m = Modifiers::default();
        let (x, y) = bild(&sun);
        let mv = |x: f64, y: f64| Event::MouseMove { x, y, mods: m };
        assert!(ev(&mut sys, &mut sun, mv(x, y)).redraw && sys.ueber_sonne);
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        assert!(ev(&mut sys, &mut sun, down).consumed);
        // Unter der Schwelle bleibt die Zeit
        ev(&mut sys, &mut sun, mv(x + 2.0, y));
        assert_eq!(sun.minutes, 720);
        // Entlang der Bahn zum Nachmittag: die Zeit steigt stetig
        let hi = himmel(&l, &sun, q);
        let mut vorher = sun.minutes;
        for k in (0..hi.tag.len()).skip(stelle_bei(&hi.tag, zeitpunkt(&sun)) as usize + 1) {
            let (px, py) = cam.project(hi.tag[k].1, w, h).unwrap();
            ev(&mut sys, &mut sun, mv(px, py + 1.0));
            assert!(
                sun.minutes >= vorher && sun.minutes <= vorher + 10,
                "{k}: {vorher} → {}",
                sun.minutes
            );
            vorher = sun.minutes;
        }
        // Weit hinter dem Untergang hält sie am letzten Punkt
        ev(&mut sys, &mut sun, mv(w * 3.0, h));
        assert!((1315..=1316).contains(&sun.minutes), "{}", sun.minutes);
        assert_eq!(sun.date, Datum::new(2026, 6, 21).unwrap());
        ev(
            &mut sys,
            &mut sun,
            Event::MouseUp {
                button: MouseButton::Left,
                x: 0.0,
                y: 0.0,
                mods: m,
            },
        );
        assert!(!sys.is_busy());

        // Leiste: „21.12.“ wählt den Tag, die Uhrzeit bleibt
        let (_, teile) = leiste(&fonts, w, false, 1.0, &theme);
        let mitte = |t: Teil| {
            let r = teile.iter().find(|p| p.0 == t).unwrap().1;
            ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64)
        };
        let klick = |p: (f64, f64)| Event::MouseDown {
            button: MouseButton::Left,
            x: p.0,
            y: p.1,
            mods: m,
        };
        let vorher = sun.minutes;
        assert!(ev(&mut sys, &mut sun, klick(mitte(Teil::Schnell(3)))).consumed);
        assert_eq!(
            (sun.date, sun.minutes),
            (Datum::new(2026, 12, 21).unwrap(), vorher)
        );
        // Datum tippen: „21.3.“ + Enter
        ev(&mut sys, &mut sun, klick(mitte(Teil::Datum)));
        assert_eq!(sys.eingabe.as_ref().map(|e| e.teil), Some(Teil::Datum));
        let mut out = Ausgang::default();
        for c in "21.3.".chars() {
            assert!(sys.key(Key::Char(c), m, sun, &mut out));
        }
        assert!(sys.key(Key::Enter, m, sun, &mut out));
        assert_eq!(out.sun.map(|s| s.date), Datum::new(2026, 3, 21));
        assert!(sys.eingabe.is_none());
        // Falsche Uhrzeit: Feld bleibt offen und rot
        ev(&mut sys, &mut sun, klick(mitte(Teil::Uhrzeit)));
        let mut out = Ausgang::default();
        for c in "25".chars() {
            sys.key(Key::Char(c), m, sun, &mut out);
        }
        sys.key(Key::Enter, m, sun, &mut out);
        assert!(out.sun.is_none() && sys.eingabe.as_ref().is_some_and(|e| e.falsch));
        assert!(sys.key(Key::Escape, m, sun, &mut out) && sys.eingabe.is_none());
    }

    /// S6: Die Spitze sitzt an der oberen Ecke gegenüber der Sonne, ihr
    /// Schatten hat die Länge aus S5.
    #[test]
    fn schattenspitze_am_wuerfel() {
        let l = Location::default();
        let ecken = ecken(&wuerfel_netz().faces);
        assert_eq!(ecken.len(), 4, "{ecken:?}");
        let ecke = |h, m| {
            let s = sun(2026, 6, 21, h, m);
            griff(&l, &s, &ecken).unwrap().punkt
        };
        // Vormittag: Sonne im Südosten, Schatten nach Nordwesten
        assert_eq!(ecke(10, 0), vec3(0.0, WUERFEL, WUERFEL));
        // Nachmittag: Sonne im Südwesten, Schatten nach Nordosten
        assert_eq!(ecke(16, 0), vec3(WUERFEL, WUERFEL, WUERFEL));
        // Wahrer Mittag: 5,7 m nach Norden (±2 %)
        let s = sun(2026, 6, 21, 13, 0);
        let lage = bauort(&l);
        let t = sonne::hoechststand(lage, s.date);
        let mittag = Sun {
            minutes: t.in_ortszeit().minuten,
            ..s
        };
        let g = griff(&l, &mittag, &ecken).unwrap();
        let weit = g.boden.y - g.punkt.y;
        assert!((weit - 5700.0).abs() < 114.0, "{weit}");
        assert!((g.boden.x - g.punkt.x).abs() < 0.03 * weit);
        // Unter 2° kein Griff
        assert_eq!(griff(&l, &sun(2026, 12, 21, 16, 30), &ecken), None);
    }

    /// S6: Die Kurve beginnt und endet bei 2° Sonnenhöhe, die Zeit steigt.
    #[test]
    fn schattenkurve_von_2_bis_2_grad() {
        let l = Location::default();
        let p = vec3(0.0, WUERFEL, WUERFEL);
        for (m, d) in [(3, 21), (6, 21), (12, 21)] {
            let s = sun(2026, m, d, 12, 0);
            let k = schattenkurve(&l, &s, p);
            assert!(k.len() > 50, "{m}: {}", k.len());
            assert!(k.windows(2).all(|w| w[0].0 < w[1].0));
            for e in [k[0], k[k.len() - 1]] {
                let h = sonne::sonnenstand(bauort(&l), e.0)
                    .richtung_modell(0.0)
                    .z
                    .asin();
                assert!(
                    (h.to_degrees() - 2.0).abs() < 0.01,
                    "{m}: {}",
                    h.to_degrees()
                );
                // 10 m Höhe bei 2°: 286 m Schatten
                let lang = (e.1 - vec3(p.x, p.y, 0.0)).length();
                assert!((lang - 286_363.0).abs() < 600.0, "{m}: {lang}");
                assert_eq!(e.1.z, 0.0);
            }
        }
        // Ohne Tag über 2° keine Kurve
        let tromsoe = Location {
            lat: Some(69.65),
            lon: Some(18.96),
            ..Location::default()
        };
        assert!(schattenkurve(&tromsoe, &sun(2026, 12, 21, 12, 0), p).is_empty());
    }

    #[test]
    fn ueber_das_ende() {
        let pts = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)];
        assert_eq!(ueber_ende(&pts, (50.0, 3.0), 2.0, 24.0), Some(true));
        assert_eq!(ueber_ende(&pts, (40.0, 3.0), 2.0, 24.0), None);
        assert_eq!(ueber_ende(&pts, (-30.0, 0.0), 0.0, 24.0), Some(false));
        assert_eq!(ueber_ende(&pts, (-30.0, 0.0), 1.0, 24.0), None);
    }

    /// S6, Bedienung: Griff an der Spitze greifen, entlang der Kurve ziehen
    /// (die Uhrzeit folgt, hin und zurück dieselbe Minute), über die Enden
    /// hinaus Auf- und Untergang.
    #[test]
    fn ziehen_am_schatten() {
        let (fonts, theme) = (ohne_schrift(), Theme::dark());
        let (w, h) = (1600.0, 1000.0);
        let q = wuerfel_quader();
        let ziel = vec3(5000.0, 5000.0, 0.0);
        let cam = Camera::looking_at(ziel + vec3(0.0, -350_000.0, 900_000.0), ziel, 45.0);
        let l = Location::default();
        let ecken = ecken(&wuerfel_netz().faces);
        let mut sun = sun(2026, 6, 21, 10, 0);
        let mut sys = Sonnensystem::default();
        let m = Modifiers::default();
        let ev = |sys: &mut Sonnensystem, sun: &mut Sun, e: Event| {
            let hi = himmel(&l, sun, q);
            let g = griff(&l, sun, &ecken);
            let lb = Lagebild {
                sun: *sun,
                himmel: &hi,
                griff: g.as_ref(),
                cam: &cam,
                wh: (w, h),
                scale: 1.0,
                fonts: &fonts,
                theme: &theme,
            };
            let out = sys.handle(&e, &lb);
            if let Some(s) = out.sun {
                *sun = s;
            }
            out
        };
        let mv = |x: f64, y: f64| Event::MouseMove { x, y, mods: m };
        let px = |p: Vec3| cam.project(p, w, h).unwrap();
        let g = griff(&l, &sun, &ecken).unwrap();
        let (x, y) = px(g.boden);
        assert!(ev(&mut sys, &mut sun, mv(x, y)).redraw && sys.ueber_schatten);
        assert!(!sys.ueber_sonne);
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: m,
        };
        assert!(ev(&mut sys, &mut sun, down).consumed);
        assert!(sys.am_schatten().is_some_and(|z| z.punkt == g.punkt));
        // Entlang der Kurve zum Abend: die Zeit steigt; zurück dieselben
        // Minuten an denselben Punkten
        let k = &g.kurve;
        // Ab dem ersten Punkt jenseits der Ziehschwelle
        let ab = (stelle_bei(k, zeitpunkt(&sun)) as usize + 1..k.len())
            .find(|i| {
                let q = px(k[*i].1);
                (q.0 - x).hypot(q.1 - y) > 2.0 * ZUG_PX
            })
            .unwrap();
        let mut hin = Vec::new();
        for p in &k[ab..] {
            let (px, py) = px(p.1);
            ev(&mut sys, &mut sun, mv(px + 0.5, py + 0.5));
            assert!(
                hin.last().is_none_or(|v| sun.minutes >= *v),
                "{hin:?} {}",
                sun.minutes
            );
            hin.push(sun.minutes);
        }
        assert_eq!(sun.minutes, minuten_bei(k, k[k.len() - 1].0).unwrap());
        // Über das Ende hinaus: Untergang
        let hi = himmel(&l, &sun, q);
        let unter = minuten_bei(&hi.tag, hi.tag[hi.tag.len() - 1].0).unwrap();
        let (a, b) = (px(k[k.len() - 2].1), px(k[k.len() - 1].1));
        let r = ((b.0 - a.0), (b.1 - a.1));
        let n = r.0.hypot(r.1);
        let raus = (b.0 + r.0 / n * 60.0, b.1 + r.1 / n * 60.0);
        ev(&mut sys, &mut sun, mv(raus.0, raus.1));
        assert_eq!(sun.minutes, unter);
        // Der Griff bleibt dabei am Abendende der Kurve
        let st = schatten_striche(&sys, None, zur_sonne(&l, &sun));
        let griff_bei = st.iter().rev().find(|s| s.0 == s.1).map(|s| s.0);
        assert_eq!(griff_bei, Some(k[k.len() - 1].1));
        ev(&mut sys, &mut sun, mv(b.0, b.1));
        assert_eq!(sun.minutes, hin[hin.len() - 1]);
        let mut zurueck = Vec::new();
        for p in k[ab..].iter().rev() {
            let (px, py) = px(p.1);
            ev(&mut sys, &mut sun, mv(px + 0.5, py + 0.5));
            zurueck.push(sun.minutes);
        }
        zurueck.reverse();
        assert_eq!(hin, zurueck);
        // Bis zum Morgen und darüber hinaus: Aufgang
        for p in k[..ab].iter().rev() {
            let (px, py) = px(p.1);
            ev(&mut sys, &mut sun, mv(px, py));
        }
        assert_eq!(sun.minutes, minuten_bei(k, k[0].0).unwrap());
        let (a, b) = (px(k[1].1), px(k[0].1));
        let r = ((b.0 - a.0), (b.1 - a.1));
        let n = r.0.hypot(r.1);
        ev(
            &mut sys,
            &mut sun,
            mv(b.0 + r.0 / n * 60.0, b.1 + r.1 / n * 60.0),
        );
        assert_eq!(sun.minutes, minuten_bei(&hi.tag, hi.tag[0].0).unwrap());
        assert_eq!(sun.date, Datum::new(2026, 6, 21).unwrap());
        let up = Event::MouseUp {
            button: MouseButton::Left,
            x: 0.0,
            y: 0.0,
            mods: m,
        };
        ev(&mut sys, &mut sun, up);
        assert!(!sys.is_busy() && sys.am_schatten().is_none());
    }

    /// S6: Liegt das Ende der Kurve außerhalb des Fensters, bleibt der Zug
    /// am letzten sichtbaren Punkt stehen (§8 11:30).
    #[test]
    fn schatten_ende_ausserhalb() {
        let (fonts, theme) = (ohne_schrift(), Theme::dark());
        let (w, h) = (1200.0, 800.0);
        let q = wuerfel_quader();
        let ziel = vec3(5000.0, 5000.0, 0.0);
        let cam = Camera::looking_at(ziel + vec3(-20000.0, -45000.0, 40000.0), ziel, 45.0);
        let l = Location::default();
        let ecken = ecken(&wuerfel_netz().faces);
        let mut sun = sun(2026, 6, 21, 10, 0);
        let mut sys = Sonnensystem::default();
        let m = Modifiers::default();
        let ev = |sys: &mut Sonnensystem, sun: &mut Sun, e: Event| {
            let hi = himmel(&l, sun, q);
            let g = griff(&l, sun, &ecken);
            let lb = Lagebild {
                sun: *sun,
                himmel: &hi,
                griff: g.as_ref(),
                cam: &cam,
                wh: (w, h),
                scale: 1.0,
                fonts: &fonts,
                theme: &theme,
            };
            let out = sys.handle(&e, &lb);
            if let Some(s) = out.sun {
                *sun = s;
            }
        };
        let g = griff(&l, &sun, &ecken).unwrap();
        let im_bild = |p: Vec3| {
            cam.project(p, w, h)
                .filter(|q| (0.0..=w).contains(&q.0) && (0.0..=h).contains(&q.1))
        };
        let k = &g.kurve;
        let ab = stelle_bei(k, zeitpunkt(&sun)) as usize + 1;
        // Das Abendende liegt außerhalb, der letzte sichtbare Punkt davor
        assert!(im_bild(k[k.len() - 1].1).is_none());
        let letzt = (ab..k.len())
            .take_while(|i| im_bild(k[*i].1).is_some())
            .last()
            .unwrap();
        assert!(letzt + 1 < k.len());
        let (x, y) = im_bild(g.boden).unwrap();
        ev(
            &mut sys,
            &mut sun,
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: m,
            },
        );
        for p in &k[ab..=letzt] {
            let (px, py) = im_bild(p.1).unwrap();
            ev(
                &mut sys,
                &mut sun,
                Event::MouseMove {
                    x: px,
                    y: py,
                    mods: m,
                },
            );
        }
        let soll = minuten_bei(k, k[letzt].0).unwrap();
        assert_eq!(sun.minutes, soll);
        // Weiter in Richtung des Endes, auch aus dem Fenster heraus: es
        // bleibt beim letzten sichtbaren Punkt, kein Untergang
        let (a, b) = (
            im_bild(k[letzt - 1].1).unwrap(),
            im_bild(k[letzt].1).unwrap(),
        );
        let r = (b.0 - a.0, b.1 - a.1);
        let n = r.0.hypot(r.1);
        for d in [30.0, 300.0, 3000.0] {
            let (px, py) = (b.0 + r.0 / n * d, b.1 + r.1 / n * d);
            ev(
                &mut sys,
                &mut sun,
                Event::MouseMove {
                    x: px,
                    y: py,
                    mods: m,
                },
            );
            assert_eq!(sun.minutes, soll, "{d}");
        }
    }

    /// S9: Ohne Gebäude steht statt der Leiste nur der Hinweis, schmaler
    /// und mittig; Kachel und Pfeil schalten dann nicht an.
    #[test]
    fn ohne_gebaeude_nur_der_hinweis() {
        let (fonts, theme) = (ohne_schrift(), Theme::dark());
        let b = LeistenBild {
            sun: Sun {
                date: Datum::new(2026, 6, 21).unwrap(),
                minutes: 720,
                on: true,
            },
            unter: false,
            ohne_haus: true,
            eingabe: None,
            hover: None,
            vw: 1600,
            scale: 1f32.to_bits(),
        };
        let (c, x, y) = leiste_malen(&b, &fonts, &theme);
        let voll = LeistenBild {
            ohne_haus: false,
            ..b.clone()
        };
        let (v, vx, vy) = leiste_malen(&voll, &fonts, &theme);
        assert!(c.width < v.width, "{} {}", c.width, v.width);
        assert_eq!((c.height, y), (v.height, vy), "so hoch wie die Leiste");
        assert!((x * 2 + c.width as i32 - 1600).abs() <= 2, "mittig: {x}");
        assert!((vx * 2 + v.width as i32 - 1600).abs() <= 2);
        assert_eq!(umschalten(false, false), Err(OHNE_HAUS));
        assert_eq!(umschalten(false, true), Ok(true));
        assert_eq!(umschalten(true, true), Ok(false));
    }
}

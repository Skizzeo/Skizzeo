//! Schattenkarte der Sonne (Sonnenstand S5): Parallelprojektion aus
//! Sonnensicht, zugeschnitten auf den Hüllquader der Schattenwerfer.
//!
//! Jeder beschattete Punkt, auch der lange Bodenschatten am Abend, liegt in
//! Sonnensicht hinter dem Quader; die Karte braucht deshalb nur den Quader,
//! und ihre Auflösung hängt nicht vom Sonnenstand ab. Neben der Karte ist
//! es hell.
//!
//! Hier steht nur die Rechnung: die Matrix, die Regel „beschattet“ und ein
//! Tiefenbild in Software mit genau dieser Regel (Prüfbilder ohne GPU). Der
//! Renderer zeichnet dieselbe Karte auf der GPU und liest sie mit
//! [`SCHATTEN_GLSL`].

use sk_math::{vec3, Mat4, Vec3};

/// Kantenlänge der Karte in Texeln.
pub const GROESSE: u32 = 4096;
/// Kantenlänge beim Ziehen an Sonne oder Schatten, wenn der
/// Tiefen-Durchgang in voller Größe länger als [`ENTWURF_AB_MS`] dauert
/// (S6, §8 11:20).
pub const ENTWURF: u32 = 2048;
pub const ENTWURF_AB_MS: f64 = 8.0;

/// Unter dieser Sonnenhöhe (Grad) wirft nichts mehr Schatten.
pub const MIN_HOEHE: f64 = 2.0;

/// Abstand, um den ein Empfänger vor dem Lesen von seiner Fläche weg
/// (längs der Normalen) und zur Sonne hin rückt, in Texeln: gegen
/// Schattenakne auf besonnten Flächen.
pub const VERSATZ_NORMALE: f64 = 1.5;
pub const VERSATZ_SONNE: f64 = 1.0;

/// Karte für eine Sonnenrichtung.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Karte {
    /// Mitte des Hüllquaders (Modell, mm); die Matrix nimmt Punkte relativ
    /// dazu, damit sie in `f32` genau bleibt.
    pub mitte: Vec3,
    /// Vom Punkt relativ zu [`Karte::mitte`] nach x, y, z in −1 … 1 (z = 1 am
    /// weitesten von der Sonne).
    pub matrix: Mat4,
    /// Zur Sonne (Einheitsvektor).
    pub zur_sonne: Vec3,
    /// Kantenlänge eines Texels (mm), die größere der beiden Richtungen.
    pub texel: f64,
    pub groesse: u32,
}

/// Mindesthöhe [`MIN_HOEHE`] als Sinus.
pub fn min_sinus() -> f64 {
    MIN_HOEHE.to_radians().sin()
}

/// Karte für die Richtung `zur_sonne` und den Hüllquader `(lo, hi)` der
/// Schattenwerfer; `None`, wenn die Sonne tiefer als [`MIN_HOEHE`] steht.
pub fn karte(zur_sonne: Vec3, (lo, hi): (Vec3, Vec3), groesse: u32) -> Option<Karte> {
    let d = zur_sonne.normalized();
    if d.z.is_nan() || d.z < min_sinus() || groesse == 0 {
        return None;
    }
    let mitte = (lo + hi) * 0.5;
    // Blick von der Sonne; „oben“ in der Karte ist die Senkrechte, soweit
    // sie quer zum Blick steht (im Zenit die y-Achse)
    let f = d * -1.0;
    let senkrecht = vec3(0.0, 0.0, 1.0) - d * d.z;
    let oben = if senkrecht.length() > 1e-9 {
        senkrecht.normalized()
    } else {
        vec3(0.0, 1.0, 0.0)
    };
    let rechts = f.cross(oben).normalized();
    let oben = rechts.cross(f);
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    for i in 0..8 {
        let e = vec3(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        ) - mitte;
        for (k, a) in [rechts, oben, f].into_iter().enumerate() {
            let v = e.dot(a);
            min[k] = min[k].min(v);
            max[k] = max[k].max(v);
        }
    }
    // Zwei Texel Rand, damit das Weichzeichnen am Rand der Karte noch
    // Werfer sieht; in der Tiefe 1 % Luft
    let n = groesse as f64;
    let rand = |k: usize| (max[k] - min[k]).max(1.0) * 2.0 / (n - 4.0).max(1.0);
    let halb = |k: usize, r: f64| ((max[k] - min[k]) * 0.5 + r).max(0.5);
    let (hx, hy) = (halb(0, rand(0)), halb(1, rand(1)));
    let hz = halb(2, (max[2] - min[2]) * 0.01 + 1.0);
    let c = |k: usize| (min[k] + max[k]) * 0.5;
    let (cx, cy, cz) = (c(0), c(1), c(2));
    let (r, u) = (rechts, oben);
    let matrix = Mat4 {
        c: [
            [r.x / hx, u.x / hy, f.x / hz, 0.0],
            [r.y / hx, u.y / hy, f.y / hz, 0.0],
            [r.z / hx, u.z / hy, f.z / hz, 0.0],
            [-cx / hx, -cy / hy, -cz / hz, 1.0],
        ],
    };
    Some(Karte {
        mitte,
        matrix,
        zur_sonne: d,
        texel: 2.0 * hx.max(hy) / n,
        groesse,
    })
}

impl Karte {
    /// Punkt (Modell, mm) in der Karte: Texel-Koordinaten (0 … groesse,
    /// x nach rechts, y nach oben) und Tiefe 0 … 1 wie im Tiefenpuffer.
    pub fn abbild(&self, p: Vec3) -> (f64, f64, f64) {
        let q = p - self.mitte;
        let v = self.matrix.mul_vec4([q.x, q.y, q.z, 1.0]);
        let n = self.groesse as f64;
        (
            (v[0] * 0.5 + 0.5) * n,
            (v[1] * 0.5 + 0.5) * n,
            v[2] * 0.5 + 0.5,
        )
    }

    /// Wo ein Empfänger an der Stelle `p` mit der Normalen `n` die Karte
    /// liest: etwas von seiner Fläche weg und zur Sonne hin.
    pub fn lesepunkt(&self, p: Vec3, n: Vec3) -> Vec3 {
        p + n * (VERSATZ_NORMALE * self.texel) + self.zur_sonne * (VERSATZ_SONNE * self.texel)
    }
}

/// Tiefenbild der Karte in Software: dieselbe Rechnung wie auf der GPU,
/// für Prüfbilder und Tests.
#[derive(Clone, Debug)]
pub struct Tiefenbild {
    pub karte: Karte,
    /// Zeilen von unten (wie die GPU-Textur), 1.0 = frei.
    pub tiefe: Vec<f32>,
}

impl Tiefenbild {
    pub fn neu(karte: Karte) -> Tiefenbild {
        let n = karte.groesse as usize;
        Tiefenbild {
            karte,
            tiefe: vec![1.0; n * n],
        }
    }

    /// Ein Dreieck (Modell, mm) eintragen: je Texelmitte im Dreieck die
    /// kleinste Tiefe.
    pub fn dreieck(&mut self, a: Vec3, b: Vec3, c: Vec3) {
        let n = self.karte.groesse as i64;
        let [a, b, c] = [a, b, c].map(|p| self.karte.abbild(p));
        let flaeche = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if flaeche.abs() < 1e-12 {
            return;
        }
        let x0 = (a.0.min(b.0).min(c.0).floor() as i64).max(0);
        let x1 = (a.0.max(b.0).max(c.0).ceil() as i64).min(n - 1);
        let y0 = (a.1.min(b.1).min(c.1).floor() as i64).max(0);
        let y1 = (a.1.max(b.1).max(c.1).ceil() as i64).min(n - 1);
        let kante = |p: (f64, f64, f64), q: (f64, f64, f64), x: f64, y: f64| {
            ((q.0 - p.0) * (y - p.1) - (q.1 - p.1) * (x - p.0)) / flaeche
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let wa = kante(b, c, px, py);
                let wb = kante(c, a, px, py);
                let wc = 1.0 - wa - wb;
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                let z = (wa * a.2 + wb * b.2 + wc * c.2).clamp(0.0, 1.0) as f32;
                let i = (y * n + x) as usize;
                if z < self.tiefe[i] {
                    self.tiefe[i] = z;
                }
            }
        }
    }

    /// Alle Flächen eines Netzes im Format von [`crate::MeshData::faces`].
    pub fn netz(&mut self, faces: &[[f32; 9]]) {
        let p = |v: &[f32; 9]| vec3(v[0] as f64, v[1] as f64, v[2] as f64);
        for t in faces.chunks_exact(3) {
            self.dreieck(p(&t[0]), p(&t[1]), p(&t[2]));
        }
    }

    /// Wie viel Sonne an der Stelle `p` mit der Normalen `n` ankommt
    /// (0 … 1): 3 × 3 Nachbartexel, je hell, wenn der Lesepunkt nicht
    /// hinter dem eingetragenen Werfer liegt. Neben der Karte hell.
    pub fn sonne(&self, p: Vec3, n: Vec3) -> f64 {
        let k = &self.karte;
        let (x, y, z) = k.abbild(k.lesepunkt(p, n));
        let g = k.groesse as f64;
        if !(0.0..=g).contains(&x) || !(0.0..=g).contains(&y) {
            return 1.0;
        }
        let z = z.min(1.0) as f32;
        let m = k.groesse as i64 - 1;
        let (ix, iy) = (x.floor() as i64, y.floor() as i64);
        let mut hell = 0;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (tx, ty) = ((ix + dx).clamp(0, m), (iy + dy).clamp(0, m));
                if z <= self.tiefe[(ty * (m + 1) + tx) as usize] {
                    hell += 1;
                }
            }
        }
        hell as f64 / 9.0
    }
}

/// Dieselbe Regel wie [`Tiefenbild::sonne`] im Shader. Erwartet
/// `u_shadow_on`, `u_shadow` (Tiefentextur mit Vergleich „≤“, NEAREST),
/// `u_shadow_m` ([`Karte::matrix`]), `u_shadow_size`, `u_shadow_sun`
/// (zur Sonne) und `u_shadow_offset` (Texel in mm mal den Versätzen:
/// Normale, Sonne). `rel` ist der Punkt relativ zu [`Karte::mitte`].
pub const SCHATTEN_GLSL: &str = r#"
uniform int u_shadow_on;
uniform sampler2DShadow u_shadow;
uniform mat4 u_shadow_m;
uniform float u_shadow_size;
uniform vec3 u_shadow_sun;
uniform vec2 u_shadow_offset;
float sun_share(vec3 rel, vec3 n) {
    if (u_shadow_on == 0) return 1.0;
    vec3 p = rel + n * u_shadow_offset.x + u_shadow_sun * u_shadow_offset.y;
    vec4 q = u_shadow_m * vec4(p, 1.0);
    vec2 t = (q.xy * 0.5 + 0.5) * u_shadow_size;
    if (t.x < 0.0 || t.y < 0.0 || t.x > u_shadow_size || t.y > u_shadow_size) return 1.0;
    float z = min(q.z * 0.5 + 0.5, 1.0);
    vec2 i = floor(t);
    float lit = 0.0;
    for (int dy = -1; dy <= 1; dy++) {
        for (int dx = -1; dx <= 1; dx++) {
            vec2 c = clamp(i + vec2(dx, dy), vec2(0.0), vec2(u_shadow_size - 1.0));
            lit += texture(u_shadow, vec3((c + 0.5) / u_shadow_size, z));
        }
    }
    return lit / 9.0;
}
"#;

/// Ersatz für [`SCHATTEN_GLSL`], wenn der Treiber die Schatten nicht
/// übersetzt: überall Sonne.
pub const SCHATTEN_AUS_GLSL: &str = r#"
float sun_share(vec3 rel, vec3 n) {
    return 1.0;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const W: f64 = 10000.0;

    fn wuerfel() -> (Vec3, Vec3) {
        (vec3(0.0, 0.0, 0.0), vec3(W, W, W))
    }

    /// Richtung zur Sonne aus Höhe und Azimut (Grad, von Norden im
    /// Uhrzeigersinn; Norden = +y).
    fn sonne(hoehe: f64, azimut: f64) -> Vec3 {
        let (h, a) = (hoehe.to_radians(), azimut.to_radians());
        vec3(h.cos() * a.sin(), h.cos() * a.cos(), h.sin())
    }

    /// Die Flächen des Würfels als Dreiecke.
    fn wuerfel_dreiecke() -> Vec<[Vec3; 3]> {
        let e = |i: usize| {
            vec3(
                if i & 1 == 0 { 0.0 } else { W },
                if i & 2 == 0 { 0.0 } else { W },
                if i & 4 == 0 { 0.0 } else { W },
            )
        };
        let seiten = [
            [0, 1, 3, 2],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 3, 7, 5],
        ];
        seiten
            .iter()
            .flat_map(|s| [[e(s[0]), e(s[1]), e(s[2])], [e(s[0]), e(s[2]), e(s[3])]])
            .collect()
    }

    fn bild(d: Vec3, groesse: u32) -> Tiefenbild {
        let mut t = Tiefenbild::neu(karte(d, wuerfel(), groesse).unwrap());
        for [a, b, c] in wuerfel_dreiecke() {
            t.dreieck(a, b, c);
        }
        t
    }

    /// Der ganze Quader liegt in der Karte, Ecken innerhalb −1 … 1; ein
    /// Texel ist beim Würfel etwa 4 mm.
    #[test]
    fn quader_in_der_karte() {
        for (h, a) in [
            (60.0, 180.0),
            (12.5, 135.0),
            (3.0, 250.0),
            (89.9, 0.0),
            (90.0, 0.0),
        ] {
            let k = karte(sonne(h, a), wuerfel(), GROESSE).unwrap();
            for i in 0..8 {
                let e = vec3(
                    if i & 1 == 0 { 0.0 } else { W },
                    if i & 2 == 0 { 0.0 } else { W },
                    if i & 4 == 0 { 0.0 } else { W },
                );
                let (x, y, z) = k.abbild(e);
                let g = GROESSE as f64;
                assert!(
                    x > 1.0 && x < g - 1.0 && y > 1.0 && y < g - 1.0,
                    "{h}° {a}°"
                );
                assert!(z > 0.0 && z < 1.0, "{h}° {a}°: {z}");
            }
            assert!(k.texel < 4.3 && k.texel > 2.0, "{h}°: {}", k.texel);
        }
        // Näher an der Sonne heißt kleinere Tiefe
        let k = karte(sonne(30.0, 180.0), wuerfel(), GROESSE).unwrap();
        assert!(k.abbild(vec3(5000.0, 0.0, W)).2 < k.abbild(vec3(5000.0, W, 0.0)).2);
        assert_eq!(karte(sonne(1.9, 180.0), wuerfel(), GROESSE), None);
        assert!(karte(sonne(2.01, 180.0), wuerfel(), GROESSE).is_some());
    }

    /// Schattenlänge hinter dem Würfel: h / tan(Höhe), ±2 % (§5 S5); der
    /// Rand hinter der Mitte der Nordkante bei Sonne im Süden.
    #[test]
    fn schattenlaenge_am_wuerfel() {
        // Ganderkesee mittags (§7): 21.06. 60,3°, 21.12. 13,4°
        for (hoehe, soll) in [(60.3, 5700.0), (13.4, 42000.0), (37.5, 13000.0)] {
            let t = bild(sonne(hoehe, 180.0), 1024);
            let l = W / hoehe.to_radians().tan();
            assert!((l - soll).abs() / soll < 0.02, "{hoehe}°: {l}");
            let auf = vec3(0.0, 0.0, 1.0);
            let am_boden = |s: f64| t.sonne(vec3(W * 0.5, W + s, 0.0), auf);
            // Grenze in 1-cm-Schritten suchen
            let mut s = 0.0;
            while am_boden(s) < 0.5 {
                s += 10.0;
            }
            assert!((s - l).abs() / l < 0.02, "{hoehe}°: Schatten {s} statt {l}");
            assert_eq!(am_boden(l * 0.5), 0.0);
            assert_eq!(am_boden(l * 1.1), 1.0);
            // Seitlich neben dem Würfel und vor ihm (Süden) hell
            assert_eq!(t.sonne(vec3(-1000.0, W + l * 0.5, 0.0), auf), 1.0);
            assert_eq!(t.sonne(vec3(W * 0.5, -1000.0, 0.0), auf), 1.0);
        }
    }

    /// Die besonnten Seiten sind ohne Akne hell, die Rückseite liegt im
    /// Eigenschatten nicht unter ihrem Werfer; das Dach ist hell.
    #[test]
    fn keine_schattenakne() {
        let t = bild(sonne(25.0, 200.0), 2048);
        for i in 1..20 {
            for j in 1..20 {
                let (u, v) = (i as f64 * 500.0, j as f64 * 500.0);
                // Südseite (y = 0, Normale −y) und Dach
                assert_eq!(
                    t.sonne(vec3(u, 0.0, v), vec3(0.0, -1.0, 0.0)),
                    1.0,
                    "{u} {v}"
                );
                assert_eq!(t.sonne(vec3(u, v, W), vec3(0.0, 0.0, 1.0)), 1.0, "{u} {v}");
            }
        }
    }

    /// Eine Laibung von 15 cm schattet sichtbar aufs Fenster (§5 S5):
    /// Wand in der Ebene y = 0 (außen −y), Öffnung von x = 4 bis 5 m, Glas
    /// 15 cm zurück; die Sonne im Ostsüdosten wirft die rechte Laibung als
    /// Streifen von 150 · |dx / dy| aufs Glas.
    #[test]
    fn laibung_schattet() {
        let laibung = [
            vec3(5000.0, 0.0, 1000.0),
            vec3(5000.0, 150.0, 1000.0),
            vec3(5000.0, 150.0, 2500.0),
            vec3(5000.0, 0.0, 2500.0),
        ];
        let quader = (vec3(3000.0, 0.0, 0.0), vec3(6000.0, 300.0, 3000.0));
        let d = sonne(20.0, 120.0);
        let mut t = Tiefenbild::neu(karte(d, quader, GROESSE).unwrap());
        t.dreieck(laibung[0], laibung[1], laibung[2]);
        t.dreieck(laibung[0], laibung[2], laibung[3]);
        assert!(t.karte.texel < 1.1, "{}", t.karte.texel);
        let streifen = 150.0 * (d.x / d.y).abs();
        assert!((streifen - 260.0).abs() < 5.0, "{streifen}");
        let glas = |x: f64| t.sonne(vec3(x, 150.0, 1700.0), vec3(0.0, -1.0, 0.0));
        assert_eq!(glas(5000.0 - streifen * 0.5), 0.0);
        assert_eq!(glas(5000.0 - streifen + 20.0), 0.0);
        assert_eq!(glas(5000.0 - streifen - 20.0), 1.0);
        assert_eq!(glas(4500.0), 1.0);
    }
}

//! GPU-Darstellung der 3D-Ansicht über OpenGL 3.3.
//!
//! Ablauf je Bild: Himmel und Boden als Vollbild-Pass (ohne Tiefe), dann
//! Flächen, dann Kanten als bildschirmbreite Bänder, dann der Boden noch einmal
//! durchscheinend über allem, was unter ihm liegt (E11). Alles in
//! einen Mehrfachabtast-Puffer (MSAA), der anschließend ins Fenster kopiert wird.
//! Darüber kommt die selbst gezeichnete Oberfläche (Titelleiste) als Textur.

pub mod gl;

use gl::*;
use std::ffi::c_void;

/// Farben und Maße der 3D-Ansicht (Farbwerte 0..1, sRGB).
#[derive(Clone, Debug)]
pub struct Style {
    pub sky: Vec<(f32, [f32; 3])>,
    pub ground: [f32; 3],
    pub horizon_softness: f32,
    /// Deckkraft des Bodens über Modellteilen unter z = 0 (1 = deckend).
    pub ground_opacity: f32,
    /// Richtung zum Licht (Weltkoordinaten, normiert).
    pub light: [f32; 3],
    /// Helligkeit abgewandter Flächen (0..1); zugewandte Flächen erreichen 1.
    pub ambient: f32,
}

/// Höchstzahl der Kantenarten (Größe der Uniform-Felder).
pub const EDGE_KINDS: usize = 8;

/// Zeilen der Aussehens-Tabelle je Darstellungsschlüssel.
pub const LOOK_ROWS: usize = 8;

/// Breite (Bildpunkte) und Farbe je Kantenart, für Zeichnung oder 3D.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeLooks {
    pub width: [f32; EDGE_KINDS],
    pub color: [[f32; 3]; EDGE_KINDS],
    /// Strichmuster je Kantenart, zwei Einträge `[Strich px, Lücke px,
    /// Punkt 0/1, 0]` (Index `2·Art` und `2·Art + 1`); alles 0 = Volllinie.
    pub dash: [[f32; 4]; 2 * EDGE_KINDS],
}

impl Default for EdgeLooks {
    fn default() -> EdgeLooks {
        EdgeLooks {
            width: [0.0; EDGE_KINDS],
            color: [[0.0; 3]; EDGE_KINDS],
            dash: [[0.0; 4]; 2 * EDGE_KINDS],
        }
    }
}

/// Strichmuster einer Linie (siehe [`EdgeLooks::dash`]); alles 0 = Volllinie.
pub type DashPattern = [[f32; 4]; 2];

/// Volllinie.
pub const SOLID: DashPattern = [[0.0; 4]; 2];

/// Länge einer Periode des Musters in Bildpunkten bei Strichbreite `w` (ein
/// Punkt ist so lang wie die Linie breit).
pub fn dash_period(p: &DashPattern, w: f32) -> f32 {
    p.iter()
        .map(|e| e[0] + e[1] + if e[2] > 0.5 { w + e[1] } else { 0.0 })
        .sum()
}

/// Farbe an der Stelle `dist` (Bildpunkte ab Linienanfang) einer Linie der
/// Länge `len`? Dieselbe Regel wie `dash_ink` in den Shadern: kurze Linien
/// (kürzer als eine Periode) bleiben voll.
pub fn dash_ink(dist: f32, len: f32, p: &DashPattern, w: f32) -> bool {
    let period = dash_period(p, w);
    if period <= 0.0 || len < period {
        return true;
    }
    let mut m = dist.max(0.0).rem_euclid(period);
    let l0 = dash_period(&[p[0], [0.0; 4]], w);
    let e = if m >= l0 {
        m -= l0;
        p[1]
    } else {
        p[0]
    };
    if m < e[0] {
        return true;
    }
    m -= e[0] + e[1];
    e[2] > 0.5 && (0.0..w).contains(&m)
}

/// Aussehen aller Darstellungsschlüssel als Tabelle für die Grafikkarte.
///
/// `texels` hat [`LOOK_ROWS`] Zeilen zu je `keys` Texeln (Zeile für Zeile);
/// Spalte = Schlüssel ohne Schnittbit. Die Zeilen:
///
/// | Zeile | r g b | a |
/// |---|---|---|
/// | 0 | Ansichtsfläche 3D | – |
/// | 1 | Schnittfläche 3D | – |
/// | 2 | Grund in der Zeichnung | Art: 0 leer, 1 Vollfläche, 2 Linien, 3 Zickzack |
/// | 3 | Schraffurfarbe | Strichbreite px |
/// | 4 | Schar 1: cx, cy, Periode px | Abstandsfaktor k |
/// | 5 | Schar 2: cx, cy, Periode px | Abstandsfaktor k |
/// | 6 | Versatz Schar 1, Versatz Schar 2, Anzahl Scharen | Zickzack-Periode |
/// | 7 | Strich und Lücke Schar 1, Strich und Lücke Schar 2 (px, 0 = durchgezogen) | |
///
/// Eine Schar sind die Linien `cx·x + cy·y − Versatz = n·Periode` in
/// Bildpunkten; der Abstand eines Pixels zur nächsten Linie ist
/// `min(m, Periode − m)·k`. Gestrichelt wird längs der Linie
/// (`(−x·cy + y·cx)·k`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Looks {
    pub keys: usize,
    pub texels: Vec<[f32; 4]>,
    pub drawing: EdgeLooks,
    pub model: EdgeLooks,
}

/// Schraffur einer Schnittfläche in der Zeichnung an einem Bildpunkt, mit
/// derselben Formel wie `FACE_FS` (für Vorschaubilder ohne Grafikkarte).
/// `rows` sind die [`LOOK_ROWS`] Texel eines Schlüssels, `(x, y)` die Lage in
/// Bildpunkten (y nach oben wie `gl_FragCoord`), `uv` die Schichtkoordinaten
/// für das Zickzack (längs, quer 0..1) und `uv_px` ihre Änderung je Bildpunkt.
pub fn fill_color(
    rows: &[[f32; 4]; LOOK_ROWS],
    x: f32,
    y: f32,
    uv: [f32; 2],
    uv_px: [f32; 2],
) -> [f32; 3] {
    let bg = rows[2];
    let fg = rows[3];
    let mut c = [bg[0], bg[1], bg[2]];
    let kind = (bg[3] + 0.5) as i32;
    let ink = match kind {
        1 => 1.0,
        2 => {
            let (o, d) = (rows[6], rows[7]);
            let mut ink = ink_of(rows[4], o[0], fg[3], [d[0], d[1]], x, y);
            if o[2] > 1.5 {
                ink = ink.max(ink_of(rows[5], o[1], fg[3], [d[2], d[3]], x, y));
            }
            ink
        }
        3 => {
            let zig = (2.0 * fract(uv[0] / rows[6][3]) - 1.0).abs();
            let f = uv[1] - zig;
            // |∇f| wie dFdx/dFdy: längs ändert sich zig mit 2/Periode
            let g = (uv_px[1].powi(2) + (2.0 * uv_px[0] / rows[6][3]).powi(2))
                .sqrt()
                .max(1e-6);
            (fg[3] * 0.5 + 0.5 - f.abs() / g).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    for k in 0..3 {
        c[k] += (fg[k] - c[k]) * ink;
    }
    c
}

fn fract(v: f32) -> f32 {
    v - v.floor()
}

/// GLSL `mod`: Rest mit dem Vorzeichen des Teilers.
fn modulo(a: f32, b: f32) -> f32 {
    a - b * (a / b).floor()
}

/// `family` und `ink_of` aus `FACE_FS`.
fn ink_of(f: [f32; 4], offset: f32, w: f32, dash: [f32; 2], x: f32, y: f32) -> f32 {
    let m = modulo(x * f[0] + y * f[1] - offset, f[2]);
    let dist = m.min(f[2] - m) * f[3];
    let mut ink = (w * 0.5 + 0.5 - dist).clamp(0.0, 1.0);
    if dash[0] > 0.0 {
        let along = (-x * f[1] + y * f[0]) * f[3];
        let p = dash[0] + dash[1];
        let a = modulo(along, p);
        let s = if a <= dash[0] {
            a.min(dash[0] - a)
        } else {
            -(a - dash[0]).min(p - a)
        };
        ink *= (0.5 + s).clamp(0.0, 1.0);
    }
    ink
}

/// Kameradaten für ein Bild. Alle Matrizen sind kamerarelativ (Auge im Ursprung),
/// damit auch weit vom Nullpunkt entfernte Modelle in `f32` genau bleiben.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub view_proj: [f32; 16],
    pub inv_view_proj: [f32; 16],
    /// Lage des Modellursprungs relativ zum Auge.
    pub origin_rel: [f32; 3],
    /// Höhe des Auges über dem Boden (z = 0).
    pub eye_z: f32,
    /// Horizontlinie in Pixeln, von unten gezählt.
    pub horizon_px: f32,
    /// Ebene, an der Kanten vor dem Auge abgeschnitten werden (Abstand).
    pub near: f32,
    /// Verdeckbare Hilfslinien zur Kamera ziehen: Position * w + xyz (kamerarelativ).
    pub pull: [f32; 4],
    /// Zeichnungsdarstellung: einfarbiger Papiergrund statt Himmel und Boden,
    /// Flächen ohne Schattierung.
    pub paper: Option<[f32; 3]>,
}

/// Dreiecksnetz für die GPU.
/// Flächen: Position, Normale, Darstellungsschlüssel, Musterkoordinaten (u, v).
/// Kanten: zwei Punkte und Kantenart. Farben, Schraffuren und Strichbreiten
/// kommen aus der Tabelle [`Looks`], nicht aus dem Netz.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub faces: Vec<[f32; 9]>,
    pub edges: Vec<([[f32; 3]; 2], f32)>,
}

struct Program {
    id: GLuint,
}

/// Vertex-Array mit eigenem Puffer.
#[derive(Default)]
struct GpuBuffer {
    vao: GLuint,
    buf: GLuint,
    count: i32,
}

#[derive(Default)]
struct GpuMesh {
    faces: GpuBuffer,
    edges: GpuBuffer,
}

/// Hilfslinie oder Markierung, immer sichtbar.
/// Ein Punkt (`a == b`) wird als Quadrat mit Kantenlänge `width` gezeichnet.
#[derive(Clone, Copy, Debug)]
pub struct Helper {
    pub a: [f32; 3],
    pub b: [f32; 3],
    /// RGBA, 0..1, nicht vormultipliziert.
    pub color: [f32; 4],
    /// Breite in Pixeln.
    pub width: f32,
    /// Gestrichelt (Strichlänge gleich Lücke, in Pixeln), 0 = durchgezogen.
    pub dash: f32,
    /// Strichmuster aus den Attributen ([`SOLID`] = keins), z. B. die
    /// Schnittlinie A–A.
    pub pattern: DashPattern,
    /// Hinter Geometrie liegende Teile nur blass zeigen (sonst immer obenauf).
    pub occlude: bool,
    /// Runde Enden; mit `a == b` ein runder Punkt vom Durchmesser `width`.
    pub round: bool,
}

/// Bild der Oberfläche an einer Stelle im Fenster.
struct Overlay {
    tex: GLuint,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

struct Target {
    fbo: GLuint,
    color: GLuint,
    depth: GLuint,
    width: i32,
    height: i32,
}

pub struct Renderer {
    gl: Gl,
    sky: Program,
    faces: Program,
    edges: Program,
    overlay: Program,
    helpers: Program,
    empty_vao: GLuint,
    meshes: Vec<GpuMesh>,
    helper_mesh: GpuBuffer,
    overlays: Vec<Overlay>,
    samples: i32,
    target: Option<Target>,
    style: Style,
    looks_tex: GLuint,
    looks: Looks,
}

const SKY_MAX: usize = 16;

const FULLSCREEN_VS: &str = r#"#version 330 core
out vec2 v_ndc;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_ndc = p * 2.0 - 1.0;
    gl_Position = vec4(v_ndc, 0.0, 1.0);
}
"#;

const SKY_FS: &str = r#"#version 330 core
in vec2 v_ndc;
out vec4 o_color;
uniform mat4 u_inv_vp;
uniform mat4 u_vp;
uniform float u_eye_z;
uniform float u_horizon_px;
uniform float u_height;
uniform float u_softness;
uniform vec3 u_ground;
uniform int u_sky_n;
uniform float u_sky_pos[16];
uniform vec3 u_sky_col[16];
// 0: Hintergrund ohne Tiefe; 1: Bodenschicht über dem Modell (E11)
uniform int u_overlay;
uniform float u_opacity;
// Absenkung der Bodenschicht (mm)
const float GROUND_SINK = 1.0;

vec3 sky(float d) {
    for (int i = 1; i < u_sky_n; i++) {
        if (d <= u_sky_pos[i]) {
            float t = (d - u_sky_pos[i - 1]) / (u_sky_pos[i] - u_sky_pos[i - 1]);
            return mix(u_sky_col[i - 1], u_sky_col[i], clamp(t, 0.0, 1.0));
        }
    }
    return u_sky_col[u_sky_n - 1];
}

void main() {
    // Strahl vom Nah- zum Fernpunkt (gilt für Perspektive und Parallelprojektion)
    vec4 n4 = u_inv_vp * vec4(v_ndc, -1.0, 1.0);
    vec4 f4 = u_inv_vp * vec4(v_ndc, 1.0, 1.0);
    vec3 o = n4.xyz / n4.w;
    vec3 dir = normalize(f4.xyz / f4.w - o);
    float above = gl_FragCoord.y - u_horizon_px;
    vec3 col = sky(abs(above) / u_height);
    float depth = 1.0;
    float z0 = u_eye_z + o.z;
    float t = abs(dir.z) > 1e-6 ? -z0 / dir.z : -1.0;
    float g = 1.0 - exp(-u_softness * abs(above));
    if (t > 0.0) {
        vec4 c = u_vp * vec4(o + dir * t, 1.0);
        depth = clamp(c.z / c.w * 0.5 + 0.5, 0.0, 1.0);
        col = mix(col, u_ground, g);
    } else if (abs(dir.z) <= 1e-6 && z0 < 0.0) {
        // Waagerechte Parallelansicht unterhalb des Bodens
        col = mix(col, u_ground, g);
    }
    if (u_overlay == 0) {
        o_color = vec4(col, 1.0);
        gl_FragDepth = 1.0;
        return;
    }
    if (t <= 0.0) {
        discard;
    }
    // Genau der Wert, der im Hintergrund steht: über leerem Boden bleibt das
    // Bild beim Mischen gleich
    o_color = vec4(floor(col * 255.0 + 0.5) / 255.0, u_opacity);
    // Tiefe der um GROUND_SINK abgesenkten Ebene: Linien und Flächen genau
    // auf ±0,00 (Wandfuß) bleiben ungetönt
    float ts = -(z0 + GROUND_SINK) / dir.z;
    vec4 cs = u_vp * vec4(o + dir * ts, 1.0);
    gl_FragDepth = ts > 0.0 ? clamp(cs.z / cs.w * 0.5 + 0.5, 0.0, 1.0) : depth;
}
"#;

const FACE_VS: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in float a_key;
layout(location = 3) in vec2 a_uv;
uniform mat4 u_vp;
uniform vec3 u_origin;
out vec3 v_normal;
flat out int v_key;
out vec2 v_uv;
void main() {
    v_normal = a_normal;
    v_key = int(a_key + 0.5);
    v_uv = a_uv;
    gl_Position = u_vp * vec4(a_pos + u_origin, 1.0);
}
"#;

const FACE_FS: &str = r#"#version 330 core
in vec3 v_normal;
flat in int v_key;
in vec2 v_uv;
out vec4 o_color;
uniform sampler2D u_looks;
uniform int u_drawing;
uniform vec3 u_light;
uniform float u_ambient;
vec4 look(int row) {
    return texelFetch(u_looks, ivec2(v_key & 0x7FFF, row), 0);
}
// Abstand des Pixels zur nächsten Linie einer Schar (Zeile 4 oder 5)
float family(vec4 f, float offset) {
    float m = mod(gl_FragCoord.x * f.x + gl_FragCoord.y * f.y - offset, f.z);
    return min(m, f.z - m) * f.w;
}
// Tinte einer Schar, gestrichelt mit Strich und Lücke (px, 0 = durchgezogen)
float ink_of(vec4 f, float offset, float w, vec2 dash) {
    float ink = clamp(w * 0.5 + 0.5 - family(f, offset), 0.0, 1.0);
    if (dash.x > 0.0) {
        float along = (-gl_FragCoord.x * f.y + gl_FragCoord.y * f.x) * f.w;
        float p = dash.x + dash.y;
        float a = mod(along, p);
        float s = a <= dash.x ? min(a, dash.x - a) : -min(a - dash.x, p - a);
        ink *= clamp(0.5 + s, 0.0, 1.0);
    }
    return ink;
}
void main() {
    bool cut = (v_key & 0x8000) != 0;
    if (u_drawing == 0) {
        vec3 c = look(cut ? 1 : 0).rgb;
        float d = max(dot(normalize(v_normal), u_light), 0.0);
        o_color = vec4(c * (u_ambient + (1.0 - u_ambient) * d), 1.0);
        return;
    }
    vec4 bg = look(2);
    vec3 c = bg.rgb;
    if (cut) {
        int kind = int(bg.a + 0.5);
        vec4 fg = look(3);
        float ink = 0.0;
        if (kind == 1) {
            c = fg.rgb;
        } else if (kind == 2) {
            vec4 o = look(6);
            vec4 d = look(7);
            ink = ink_of(look(4), o.x, fg.a, d.xy);
            if (o.z > 1.5) {
                ink = max(ink, ink_of(look(5), o.y, fg.a, d.zw));
            }
        } else if (kind == 3) {
            // Zickzack zwischen den Schichtflächen (v = 0 und v = 1)
            float zig = abs(2.0 * fract(v_uv.x / look(6).a) - 1.0);
            float f = v_uv.y - zig;
            float g = max(length(vec2(dFdx(f), dFdy(f))), 1e-6);
            ink = clamp(fg.a * 0.5 + 0.5 - abs(f) / g, 0.0, 1.0);
        }
        c = mix(c, fg.rgb, ink);
    }
    o_color = vec4(c, 1.0);
}
"#;

const DASH_GLSL: &str = r#"
// Strichmuster (E4): zwei Einträge (Strich, Lücke, Punkt 0/1) in Bildpunkten;
// ein Punkt ist so lang wie die Linie breit. Linien kürzer als eine Periode
// bleiben voll. Gleiche Regel wie `dash_ink` in Rust.
bool dash_ink(float dist, float len, vec4 p0, vec4 p1, float w) {
    float l0 = p0.x + p0.y + (p0.z > 0.5 ? w + p0.y : 0.0);
    float l1 = p1.x + p1.y + (p1.z > 0.5 ? w + p1.y : 0.0);
    float period = l0 + l1;
    if (period <= 0.0 || len < period) return true;
    float m = mod(max(dist, 0.0), period);
    vec4 e = p0;
    if (m >= l0) { m -= l0; e = p1; }
    if (m < e.x) return true;
    m -= e.x + e.y;
    return e.z > 0.5 && m >= 0.0 && m < w;
}
"#;

const EDGE_VS: &str = r#"#version 330 core
// Eine Instanz je Kante; die sechs Ecken der beiden Dreiecke kommen aus gl_VertexID.
layout(location = 0) in vec3 a_a;
layout(location = 1) in vec3 a_b;
layout(location = 2) in float a_kind;
const vec2 CORNERS[6] = vec2[6](
    vec2(0.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
    vec2(0.0, -1.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
uniform mat4 u_vp;
uniform vec3 u_origin;
uniform vec2 u_viewport;
uniform float u_edge_width[8];
uniform vec3 u_edge_color[8];
uniform vec4 u_edge_dash[16];
uniform float u_near;
flat out vec3 v_color;
// Strichmuster: Lage längs der Kante (px ab Anfang), Länge, Breite
noperspective out float v_dist;
flat out vec4 v_p0;
flat out vec4 v_p1;
flat out vec2 v_len_w;
void main() {
    int k = clamp(int(a_kind + 0.5), 0, 7);
    v_color = u_edge_color[k];
    v_p0 = u_edge_dash[2 * k];
    v_p1 = u_edge_dash[2 * k + 1];
    vec4 ca = u_vp * vec4(a_a + u_origin, 1.0);
    vec4 cb = u_vp * vec4(a_b + u_origin, 1.0);
    if (ca.w < u_near && cb.w < u_near) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        return;
    }
    if (ca.w < u_near) ca = mix(ca, cb, (u_near - ca.w) / (cb.w - ca.w));
    if (cb.w < u_near) cb = mix(cb, ca, (u_near - cb.w) / (ca.w - cb.w));
    vec2 half_vp = u_viewport * 0.5;
    vec2 sa = ca.xy / ca.w * half_vp;
    vec2 sb = cb.xy / cb.w * half_vp;
    vec2 d = sb - sa;
    float len = length(d);
    d = len > 1e-6 ? d / len : vec2(1.0, 0.0);
    vec2 n = vec2(-d.y, d.x);
    vec2 a_corner = CORNERS[gl_VertexID];
    bool at_b = a_corner.x > 0.5;
    vec4 c = at_b ? cb : ca;
    float w = u_edge_width[k];
    vec2 off = (n * a_corner.y + d * (at_b ? 1.0 : -1.0)) * (w * 0.5);
    v_dist = at_b ? len + w * 0.5 : -w * 0.5;
    v_len_w = vec2(len, w);
    c.xy += off / half_vp * c.w;
    gl_Position = c;
}
"#;

const EDGE_FS: &str = r#"
flat in vec3 v_color;
noperspective in float v_dist;
flat in vec4 v_p0;
flat in vec4 v_p1;
flat in vec2 v_len_w;
out vec4 o_color;
void main() {
    if (v_p0.x + v_p0.y > 0.0 && !dash_ink(v_dist, v_len_w.x, v_p0, v_p1, v_len_w.y)) discard;
    o_color = vec4(v_color, 1.0);
}
"#;

const HELPER_VS: &str = r#"#version 330 core
layout(location = 0) in vec3 a_a;
layout(location = 1) in vec3 a_b;
layout(location = 2) in vec2 a_corner;
layout(location = 3) in vec4 a_color;
layout(location = 4) in vec3 a_style;
layout(location = 5) in vec4 a_pat0;
layout(location = 6) in vec4 a_pat1;
uniform mat4 u_vp;
uniform vec3 u_origin;
uniform vec2 u_viewport;
uniform float u_near;
uniform vec4 u_pull;
out vec4 v_color;
noperspective out float v_dist;
flat out float v_dash;
flat out vec4 v_p0;
flat out vec4 v_p1;
// Lage im Strich in Pixeln (längs ab Anfang, quer ab Mitte), für runde Enden
noperspective out vec2 v_cap;
flat out vec3 v_round;
void main() {
    // Verdeckbare Linien ein wenig zur Kamera ziehen, damit sie nicht mit
    // der Fläche, auf der sie liegen, um die Tiefe kämpfen
    bool occl = mod(a_style.z, 2.0) > 0.5;
    vec4 pull = occl ? u_pull : vec4(0.0, 0.0, 0.0, 1.0);
    vec4 ca = u_vp * vec4((a_a + u_origin) * pull.w + pull.xyz, 1.0);
    vec4 cb = u_vp * vec4((a_b + u_origin) * pull.w + pull.xyz, 1.0);
    v_color = a_color;
    v_dash = a_style.y;
    v_p0 = a_pat0;
    v_p1 = a_pat1;
    if (ca.w < u_near && cb.w < u_near) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        v_dist = 0.0;
        return;
    }
    if (ca.w < u_near) ca = mix(ca, cb, (u_near - ca.w) / (cb.w - ca.w));
    if (cb.w < u_near) cb = mix(cb, ca, (u_near - cb.w) / (ca.w - cb.w));
    vec2 half_vp = u_viewport * 0.5;
    vec2 sa = ca.xy / ca.w * half_vp;
    vec2 sb = cb.xy / cb.w * half_vp;
    vec2 d = sb - sa;
    float len = length(d);
    d = len > 1e-6 ? d / len : vec2(1.0, 0.0);
    vec2 n = vec2(-d.y, d.x);
    bool at_b = a_corner.x > 0.5;
    vec4 c = at_b ? cb : ca;
    float w = a_style.x;
    vec2 off = (n * a_corner.y + d * (at_b ? 1.0 : -1.0)) * (w * 0.5);
    v_dist = at_b ? len : 0.0;
    v_cap = vec2(at_b ? len + w * 0.5 : -w * 0.5, a_corner.y * w * 0.5);
    v_round = vec3(a_style.z > 1.5 ? 1.0 : 0.0, len, w * 0.5);
    c.xy += off / half_vp * c.w;
    // Nicht verdeckbar: ganz vorne (Tiefe 0)
    if (!occl) c.z = -c.w;
    gl_Position = c;
}
"#;

const HELPER_FS: &str = r#"
in vec4 v_color;
noperspective in float v_dist;
flat in float v_dash;
flat in vec4 v_p0;
flat in vec4 v_p1;
noperspective in vec2 v_cap;
flat in vec3 v_round;
uniform float u_hidden;
out vec4 o_color;
void main() {
    if (v_dash > 0.0 && mod(v_dist, 2.0 * v_dash) > v_dash) discard;
    // Strichmuster aus den Attributen (Schnittlinie A–A)
    if (v_p0.x + v_p0.y > 0.0 && !dash_ink(v_dist, v_round.y, v_p0, v_p1, 2.0 * v_round.z)) discard;
    float a = v_color.a * (u_hidden > 0.5 ? 0.3 : 1.0);
    if (v_round.x > 0.5) {
        // Runde Enden: Abstand zur Mittelstrecke, weicher Rand
        float x = v_cap.x < 0.0 ? v_cap.x : max(v_cap.x - v_round.y, 0.0);
        float cov = clamp(v_round.z - length(vec2(x, v_cap.y)) + 0.5, 0.0, 1.0);
        if (cov <= 0.0) discard;
        a *= cov;
    }
    o_color = vec4(v_color.rgb * a, a);
}
"#;

const OVERLAY_FS: &str = r#"#version 330 core
in vec2 v_ndc;
out vec4 o_color;
uniform sampler2D u_tex;
void main() {
    vec2 uv = vec2(v_ndc.x * 0.5 + 0.5, 0.5 - v_ndc.y * 0.5);
    o_color = texture(u_tex, uv);
}
"#;

impl Renderer {
    pub fn new(gl: Gl, style: Style) -> Result<Renderer, String> {
        unsafe {
            let sky = program(&gl, FULLSCREEN_VS, SKY_FS)?;
            let faces = program(&gl, FACE_VS, FACE_FS)?;
            let edges = program(&gl, EDGE_VS, &with_dash(EDGE_FS))?;
            let overlay = program(&gl, FULLSCREEN_VS, OVERLAY_FS)?;
            let helpers = program(&gl, HELPER_VS, &with_dash(HELPER_FS))?;
            let mut vao = 0u32;
            gl.glGenVertexArrays(1, &mut vao);
            let mut max_samples = 0;
            gl.glGetIntegerv(MAX_SAMPLES, &mut max_samples);
            Ok(Renderer {
                gl,
                sky,
                faces,
                edges,
                overlay,
                helpers,
                empty_vao: vao,
                meshes: Vec::new(),
                helper_mesh: GpuBuffer::default(),
                overlays: Vec::new(),
                samples: max_samples.clamp(1, 8),
                target: None,
                style,
                looks_tex: 0,
                looks: Looks::default(),
            })
        }
    }

    /// Treiberangaben für Fehlermeldungen und Protokoll.
    pub fn driver_info(&self) -> String {
        unsafe {
            let s = |n| {
                let p = self.gl.glGetString(n);
                if p.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(p as *const std::ffi::c_char)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            format!("{} / {}", s(RENDERER), s(VERSION))
        }
    }

    /// Lädt die Aussehens-Tabelle (Texel und Kantenstile) hoch. Netze bleiben
    /// unberührt: eine Änderung an Stift, Schraffur oder Oberfläche kostet nur
    /// diese wenigen Kilobyte.
    pub fn set_looks(&mut self, looks: &Looks) {
        if *looks == self.looks {
            return;
        }
        let gl = &self.gl;
        let keys = looks.keys.max(1);
        let mut texels = looks.texels.clone();
        texels.resize(keys * LOOK_ROWS, [0.0; 4]);
        unsafe {
            if self.looks_tex == 0 {
                gl.glGenTextures(1, &mut self.looks_tex);
                gl.glBindTexture(TEXTURE_2D, self.looks_tex);
                for (p, v) in [
                    (TEXTURE_MIN_FILTER, NEAREST),
                    (TEXTURE_MAG_FILTER, NEAREST),
                    (TEXTURE_WRAP_S, CLAMP_TO_EDGE),
                    (TEXTURE_WRAP_T, CLAMP_TO_EDGE),
                ] {
                    gl.glTexParameteri(TEXTURE_2D, p, v);
                }
            }
            gl.glBindTexture(TEXTURE_2D, self.looks_tex);
            gl.glPixelStorei(UNPACK_ALIGNMENT, 4);
            gl.glTexImage2D(
                TEXTURE_2D,
                0,
                RGBA32F as GLint,
                keys as i32,
                LOOK_ROWS as i32,
                0,
                RGBA,
                FLOAT,
                texels.as_ptr() as *const c_void,
            );
            gl.glBindTexture(TEXTURE_2D, 0);
        }
        self.looks = looks.clone();
    }

    /// Ersetzt das Netz in Platz `slot` (z. B. 0 = Modell, 1 = Vorschau).
    pub fn set_mesh(&mut self, slot: usize, mesh: &MeshData) {
        while self.meshes.len() <= slot {
            self.meshes.push(GpuMesh::default());
        }
        let gl = &self.gl;
        let gm = &mut self.meshes[slot];
        unsafe {
            fill(
                gl,
                &mut gm.faces,
                &mesh.faces,
                &[(3, 0), (3, 12), (1, 24), (2, 28)],
            );

            // Eine Instanz je Kante (28 Byte); der Vertex-Shader zieht sie zu zwei
            // Dreiecken auf die Breite ihrer Kantenart auf.
            let v: Vec<[f32; 7]> = mesh
                .edges
                .iter()
                .map(|([a, b], wf)| [a[0], a[1], a[2], b[0], b[1], b[2], *wf])
                .collect();
            fill_with(gl, &mut gm.edges, &v, &[(3, 0), (3, 12), (1, 24)], 1);
        }
    }

    /// Hilfslinien und Markierungen für das nächste Bild.
    pub fn set_helpers(&mut self, helpers: &[Helper]) {
        let mut v: Vec<[f32; 23]> = Vec::with_capacity(helpers.len() * 6);
        for h in helpers {
            let [p0, p1] = h.pattern;
            for c in CORNERS {
                v.push([
                    h.a[0],
                    h.a[1],
                    h.a[2],
                    h.b[0],
                    h.b[1],
                    h.b[2],
                    c[0],
                    c[1],
                    h.color[0],
                    h.color[1],
                    h.color[2],
                    h.color[3],
                    h.width,
                    h.dash,
                    h.occlude as u8 as f32 + 2.0 * h.round as u8 as f32,
                    p0[0],
                    p0[1],
                    p0[2],
                    p0[3],
                    p1[0],
                    p1[1],
                    p1[2],
                    p1[3],
                ]);
            }
        }
        unsafe {
            fill(
                &self.gl,
                &mut self.helper_mesh,
                &v,
                &[(3, 0), (3, 12), (2, 24), (4, 32), (3, 48), (4, 60), (4, 76)],
            );
        }
    }

    /// Oberflächenbild Nummer `slot` an Fensterposition `(x, y)` (links oben, Pixel),
    /// vormultipliziertes RGBA8, Zeilen von oben. Breite 0 blendet es aus.
    pub fn set_overlay(
        &mut self,
        slot: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        rgba_premul: &[u8],
    ) {
        let gl = &self.gl;
        while self.overlays.len() <= slot {
            let mut tex = 0;
            unsafe { gl.glGenTextures(1, &mut tex) };
            self.overlays.push(Overlay {
                tex,
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            });
        }
        let o = &mut self.overlays[slot];
        (o.x, o.y, o.w, o.h) = (x, y, width as i32, height as i32);
        if width == 0 || height == 0 {
            return;
        }
        unsafe {
            gl.glBindTexture(TEXTURE_2D, o.tex);
            gl.glPixelStorei(UNPACK_ALIGNMENT, 1);
            gl.glTexImage2D(
                TEXTURE_2D,
                0,
                RGBA8 as GLint,
                width as i32,
                height as i32,
                0,
                RGBA,
                UNSIGNED_BYTE,
                rgba_premul.as_ptr() as *const c_void,
            );
            gl.glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST);
            gl.glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST);
            gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE);
            gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE);
        }
    }

    /// Einfarbige Fläche als Oberflächenbild (etwa das Abdunkeln hinter einem
    /// Dialog): ein Bildpunkt, auf `width` × `height` gestreckt.
    pub fn set_overlay_fill(
        &mut self,
        slot: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        rgba_premul: [u8; 4],
    ) {
        self.set_overlay(slot, x, y, 1, 1, &rgba_premul);
        let o = &mut self.overlays[slot];
        (o.w, o.h) = (width as i32, height as i32);
    }

    /// Ersetzt einen Ausschnitt eines schon hochgeladenen Oberflächenbildes:
    /// `(x, y)` links oben im Bild, vormultipliziertes RGBA8. Ein Ausschnitt,
    /// der nicht ganz im Bild liegt, wird verworfen.
    pub fn update_overlay(
        &mut self,
        slot: usize,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        rgba_premul: &[u8],
    ) {
        let Some(o) = self.overlays.get(slot) else {
            return;
        };
        let (w, h) = (width as i32, height as i32);
        if w <= 0 || h <= 0 || x < 0 || y < 0 || x + w > o.w || y + h > o.h {
            return;
        }
        if rgba_premul.len() < (w * h * 4) as usize {
            return;
        }
        let gl = &self.gl;
        unsafe {
            gl.glBindTexture(TEXTURE_2D, o.tex);
            gl.glPixelStorei(UNPACK_ALIGNMENT, 1);
            gl.glTexSubImage2D(
                TEXTURE_2D,
                0,
                x,
                y,
                w,
                h,
                RGBA,
                UNSIGNED_BYTE,
                rgba_premul.as_ptr() as *const c_void,
            );
        }
    }

    /// Verschiebt ein schon hochgeladenes Oberflächenbild, ohne es neu zu übertragen.
    pub fn move_overlay(&mut self, slot: usize, x: i32, y: i32) {
        if let Some(o) = self.overlays.get_mut(slot) {
            (o.x, o.y) = (x, y);
        }
    }

    fn ensure_target(&mut self, w: i32, h: i32) -> Result<(), String> {
        if let Some(t) = &self.target {
            if t.width == w && t.height == h {
                return Ok(());
            }
        }
        let gl = &self.gl;
        unsafe {
            if let Some(t) = self.target.take() {
                gl.glDeleteFramebuffers(1, &t.fbo);
                gl.glDeleteRenderbuffers(1, &t.color);
                gl.glDeleteRenderbuffers(1, &t.depth);
            }
            let (mut fbo, mut rb) = (0, [0u32; 2]);
            gl.glGenFramebuffers(1, &mut fbo);
            gl.glGenRenderbuffers(2, rb.as_mut_ptr());
            gl.glBindFramebuffer(FRAMEBUFFER, fbo);
            gl.glBindRenderbuffer(RENDERBUFFER, rb[0]);
            gl.glRenderbufferStorageMultisample(RENDERBUFFER, self.samples, RGBA8, w, h);
            gl.glFramebufferRenderbuffer(FRAMEBUFFER, COLOR_ATTACHMENT0, RENDERBUFFER, rb[0]);
            gl.glBindRenderbuffer(RENDERBUFFER, rb[1]);
            gl.glRenderbufferStorageMultisample(
                RENDERBUFFER,
                self.samples,
                DEPTH_COMPONENT24,
                w,
                h,
            );
            gl.glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, rb[1]);
            let status = gl.glCheckFramebufferStatus(FRAMEBUFFER);
            if status != FRAMEBUFFER_COMPLETE {
                return Err(format!("Zeichenpuffer unvollständig (Status {status:#x})."));
            }
            self.target = Some(Target {
                fbo,
                color: rb[0],
                depth: rb[1],
                width: w,
                height: h,
            });
        }
        Ok(())
    }

    /// Zeichnet ein Bild. Die 3D-Ansicht füllt das Fenster unterhalb von `top` Pixeln.
    pub fn draw(&mut self, win_w: u32, win_h: u32, top: u32, view: &View) -> Result<(), String> {
        let (w, h) = (win_w as i32, win_h as i32 - top as i32);
        if w <= 0 || h <= 0 {
            return Ok(());
        }
        self.ensure_target(w, h)?;
        let target_fbo = self.target.as_ref().map_or(0, |t| t.fbo);
        let gl = &self.gl;
        let st = &self.style;
        unsafe {
            gl.glBindFramebuffer(FRAMEBUFFER, target_fbo);
            gl.glViewport(0, 0, w, h);
            gl.glDisable(SCISSOR_TEST);
            gl.glDisable(BLEND);
            gl.glDisable(CULL_FACE);
            gl.glDisable(FRAMEBUFFER_SRGB);
            gl.glEnable(MULTISAMPLE);
            gl.glEnable(DEPTH_TEST);
            gl.glDepthMask(TRUE);
            gl.glClearDepth(1.0);
            gl.glClear(DEPTH_BUFFER_BIT);

            // Himmel und Boden, in Zeichnungen einfarbiges Papier
            if let Some(c) = view.paper {
                gl.glClearColor(c[0], c[1], c[2], 1.0);
                gl.glClear(COLOR_BUFFER_BIT);
            }
            gl.glDepthFunc(ALWAYS);
            let p = self.sky.id;
            gl.glUseProgram(p);
            mat(gl, p, c"u_inv_vp", &view.inv_view_proj);
            mat(gl, p, c"u_vp", &view.view_proj);
            gl.glUniform1f(loc(gl, p, c"u_eye_z"), view.eye_z);
            gl.glUniform1f(loc(gl, p, c"u_horizon_px"), view.horizon_px);
            gl.glUniform1f(loc(gl, p, c"u_height"), h as f32);
            gl.glUniform1f(loc(gl, p, c"u_softness"), st.horizon_softness);
            vec3(gl, p, c"u_ground", st.ground);
            let n = st.sky.len().min(SKY_MAX);
            let pos: Vec<f32> = st.sky[..n].iter().map(|s| s.0).collect();
            let col: Vec<f32> = st.sky[..n].iter().flat_map(|s| s.1).collect();
            gl.glUniform1i(loc(gl, p, c"u_sky_n"), n as i32);
            gl.glUniform1fv(loc(gl, p, c"u_sky_pos"), n as i32, pos.as_ptr());
            gl.glUniform3fv(loc(gl, p, c"u_sky_col"), n as i32, col.as_ptr());
            gl.glUniform1i(loc(gl, p, c"u_overlay"), 0);
            gl.glUniform1f(loc(gl, p, c"u_opacity"), st.ground_opacity.clamp(0.0, 1.0));
            gl.glBindVertexArray(self.empty_vao);
            if view.paper.is_none() {
                gl.glDrawArrays(TRIANGLES, 0, 3);
            }

            // Flächen, leicht nach hinten versetzt, damit die Kanten sauber obenauf liegen
            gl.glDepthFunc(LESS);
            gl.glEnable(POLYGON_OFFSET_FILL);
            gl.glPolygonOffset(1.0, 1.0);
            let p = self.faces.id;
            gl.glUseProgram(p);
            mat(gl, p, c"u_vp", &view.view_proj);
            vec3(gl, p, c"u_origin", view.origin_rel);
            vec3(gl, p, c"u_light", st.light);
            gl.glUniform1f(loc(gl, p, c"u_ambient"), st.ambient);
            let drawing = view.paper.is_some();
            gl.glUniform1i(loc(gl, p, c"u_drawing"), drawing as GLint);
            gl.glActiveTexture(TEXTURE0 + 1);
            gl.glBindTexture(TEXTURE_2D, self.looks_tex);
            gl.glUniform1i(loc(gl, p, c"u_looks"), 1);
            gl.glActiveTexture(TEXTURE0);
            for m in &self.meshes {
                gl.glBindVertexArray(m.faces.vao);
                gl.glDrawArrays(TRIANGLES, 0, m.faces.count);
            }
            gl.glDisable(POLYGON_OFFSET_FILL);

            // Kanten
            gl.glDepthFunc(LEQUAL);
            let p = self.edges.id;
            gl.glUseProgram(p);
            mat(gl, p, c"u_vp", &view.view_proj);
            vec3(gl, p, c"u_origin", view.origin_rel);
            let edges = if drawing {
                &self.looks.drawing
            } else {
                &self.looks.model
            };
            gl.glUniform2f(loc(gl, p, c"u_viewport"), w as f32, h as f32);
            let n = EDGE_KINDS as i32;
            gl.glUniform1fv(loc(gl, p, c"u_edge_width"), n, edges.width.as_ptr());
            let colors = edges.color.as_flattened();
            gl.glUniform3fv(loc(gl, p, c"u_edge_color"), n, colors.as_ptr());
            let dash = edges.dash.as_flattened();
            gl.glUniform4fv(loc(gl, p, c"u_edge_dash"), 2 * n, dash.as_ptr());
            gl.glUniform1f(loc(gl, p, c"u_near"), view.near);
            for m in self.meshes.iter().filter(|m| m.edges.count > 0) {
                gl.glBindVertexArray(m.edges.vao);
                gl.glDrawArraysInstanced(TRIANGLES, 0, 6, m.edges.count);
            }

            // Boden durchscheinend über allem, was unter z = 0 liegt; über dem
            // Boden scheitert der Tiefentest, über leerem Boden bleibt das Bild
            if view.paper.is_none() && st.ground_opacity > 0.0 {
                gl.glDepthFunc(LESS);
                gl.glDepthMask(FALSE);
                gl.glEnable(BLEND);
                gl.glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
                gl.glUseProgram(self.sky.id);
                gl.glUniform1i(loc(gl, self.sky.id, c"u_overlay"), 1);
                gl.glBindVertexArray(self.empty_vao);
                gl.glDrawArrays(TRIANGLES, 0, 3);
                gl.glDisable(BLEND);
                gl.glDepthMask(TRUE);
            }

            // Hilfslinien und Markierungen: erst die sichtbaren Teile, dann die
            // verdeckten Teile verdeckbarer Linien blass
            if self.helper_mesh.count > 0 {
                gl.glDepthMask(FALSE);
                gl.glEnable(BLEND);
                gl.glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
                let p = self.helpers.id;
                gl.glUseProgram(p);
                mat(gl, p, c"u_vp", &view.view_proj);
                vec3(gl, p, c"u_origin", view.origin_rel);
                gl.glUniform2f(loc(gl, p, c"u_viewport"), w as f32, h as f32);
                gl.glUniform1f(loc(gl, p, c"u_near"), view.near);
                let q = view.pull;
                gl.glUniform4f(loc(gl, p, c"u_pull"), q[0], q[1], q[2], q[3]);
                gl.glBindVertexArray(self.helper_mesh.vao);
                for (func, hidden) in [(LEQUAL, 0.0), (GREATER, 1.0)] {
                    gl.glDepthFunc(func);
                    gl.glUniform1f(loc(gl, p, c"u_hidden"), hidden);
                    gl.glDrawArrays(TRIANGLES, 0, self.helper_mesh.count);
                }
                gl.glDisable(BLEND);
                gl.glDepthMask(TRUE);
            }

            // Mehrfachabtastung auflösen und ins Fenster kopieren
            gl.glBindFramebuffer(READ_FRAMEBUFFER, target_fbo);
            gl.glBindFramebuffer(DRAW_FRAMEBUFFER, 0);
            gl.glBlitFramebuffer(0, 0, w, h, 0, 0, w, h, COLOR_BUFFER_BIT, NEAREST as GLenum);

            gl.glBindFramebuffer(FRAMEBUFFER, 0);
            gl.glDisable(DEPTH_TEST);
            // Oberfläche (Titelleiste, Paneele) obenauf
            gl.glEnable(BLEND);
            gl.glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
            let p = self.overlay.id;
            gl.glUseProgram(p);
            gl.glActiveTexture(TEXTURE0);
            gl.glUniform1i(loc(gl, p, c"u_tex"), 0);
            gl.glBindVertexArray(self.empty_vao);
            for o in self.overlays.iter().filter(|o| o.w > 0 && o.h > 0) {
                gl.glViewport(o.x, win_h as i32 - o.y - o.h, o.w, o.h);
                gl.glBindTexture(TEXTURE_2D, o.tex);
                gl.glDrawArrays(TRIANGLES, 0, 3);
            }
            gl.glDisable(BLEND);
            gl.glBindVertexArray(0);
        }
        Ok(())
    }

    /// Liest das fertige Bild (vor dem Tauschen der Puffer) als RGBA8, Zeilen von oben.
    pub fn read_pixels(&self, w: u32, h: u32) -> Vec<u8> {
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            self.gl.glBindFramebuffer(READ_FRAMEBUFFER, 0);
            self.gl.glReadBuffer(BACK);
            self.gl.glPixelStorei(PACK_ALIGNMENT, 1);
            self.gl.glReadPixels(
                0,
                0,
                w as i32,
                h as i32,
                RGBA,
                UNSIGNED_BYTE,
                px.as_mut_ptr() as *mut c_void,
            );
        }
        let row = (w * 4) as usize;
        let mut flipped = Vec::with_capacity(px.len());
        for r in px.chunks(row).rev() {
            flipped.extend_from_slice(r);
        }
        flipped
    }
}

const CORNERS: [[f32; 2]; 6] = [
    [0.0, -1.0],
    [1.0, -1.0],
    [1.0, 1.0],
    [0.0, -1.0],
    [1.0, 1.0],
    [0.0, 1.0],
];

/// Lädt Vertexdaten in `b` und legt die Attribute (Anzahl, Byte-Versatz) fest.
unsafe fn fill<T>(gl: &Gl, b: &mut GpuBuffer, data: &[T], attrs: &[(i32, usize)]) {
    fill_with(gl, b, data, attrs, 0);
}

/// Wie [`fill`]; `divisor` 1 macht jeden Eintrag zu einer Instanz.
unsafe fn fill_with<T>(
    gl: &Gl,
    b: &mut GpuBuffer,
    data: &[T],
    attrs: &[(i32, usize)],
    divisor: u32,
) {
    if b.vao == 0 {
        gl.glGenVertexArrays(1, &mut b.vao);
        gl.glGenBuffers(1, &mut b.buf);
        gl.glBindVertexArray(b.vao);
        gl.glBindBuffer(ARRAY_BUFFER, b.buf);
        let stride = std::mem::size_of::<T>() as i32;
        for (i, &(n, off)) in attrs.iter().enumerate() {
            gl.glVertexAttribPointer(i as u32, n, FLOAT, FALSE, stride, off as *const c_void);
            gl.glEnableVertexAttribArray(i as u32);
            if divisor != 0 {
                gl.glVertexAttribDivisor(i as u32, divisor);
            }
        }
    }
    gl.glBindVertexArray(b.vao);
    gl.glBindBuffer(ARRAY_BUFFER, b.buf);
    upload(gl, data);
    gl.glBindVertexArray(0);
    b.count = data.len() as i32;
}

unsafe fn upload<T>(gl: &Gl, data: &[T]) {
    gl.glBufferData(
        ARRAY_BUFFER,
        std::mem::size_of_val(data) as isize,
        data.as_ptr() as *const c_void,
        DYNAMIC_DRAW,
    );
}

unsafe fn loc(gl: &Gl, p: GLuint, name: &std::ffi::CStr) -> GLint {
    gl.glGetUniformLocation(p, name.as_ptr() as *const GLchar)
}

unsafe fn mat(gl: &Gl, p: GLuint, name: &std::ffi::CStr, m: &[f32; 16]) {
    gl.glUniformMatrix4fv(loc(gl, p, name), 1, FALSE, m.as_ptr());
}

unsafe fn vec3(gl: &Gl, p: GLuint, name: &std::ffi::CStr, v: [f32; 3]) {
    gl.glUniform3f(loc(gl, p, name), v[0], v[1], v[2]);
}

unsafe fn shader(gl: &Gl, kind: GLenum, src: &str) -> Result<GLuint, String> {
    let s = gl.glCreateShader(kind);
    let ptr = src.as_ptr() as *const GLchar;
    let len = src.len() as GLint;
    gl.glShaderSource(s, 1, &ptr, &len);
    gl.glCompileShader(s);
    let mut ok = 0;
    gl.glGetShaderiv(s, COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let mut n = 0;
        gl.glGetShaderiv(s, INFO_LOG_LENGTH, &mut n);
        let mut log = vec![0u8; n.max(1) as usize];
        gl.glGetShaderInfoLog(s, n, &mut n, log.as_mut_ptr() as *mut GLchar);
        return Err(format!(
            "Shader-Fehler: {}",
            String::from_utf8_lossy(&log[..n.max(0) as usize])
        ));
    }
    Ok(s)
}

/// Fragment-Shader mit der gemeinsamen Strichmuster-Funktion davor.
fn with_dash(fs: &str) -> String {
    format!("#version 330 core\n{DASH_GLSL}{fs}")
}

unsafe fn program(gl: &Gl, vs: &str, fs: &str) -> Result<Program, String> {
    let v = shader(gl, VERTEX_SHADER, vs)?;
    let f = shader(gl, FRAGMENT_SHADER, fs)?;
    let p = gl.glCreateProgram();
    gl.glAttachShader(p, v);
    gl.glAttachShader(p, f);
    gl.glLinkProgram(p);
    gl.glDeleteShader(v);
    gl.glDeleteShader(f);
    let mut ok = 0;
    gl.glGetProgramiv(p, LINK_STATUS, &mut ok);
    if ok == 0 {
        let mut n = 0;
        gl.glGetProgramiv(p, INFO_LOG_LENGTH, &mut n);
        let mut log = vec![0u8; n.max(1) as usize];
        gl.glGetProgramInfoLog(p, n, &mut n, log.as_mut_ptr() as *mut GLchar);
        return Err(format!(
            "Shader-Verknüpfung fehlgeschlagen: {}",
            String::from_utf8_lossy(&log[..n.max(0) as usize])
        ));
    }
    Ok(Program { id: p })
}

impl Renderer {
    pub fn set_style(&mut self, style: Style) {
        self.style = style;
    }
}

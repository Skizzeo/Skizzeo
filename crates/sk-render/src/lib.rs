//! GPU-Darstellung der 3D-Ansicht über OpenGL 3.3.
//!
//! Ablauf je Bild: Himmel und Boden als Vollbild-Pass (ohne Tiefe), dann
//! Flächen, dann Kanten als bildschirmbreite Bänder, dann der Boden noch einmal
//! durchscheinend über allem, was unter ihm liegt (E11). Alles in
//! einen Mehrfachabtast-Puffer (MSAA), der anschließend ins Fenster kopiert wird.
//! Darüber kommt die selbst gezeichnete Oberfläche (Titelleiste) als Textur.

pub mod gl;
#[cfg(target_os = "linux")]
pub mod glx;
pub mod schatten;

use gl::*;
use schatten::{SCHATTEN_AUS_GLSL, SCHATTEN_GLSL};
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
/// Kantenart der feinen Linie; muss `sk_model::edge_kind::FINE` und der
/// Konstante `FINE` in `EDGE_VS` gleichen (Strich unter dem Gelände, S11).
pub const EDGE_FINE: u8 = 2;

/// Zeilen der Aussehens-Tabelle je Darstellungsschlüssel.
pub const LOOK_ROWS: usize = 15;

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
/// | 8 | Muster: Art 0 ohne, 1 Mauerwerk, 2 Putz, 3 Sichtbeton, 4 Holz, 5 Platten, 6 Naturstein; dann je Art (unten) | |
/// | 9 | je Art; Feld b immer der Startwert | |
/// | 10 | Farbe 1 (r·65536 + g·256 + b), Anteil 1 %, Farbe 2, Anteil 2 % | |
/// | 11 | Farbe 3, Fugenfarbe, Anteil 3 % | Mischfarbe (Ferne, 3D ohne Muster) |
/// | 12 | Flammung %, Enden braun %, Relief % bzw. Poren % | Deckkraft des Musters 0..1 (wilder Verband: 0, bis seine Tabelle vorliegt, dann eingeblendet) |
/// | 13 | Kopffarbe 1, Anteil 1, Kopffarbe 2, Anteil 2 (ohne `hpal` wie Zeile 10) | |
/// | 14 | Kopffarbe 3, –, Anteil 3 | |
///
/// Zeilen 8 und 9 je Art (r g b a):
///
/// | Art | Zeile 8 | Zeile 9 |
/// |---|---|---|
/// | 1 Mauerwerk | Länge, Höhe, Fuge | Verband (0,5 / ⅓ / −1 wild / 2 Block / 3 Kreuz), Streuung %, Startwert, Nummer der Verbandstabelle |
/// | 2 Putz | – | –, Streuung %, Startwert, Körnung mm |
/// | 3 Sichtbeton | Tafel Breite, Höhe, Stoßbreite | Anker 0/1, Wolkigkeit %, Startwert |
/// | 4 Holz | Brettbreite, Fuge, senkrecht 0/1 | Maserung %, –, Startwert; Zeile 10: Holzfarbe 1, –, Holzfarbe 2 |
/// | 5 Platten | Länge, Breite, Fuge | Halbversatz 0/1, Streuung %, Startwert |
/// | 6 Naturstein | Steingröße, Fuge, Unregelmäßigkeit % | –, –, Startwert |
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
    /// Fugen in Ansichten (Stift „Ansichtsmuster“): Farbe 0..1, Breite px.
    pub pattern_ink: [f32; 4],
    /// Verbandstabellen des wilden Verbands untereinander, je
    /// [`BOND_TABLE_BYTES`] (Nummer in Looks-Zeile 9, Feld w).
    pub bond: Vec<u8>,
}

/// Größe einer Verbandstabelle: 128 Schichten × 128 Viertel.
pub const BOND_TABLE_BYTES: usize = 128 * 128;

/// Musterdarstellung einer Ansicht (Uniform `u_patterns`).
pub mod pattern_mode {
    /// Kein Muster.
    pub const NONE: i32 = 0;
    /// Ansichten: nur Fugenlinien in Tinte.
    pub const LINES: i32 = 1;
    /// 3D: Steinfarben und Fugen.
    pub const COLORS: i32 = 2;
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

impl View {
    /// Dieselbe Ansicht um `dy` Pixel nach unten verschoben (Ansichtshöhe
    /// `h`): eine Verschiebung in der Matrix, kein neues Netz.
    pub fn shifted(mut self, dy: f32, h: f32) -> View {
        let d = -2.0 * dy / h.max(1.0);
        for c in 0..4 {
            self.view_proj[c * 4 + 1] += d * self.view_proj[c * 4 + 3];
        }
        self
    }

    /// Dieselbe Ansicht um `dx` Pixel nach rechts verschoben und waagerecht
    /// um `k` gestaucht (Achse bei `axis` Pixel von links, Ansichtsbreite `w`).
    pub fn squeezed(mut self, dx: f32, k: f32, axis: f32, w: f32) -> View {
        let w = w.max(1.0);
        let a = 2.0 * axis / w - 1.0;
        let d = 2.0 * dx / w + (1.0 - k) * a;
        for c in 0..4 {
            self.view_proj[c * 4] = k * self.view_proj[c * 4] + d * self.view_proj[c * 4 + 3];
        }
        self
    }
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
    /// Musterdarstellung ([`pattern_mode`]); geschnittene und blasse
    /// Flächen zeigen nie ein Muster.
    pub patterns: i32,
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
    /// Größe des Bildes (gezeichnet wird es `w` × `h` groß) und Deckkraft.
    tw: i32,
    th: i32,
    alpha: f32,
}

/// Festgehaltenes Bild der Modellansicht (Überblendung beim Geschosswechsel).
struct Snapshot {
    fbo: GLuint,
    tex: GLuint,
    w: i32,
    h: i32,
    /// Deckkraft und Versatz nach unten (Pixel) beim nächsten Bild.
    alpha: f32,
    offset: f32,
    /// Versatz nach rechts (Pixel).
    offset_x: f32,
    /// Blatt wenden: (Stauchung 0…1, Achse in Pixeln von links, Helligkeit).
    turn: Option<(f32, f32, f32)>,
}

struct Target {
    fbo: GLuint,
    color: GLuint,
    depth: GLuint,
    width: i32,
    height: i32,
}

/// Teilbild der Vorschau im Fenster „Muster“ (Paket 7b): eine kleine
/// Szene mit eigener Kamera in einem Bereich des Vorschaubilds.
#[derive(Clone, Copy, Debug)]
pub struct PreviewItem {
    /// Bereich im Vorschaubild: links, oben, Breite, Höhe (Pixel).
    pub rect: [i32; 4],
    /// Nur dieser Ausschnitt wird gezeichnet (Vorher/Nachher-Teiler).
    pub clip: Option<[i32; 4]>,
    pub view: View,
    /// Netz aus [`Renderer::set_preview_mesh`].
    pub mesh: usize,
    /// Licht wie im Modell; sonst flach (Varianten von vorne).
    pub lit: bool,
    /// Himmel und Boden dahinter.
    pub sky: bool,
}

/// Vorschau mit dem Flächen-Shader in einem eigenen Bild (Review 3d): nur
/// bei Änderung neu gezeichnet, dann als Bild unter das Oberflächenbild
/// `before` gelegt, das an diesen Stellen durchsichtig ist.
#[derive(Clone, Debug)]
pub struct Preview {
    /// Lage im Fenster (links, oben) und Größe des Vorschaubilds (Pixel).
    pub at: [i32; 4],
    pub before: usize,
    pub items: Vec<PreviewItem>,
}

/// Bild der Vorschau: mehrfach abgetastet gezeichnet, aufgelöst in eine
/// Textur; neu angelegt nur bei anderer Größe.
struct PreviewTarget {
    ms_fbo: GLuint,
    ms: [GLuint; 2],
    fbo: GLuint,
    tex: GLuint,
    /// Festgehaltenes voriges Bild zum Überblenden (Vorlagenwechsel).
    prev_fbo: GLuint,
    prev_tex: GLuint,
    w: i32,
    h: i32,
}

/// Sonne in der 3D-Ansicht (Sonnenstand S5): Richtung zur Sonne
/// (Modell) und Umgebungsanteil der Flächen, solange sie scheint. Ab
/// [`schatten::MIN_HOEHE`] werfen die Netze Schatten auf sich und den Boden.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunLight {
    pub zur_sonne: [f64; 3],
    pub ambient: f32,
}

/// Schatten in den Ansichten (Sonnenstand S7), im Papiermodus: Richtung
/// zum Licht (Modell) und Darstellung. Abgewandte Flächen und Flächen im
/// Schlagschatten werden grau getönt oder schraffiert.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperShade {
    pub zum_licht: [f64; 3],
    /// Schraffur statt grauer Fläche.
    pub hatch: bool,
    /// Graue Fläche: Tinte (RGB) und ihr Anteil.
    pub tone: [f32; 4],
    /// Schraffur unter 45° steigend: Abstand und Strichbreite (px), Tinte.
    pub hatch_px: [f32; 2],
    pub hatch_ink: [f32; 3],
}

/// Schattenkarte auf der GPU: Tiefentextur mit Vergleich in eigenem
/// Zeichenpuffer.
struct ShadowMap {
    fbo: GLuint,
    tex: GLuint,
    size: i32,
}

pub struct Renderer {
    gl: Gl,
    sky: Program,
    faces: Program,
    /// Meldung des Treibers, wenn der Flächen-Shader mit Mustern nicht
    /// übersetzt werden konnte; dann zeichnet er ohne Muster.
    pattern_error: Option<String>,
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
    bond_tex: GLuint,
    looks: Looks,
    snap_prog: Program,
    snapshot: Option<Snapshot>,
    /// Platz des blassen Netzes (Isolieren, Paket 3) und seine Deckkraft.
    ghost: Option<(usize, f32)>,
    /// Vorschau im Fenster „Muster“ (Paket 7b) mit eigenen Netzen und
    /// eigener Aussehens-Tabelle.
    preview: Option<Preview>,
    preview_dirty: bool,
    preview_meshes: Vec<GpuMesh>,
    preview_looks: Looks,
    preview_looks_tex: GLuint,
    preview_bond_tex: GLuint,
    preview_target: Option<PreviewTarget>,
    /// Deckkraft des festgehaltenen vorigen Vorschaubilds.
    preview_fade: f32,
    /// Probe `--musterprobe`: Muster in voller Nähe, ohne Kantenglättung.
    preview_exact: bool,
    /// Schatten der Sonne (S5): Tiefen-Programm, Karte (bei Bedarf
    /// angelegt), leere Tiefentextur für die Zeit ohne Karte, größte
    /// Texturgröße des Treibers.
    shadow_prog: Program,
    shadow_map: Option<ShadowMap>,
    shadow_dummy: GLuint,
    shadow_max: i32,
    sun: Option<SunLight>,
    /// Karte des letzten Bildes; neu gezeichnet, wenn sie sich ändert oder
    /// ein Netz neu kommt.
    shadow_karte: Option<schatten::Karte>,
    shadow_dirty: bool,
    /// Schatten der Ansichten (S7) und ob das Bild auf Papier entsteht
    /// (dann gilt deren Licht statt der Sonne).
    paper_shade: Option<PaperShade>,
    shadow_paper: bool,
    /// Ansichten (S11): unter dem Gelände 0 wie alles, 1 ausgeblendet, 2
    /// gestrichelt mit Strich und Lücke (px); nur auf Papier.
    below: (i32, [f32; 2]),
    /// Höhe von OK Gelände (mm): Boden in 3D, „unter dem Gelände“ in den
    /// Ansichten (Gelände Thema 1).
    terrain: f32,
    /// Konnte der Treiber die Karte nicht anlegen: Meldung (einmal
    /// abzuholen), danach ohne Schatten.
    shadow_failed: bool,
    shadow_error: Option<String>,
    /// Zeitabfrage des Tiefen-Durchgangs (S6): die Abfrage, die
    /// Kartengröße, solange ihr Ergebnis aussteht, die neue Messung (Größe,
    /// ms; einmal abzuholen) und die letzte in voller Größe.
    shadow_query: GLuint,
    shadow_query_open: Option<u32>,
    shadow_ms: Option<(u32, f64)>,
    shadow_ms_voll: Option<f64>,
    /// Beim Ziehen an Sonne oder Schatten: die beim Greifen gewählte
    /// Kartengröße, bis zum Loslassen.
    shadow_entwurf: Option<u32>,
    /// Hüllquader je Netz (Modell, mm).
    mesh_bounds: Vec<Option<(sk_math::Vec3, sk_math::Vec3)>>,
}

/// Einheit der Schattenkarte im Flächen- und Himmels-Shader.
const SHADOW_UNIT: GLenum = 3;

const SHADOW_VS: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
uniform mat4 u_m;
uniform vec3 u_c;
void main() {
    gl_Position = u_m * vec4(a_pos - u_c, 1.0);
}
"#;

const SHADOW_FS: &str = r#"#version 330 core
void main() {}
"#;

const SKY_MAX: usize = 16;

const FULLSCREEN_VS: &str = r#"#version 330 core
out vec2 v_ndc;
void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    v_ndc = p * 2.0 - 1.0;
    gl_Position = vec4(v_ndc, 0.0, 1.0);
}
"#;

/// Himmel und Boden; davor gehören `#version` und [`schatten::SCHATTEN_GLSL`].
const SKY_FS: &str = r#"
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
// Mitte der Schattenkarte relativ zum Auge; Boden im Schatten mal u_shade
uniform vec3 u_shadow_c;
uniform float u_shade;

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
        vec3 hit = o + dir * t;
        vec4 c = u_vp * vec4(hit, 1.0);
        depth = clamp(c.z / c.w * 0.5 + 0.5, 0.0, 1.0);
        // Schatten der Sonne auf dem Boden (S5)
        float share = sun_share(hit - u_shadow_c, vec3(0.0, 0.0, 1.0));
        col = mix(col, u_ground * mix(u_shade, 1.0, share), g);
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
// Modelllage in mm (Musterkoordinaten, Paket 6)
out vec3 v_model;
void main() {
    v_normal = a_normal;
    v_key = int(a_key + 0.5);
    v_uv = a_uv;
    v_model = a_pos;
    gl_Position = u_vp * vec4(a_pos + u_origin, 1.0);
}
"#;

const FACE_FS: &str = r#"
in vec3 v_normal;
flat in int v_key;
in vec2 v_uv;
in vec3 v_model;
out vec4 o_color;
uniform sampler2D u_looks;
uniform int u_drawing;
// Musterdarstellung: 0 aus, 1 Fugenlinien (Ansichten), 2 Farben (3D)
uniform int u_patterns;
// Stift „Ansichtsmuster“: Farbe, Breite px
uniform vec4 u_pattern_ink;
uniform vec3 u_light;
uniform float u_ambient;
// Mitte der Schattenkarte (Modell, mm)
uniform vec3 u_shadow_c;
// Deckkraft: 1 deckend, darunter blass (Isolieren) und ohne Schraffur
uniform float u_alpha;
// Schatten in den Ansichten (S7): 0 aus, 1 graue Fläche, 2 Schraffur;
// Richtung zum Licht, Ton (Tinte, Anteil), Schraffur (Abstand, Breite px)
// und ihre Tinte
uniform int u_paper_shade;
uniform vec3 u_paper_light;
uniform vec4 u_shade_tone;
uniform vec2 u_hatch;
uniform vec3 u_hatch_ink;
// Ansichten (S11): unter dem Gelände (z < 0) 0 wie alles, 1 ausgeblendet,
// 2 gestrichelt; Flächen dort in Papierfarbe, sie verdecken weiter
uniform int u_below;
uniform vec3 u_paper_rgb;
// Höhe von OK Gelände im Modell (mm, Gelände Thema 1)
uniform float u_terrain;
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
        // Muster ohne Darstellung in 3D: Mischfarbe statt der Ansichtsfläche
        // (Zeile 11, Feld w); Putz bleibt in seiner Farbe (Paket 6)
        int kind = int(look(8).x + 0.5);
        vec3 surf = c;
        if (!cut && kind != 0 && kind != 2) c = unpack_rgb(look(11).w);
        // Paket 6b/7a: Muster, weich zur Mischfarbe in der Ferne
        if (!cut && u_alpha >= 1.0 && u_patterns == 2 && kind != 0) {
            vec3 far = kind == 2 ? unpack_rgb(look(11).w) : c;
            c = mix(c, pattern_rgb(v_model, v_normal, far, surf, look(8), look(9), look(10), look(11), look(12), look(13), look(14)), look(12).w);
        }
        vec3 n = normalize(v_normal);
        float d = max(dot(n, u_light), 0.0);
        // Schatten der Sonne (S5): nur besonnte Seiten lesen die Karte
        if (d > 0.0) d *= sun_share(v_model - u_shadow_c, n);
        o_color = vec4(c * (u_ambient + (1.0 - u_ambient) * d), u_alpha);
        return;
    }
    vec4 bg = look(2);
    vec3 c = bg.rgb;
    if (u_below != 0 && !cut && v_model.z < u_terrain - 0.5) {
        o_color = vec4(u_paper_rgb, u_alpha);
        return;
    }
    if (!cut && u_alpha >= 1.0 && u_paper_shade != 0) {
        // Eigenschatten (abgewandt) und Schlagschatten aus der Karte
        vec3 n = normalize(v_normal);
        float lit = dot(n, u_paper_light) > 0.0 ? sun_share(v_model - u_shadow_c, n) : 0.0;
        float s = 1.0 - lit;
        if (u_paper_shade == 1) {
            c = mix(c, u_shade_tone.rgb, u_shade_tone.a * s);
        } else {
            // 45° steigend: Abstand senkrecht zu den Linien u_hatch.x. Wie
            // die Schnittschraffur am Fenster verankert und mit voller Tinte
            // ab halbem Schatten (Jörn 09.10. 14:10, S13)
            float p = u_hatch.x * 1.41421356;
            vec2 q = gl_FragCoord.xy;
            float m = mod(q.x - q.y, p);
            float dist = min(m, p - m) * 0.70710678;
            float ink = clamp(u_hatch.y * 0.5 + 0.5 - dist, 0.0, 1.0);
            c = mix(c, u_hatch_ink, ink * step(0.5, s));
        }
    }
    if (!cut && u_alpha >= 1.0 && u_patterns == 1 && int(look(8).x + 0.5) != 0) {
        // Ansicht: Fugen als Mittellinien in Tinte, keine Steinfarben
        float ink = pattern_lines(v_model, v_normal, look(8), look(9), u_pattern_ink.a) * look(12).w;
        c = mix(c, u_pattern_ink.rgb, ink);
    }
    if (cut && u_alpha < 1.0) {
        // Blasse Schnittfläche: Füllung ohne Schraffur
        if (int(bg.a + 0.5) == 1) c = look(3).rgb;
    } else if (cut) {
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
    o_color = vec4(c, u_alpha);
}
"#;

/// Muster (Paket 6, 7) im Fragment-Shader: Hash, Verbände, Fugen und die
/// Arten aus Paket 7. Zeile für Zeile dieselbe Rechnung wie
/// `sk_model::proctex` und `texgen` (lowbias32, Regel 61); nur Ganzzahlen
/// und `floor`, kein Gleitkomma-Hash (Review 3c P6). Bekommt die
/// Looks-Zeilen 8–14 als Argumente.
pub const PATTERN_GLSL: &str = r#"
// Farbe aus r·65536 + g·256 + b (Looks-Zeilen 10–14)
vec3 unpack_rgb(float f) {
    int n = int(f + 0.5);
    return vec3(float((n >> 16) & 255), float((n >> 8) & 255), float(n & 255)) / 255.0;
}
uint lowbias32(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352du;
    x ^= x >> 15;
    x *= 0x846ca68bu;
    x ^= x >> 16;
    return x;
}
uint pat_hash(int row, int col, uint seed) {
    uint a = lowbias32(uint(row) ^ (seed * 0x9e3779b9u));
    return lowbias32(a + uint(col) * 0x85ebca6bu);
}
// Hash als Anteil 0..1 (24 Bit), wie `unit` in Rust
float pat_unit(uint h) {
    return float(h >> 8) / 16777216.0;
}
// 16 Bit ab `shift` als 0..1, wie `texgen::h01`
float h01(uint h, int shift) {
    return float((h >> uint(shift)) & 0xffffu) / 65535.0;
}
// Näherungsweise normalverteilt (σ 1), wie `texgen::gauss3`
float gauss3(uint h) {
    float a = float((h & 1023u) + ((h >> 10) & 1023u) + ((h >> 20) & 1023u)) / 1023.0;
    return (a - 1.5) * 2.0;
}
// Wertrauschen −1..1 auf einem Gitter cu × cv mm, wie `texgen::vnoise`
float vnoise(float u, float v, float cu, float cv, uint seed) {
    float x = u / cu;
    float y = v / cv;
    float gx = floor(x);
    float gy = floor(y);
    float fx = x - gx;
    float fy = y - gy;
    int ix = int(gx);
    int iy = int(gy);
    float a = h01(pat_hash(ix, iy, seed), 0) * 2.0 - 1.0;
    float b = h01(pat_hash(ix + 1, iy, seed), 0) * 2.0 - 1.0;
    float c = h01(pat_hash(ix, iy + 1, seed), 0) * 2.0 - 1.0;
    float d = h01(pat_hash(ix + 1, iy + 1, seed), 0) * 2.0 - 1.0;
    float sx = fx * fx * (3.0 - 2.0 * fx);
    float sy = fy * fy * (3.0 - 2.0 * fy);
    return (a * (1.0 - sx) + b * sx) * (1.0 - sy) + (c * (1.0 - sx) + d * sx) * sy;
}
// Je Zelle ein Wert −1..1, wie `texgen::cellnoise`
float cellnoise(float u, float v, float cell, uint seed) {
    return h01(pat_hash(int(floor(u / cell)), int(floor(v / cell)), seed), 0) * 2.0 - 1.0;
}
// Helligkeit je Stein bzw. Platte: ± Streuung % (Paket 6)
float spread_factor(uint h, float spread) {
    if (spread <= 0.0) return 1.0;
    return 1.0 + spread / 100.0 * (2.0 * pat_unit(lowbias32(h ^ 0x68e31da4u)) - 1.0);
}
// Farbe (0–255) nach den Anteilen einer Palette (zwei Zeilen wie 10/11),
// ganzzahlig wie `texgen::pick`; `fam` = Nummer der Farbe
vec3 pat_pick(vec4 a, vec4 b, uint h, out int fam) {
    uint x = (h >> 8) * 100u;
    float cols[3] = float[3](a.x, a.z, b.x);
    float shares[3] = float[3](a.y, a.w, b.z);
    uint cum = 0u;
    float c = cols[0];
    fam = 0;
    for (int i = 0; i < 3; i++) {
        if (shares[i] <= 0.0) continue;
        cum += uint(shares[i]);
        c = cols[i];
        fam = i;
        if (x < (cum << 24)) break;
    }
    return unpack_rgb(c) * 255.0;
}
// Wilder Verband: Verbandstabellen untereinander (je 128 Schichten × 128
// Viertel, R8UI), Byte = Abstand zum Steinanfang (Bits 0–1) und Kopf (Bit 2),
// wie `sk_model::proctex::BondTable`
uniform usampler2D u_bond;
// Musterkoordinaten (mm): senkrechte Fläche u längs, v = Höhe über ±0,00;
// waagerechte Fläche: x, y. z: 1 senkrecht, 0 waagerecht
vec3 pattern_uv(vec3 p, vec3 n) {
    n = normalize(n);
    if (abs(n.z) < 0.5) {
        vec2 t = normalize(vec2(-n.y, n.x));
        return vec3(dot(p.xy, t), p.z, 1.0);
    }
    return vec3(p.x, p.y, 0.0);
}
// Stein an u in Reihe `row`, wie `locate` in Rust: Nummer, Lage ab der
// Stoßfugenmitte am Steinanfang (mm), Achsmaß (mm), Kopf 0/1.
// Verband p9.x: 0,5 halb, ⅓ Drittel, −1 wild, 2 Block, 3 Kreuz
vec4 masonry_stone(float u, int row, vec4 p8, vec4 p9) {
    float a = p8.y + p8.w;
    if (p9.x < 0.0) {
        float x = 4.0 * u / a;
        int q = int(floor(x));
        uint c = texelFetch(u_bond, ivec2(q & 127, (row & 127) + 128 * int(p9.w + 0.5)), 0).r;
        int start = q - int(c & 3u);
        bool head = (c & 4u) != 0u;
        float n = head ? 2.0 : 4.0;
        return vec4(float(start), (x - float(start)) * a * 0.25, n * a * 0.25, head ? 1.0 : 0.0);
    }
    float off = 0.0;
    float pitch = a;
    float head = 0.0;
    if (p9.x > 1.5) {
        if ((row & 1) == 1) {
            // Kopfschicht: halbe Steine, um ¼ Stein versetzt
            off = a * 0.25;
            pitch = a * 0.5;
            head = 1.0;
        } else if (p9.x > 2.5 && ((row >> 1) & 1) == 1) {
            off = a * 0.5;
        }
    } else if (p9.x > 0.4) {
        off = float(row & 1) * a * 0.5;
    } else {
        off = float(row - 3 * int(floor(float(row) / 3.0))) * a / 3.0;
    }
    float col = floor((u + off) / pitch);
    return vec4(col, u + off - col * pitch, pitch, head);
}
// Rissrillen (Regel 70) wie `texgen::grooves`: Höhe der Nulllinie
float groove_n(float u, float w, uint seed) {
    return vnoise(u, w, 34.0, 6.0, seed + 43u) + 0.35 * vnoise(u, w, 11.0, 3.0, seed + 44u);
}
float groove_dist(float u, float w, uint seed) {
    float d = abs(groove_n(u, w + 0.5, seed) - groove_n(u, w - 0.5, seed));
    return abs(groove_n(u, w, seed)) / max(d, 0.03);
}
// Mauerwerk (0–255): Familie nach den Anteilen (Köpfe nach Zeilen 13/14),
// Flammung roter Läufer, Streuung, Relief mit Gewicht `rw` (Ferne)
vec3 masonry_rgb(float u, float v, float px, vec4 p8, vec4 p9, vec4 p10, vec4 p11, vec4 p12, vec4 p13, vec4 p14) {
    uint seed = uint(p9.z + 0.5);
    float h = p8.z;
    float j = p8.w;
    float course = h + j;
    float rf = floor((v + j * 0.5) / course);
    int row = int(rf);
    float dv = v + j * 0.5 - rf * course;
    vec4 st = masonry_stone(u, row, p8, p9);
    int stone = int(st.x);
    float x = st.y;
    float pitch = st.z;
    bool head = st.w > 0.5;
    float uin = x - j * 0.5;
    float slen = pitch - j;
    float vin = dv - j;
    // Relief nur nah: Rillen unter 3 px Breite ausblenden (paket-7 §8.6)
    float rl = p12.z / 100.0 * smoothstep(1.5, 3.0, 1.4 / px);
    vec3 jc = unpack_rgb(p11.y) * 255.0;
    if (p12.z > 0.0) jc += rl / (p12.z / 100.0) * 6.0 * 1.7320508 * cellnoise(u, v, 1.5, seed + 13u);
    uint hs = pat_hash(row, stone, seed);
    int fam;
    vec3 c = head ? pat_pick(p13, p14, hs, fam) : pat_pick(p10, p11, hs, fam);
    if (p12.x > 0.0 && !head && fam == 0) {
        uint h2 = pat_hash(row, stone, seed + 5u);
        if (h01(h2, 0) < p12.x / 100.0) {
            float cen = 0.5 + 0.05 * gauss3(lowbias32(h2 + 1u));
            float wid = clamp(0.53 + 0.23 * gauss3(lowbias32(h2 + 2u)), 0.15, 0.85);
            bool silver = h01(h2, 16) >= p12.y / 100.0;
            float xl = (cen - wid * 0.5) * slen + 10.0 * vnoise(u, v, 40.0, 30.0, seed + 9u);
            float xr = (cen + wid * 0.5) * slen + 10.0 * vnoise(u, v, 40.0, 30.0, seed + 10u);
            float red = smoothstep(0.0, 1.0, (uin - xl) / 66.0 + 0.5) * smoothstep(0.0, 1.0, (xr - uin) / 66.0 + 0.5);
            float e = 1.0 - red;
            vec3 end = unpack_rgb(silver ? p11.x : p10.z) * 255.0;
            c = c * (1.0 - e) + end * e;
            if (e > 0.5) fam = silver ? 2 : 1;
        }
    }
    // Mit Relief normalverteilt (wie `masonry_rgb` in Rust)
    c *= p12.z > 0.0 ? 1.0 + p9.y / 100.0 * gauss3(lowbias32(hs ^ 0x27d4eb2du)) : spread_factor(hs, p9.y);
    if (rl > 0.0) {
        float k = fam == 2 ? 1.25 : 1.0;
        float fk = k * 14.0 * rl * 1.7320508 * cellnoise(u, v, 1.5, seed + 11u);
        float dk = 0.0;
        float rm = 0.0;
        if (vin > 1.5 && vin < h - 1.5 && uin > 1.5 && uin < slen - 1.5) {
            float seg = vnoise(u, v, 30.0, 14.0, seed + 45u) + 0.4 * vnoise(u, v, 9.0, 6.0, seed + 46u);
            float on = clamp((seg - 0.12) / 0.15, 0.0, 1.0) * rl;
            if (on > 0.0) {
                dk = clamp(1.0 - (groove_dist(u, v, seed) - 0.7) / 0.7, 0.0, 1.0) * on;
                float rim = clamp(1.0 - (groove_dist(u, v - 2.2, seed) - 1.0) / 0.7, 0.0, 1.0) * on;
                rm = clamp(rim - dk, 0.0, 1.0);
            }
        }
        bool sp = h01(pat_hash(int(floor(u / 1.5)), int(floor(v / 1.5)), seed + 12u), 0) < 0.07;
        float lift = 25.0 * rl * max(rm, sp ? 1.0 - dk : 0.0);
        c = (c + fk) * (1.0 - 0.75 * rl * dk) + lift;
    }
    // Kante weich: Abstand in den Stein hinein (mm)
    float inside = min(min(vin, course - dv), min(x, pitch - x) - j * 0.5);
    return mix(jc, c, clamp(inside / px + 0.5, 0.0, 1.0));
}
// Reibeputz (paket-7 §8.5) wie `texgen::plaster`: Höhe des Korns
float plaster_h(float a, float b, float lx, float ly, uint seed) {
    return vnoise(a, b, lx, ly, seed) + 0.6 * vnoise(a, b, lx * 0.5, ly * 0.5, seed + 1u)
        + 0.25 * vnoise(a, b, lx * 2.0, ly * 2.0, seed + 2u);
}
vec3 plaster_rgb(float u, float v, vec3 base, vec4 p9) {
    uint seed = uint(p9.z + 0.5);
    float lx = p9.w * 1.4;
    float ly = lx / 1.5;
    float d = ly * 0.3;
    float slope = (plaster_h(u, v + d, lx, ly, seed) - plaster_h(u, v - d, lx, ly, seed)) / (2.0 * d) * ly;
    float sh = max(0.75 * slope - 0.35 * plaster_h(u, v, lx, ly, seed) - 0.12, 0.0);
    float k = p9.y / 4.0 * 28.0;
    vec3 top = base + 6.0;
    return max(top - k * sh, top - 48.0);
}
// Sichtbeton (Regel 69) wie `texgen::concrete`; Feinkorn, Poren und Sand
// mit Gewicht `fine` (aus zwischen 1 und 0,5 px je mm, p7 §5)
vec3 concrete_rgb(float u, float v, float px, float fine, vec3 base, vec4 p8, vec4 p9, vec4 p12) {
    uint s = uint(p9.z + 0.5);
    float cl = p9.y / 2.0;
    float big = (1.1 * vnoise(u, v, 32.0, 32.0, s) + 0.8 * vnoise(u, v, 64.0, 64.0, s + 1u)
        + 0.7 * vnoise(u, v, 160.0, 160.0, s + 2u)) * cl * 1.6;
    float dl = big;
    float kl = 0.0;
    if (fine > 0.0) {
        float mid = (2.9 * vnoise(u, v, 4.0, 4.0, s + 3u) + 2.3 * vnoise(u, v, 8.0, 16.0, s + 4u)
            + 1.6 * vnoise(u, v, 16.0, 32.0, s + 5u)) * 1.6;
        float fd = mid + 6.0 * 1.7320508 * cellnoise(u, v, 1.0, s + 6u);
        float pores = p12.z;
        if (pores > 0.0) {
            float f = pores / 0.5;
            float gx = floor(u / 10.0);
            float gy = floor(v / 10.0);
            uint hh = pat_hash(int(gx), int(gy), s + 23u);
            if (h01(hh, 0) < 0.117 * f) {
                float cx = (gx + 0.3 + 0.4 * h01(hh, 8)) * 10.0;
                float cy = (gy + 0.3 + 0.4 * h01(hh, 16)) * 10.0;
                float dia = clamp(2.3 * exp(0.3 * gauss3(pat_hash(int(gx), int(gy), s + 29u))), 1.5, 4.5);
                kl = clamp(dia * 0.5 - length(vec2(u - cx, v - cy)) + 0.5, 0.0, 1.0);
            }
            float mx = floor(u / 5.0);
            float my = floor(v / 5.0);
            uint hm = pat_hash(int(mx), int(my), s + 31u);
            float pcx = (mx + 0.2 + 0.6 * h01(hm, 8)) * 5.0;
            float pcy = (my + 0.2 + 0.6 * h01(hm, 16)) * 5.0;
            float pr = 0.5 + 0.3 * h01(lowbias32(hm + 3u), 0);
            if (h01(hm, 0) < 0.18 * f && length(vec2(u - pcx, v - pcy)) < pr) fd -= 35.0;
            uint hsd = pat_hash(int(mx), int(my), s + 37u);
            float scx = (mx + 0.1 + 0.8 * h01(hsd, 8)) * 5.0;
            float scy = (my + 0.1 + 0.8 * h01(hsd, 16)) * 5.0;
            if (h01(hsd, 0) < 0.1425 * f && abs(u - scx) < 0.5 && abs(v - scy) < 0.5) fd += 25.0;
        }
        dl += fine * fd;
        kl *= fine;
    }
    float f = 1.0 - 0.65 * kl;
    float w = p8.y;
    float h = p8.z;
    float j = p8.w;
    if (j > 0.0) {
        float du = abs(u - floor(u / w + 0.5) * w);
        float dv = abs(v - floor(v / h + 0.5) * h);
        f *= 1.0 - 0.18 * clamp((j * 0.5 - min(du, dv)) / px + 0.5, 0.0, 1.0);
    }
    if (p9.x > 0.5) {
        float ax = u - w * 0.25;
        float ay = v - h * 0.5;
        ax -= floor(ax / (w * 0.5) + 0.5) * w * 0.5;
        ay -= floor(ay / h + 0.5) * h;
        if (ax * ax + ay * ay < 144.0) f *= 0.7;
    }
    return (base + dl) * f;
}
// Holzschalung wie `texgen::timber`: Dreieckswelle −1..1, Periode 1
float pat_wave(float x) {
    float t = 1.0 - 4.0 * abs(x - floor(x) - 0.5);
    return t * (1.5 - 0.5 * t * t);
}
vec3 timber_rgb(float u, float v, float px, vec4 p8, vec4 p9, vec4 p10) {
    uint seed = uint(p9.z + 0.5);
    float a = p8.w > 0.5 ? u : v;
    float b = p8.w > 0.5 ? v : u;
    float j = p8.z;
    float p = p8.y + j;
    float xa = a + j * 0.5;
    float kf = floor(xa / p);
    int k = int(kf);
    float ai = xa - kf * p;
    uint hh = pat_hash(k, 3, seed);
    vec3 c = unpack_rgb((hh & 1u) == 1u ? p10.x : p10.z) * 255.0;
    float br = 1.0 + 0.08 * (float((hh >> 8) & 255u) / 255.0 * 2.0 - 1.0);
    float t = ai - j;
    float warp = vnoise(t, b, 90.0, 90.0, seed + uint(k - 97 * int(floor(float(k) / 97.0))));
    float g = p9.x / 100.0 * pat_wave((t * 0.22 + warp * 5.0) / 6.2831853);
    float inside = min(ai - j, p - ai);
    float f = mix(0.45, 1.0, clamp(inside / px + 0.5, 0.0, 1.0));
    return c * br * (1.0 + g) * f;
}
// Platten wie `texgen::tile_at`
vec3 tiles_rgb(float u, float v, float px, vec4 p8, vec4 p9, vec4 p10, vec4 p11) {
    uint seed = uint(p9.z + 0.5);
    float j = p8.w;
    float pu = p8.y + j;
    float pv = p8.z + j;
    float y = v + j * 0.5;
    float row = floor(y / pv);
    float dv = y - row * pv;
    int r = int(row);
    float off = (p9.x > 0.5 && (r & 1) == 1) ? pu * 0.5 : 0.0;
    float x = u + off + j * 0.5;
    float col = floor(x / pu);
    float du = x - col * pu;
    uint hs = pat_hash(r, int(col), seed);
    int fam;
    vec3 c = pat_pick(p10, p11, hs, fam) * spread_factor(hs, p9.y);
    float inside = min(min(du - j, pu - du), min(dv - j, pv - dv));
    return mix(unpack_rgb(p11.y) * 255.0, c, clamp(inside / px + 0.5, 0.0, 1.0));
}
// Naturstein (Voronoi) wie `texgen::stone_point`/`stone_cell`
vec2 stone_point(int cx, int cy, float s, float irr, uint seed) {
    uint hh = pat_hash(cx, cy, seed);
    return vec2((float(cx) + 0.5 + (h01(hh, 0) - 0.5) * irr) * s, (float(cy) + 0.5 + (h01(hh, 16) - 0.5) * irr) * s);
}
// Nächste Zelle (xy) und Abstand zur Zellgrenze in mm (z): eine Suche über
// 3 × 3 Zellen, Grenze = nächste Mittelsenkrechte zu den übrigen Punkten
// (Review 3t), wie `texgen::stone_cell`
vec3 stone_cell(float u, float v, float s, float irr, uint seed) {
    int gx = int(floor(u / s));
    int gy = int(floor(v / s));
    vec2 x = vec2(u, v);
    vec2 pts[9];
    int a = 0;
    float bd = 1e30;
    for (int i = 0; i < 9; i++) {
        pts[i] = stone_point(gx + i % 3 - 1, gy + i / 3 - 1, s, irr, seed);
        vec2 d = pts[i] - x;
        float dd = dot(d, d);
        if (dd < bd) {
            bd = dd;
            a = i;
        }
    }
    vec2 pa = pts[a];
    float edge = 1e30;
    for (int i = 0; i < 9; i++) {
        vec2 n = pts[i] - pa;
        float l = length(n);
        if (i == a || l < 1e-6) continue;
        edge = min(edge, dot((pa + pts[i]) * 0.5 - x, n) / l);
    }
    return vec3(float(gx + a % 3 - 1), float(gy + a / 3 - 1), edge);
}
vec3 stone_rgb(float u, float v, float px, vec4 p8, vec4 p9, vec4 p10, vec4 p11) {
    uint seed = uint(p9.z + 0.5);
    vec3 cell = stone_cell(u, v, p8.y, p8.w / 100.0, seed);
    uint hs = pat_hash(int(cell.x), int(cell.y), seed + 1u);
    int fam;
    vec3 c = pat_pick(p10, p11, hs, fam) * spread_factor(hs, 4.0);
    return mix(unpack_rgb(p11.y) * 255.0, c, clamp((cell.z - p8.z * 0.5) / px + 0.5, 0.0, 1.0));
}
// Größe der Musterteile (mm) für das Ausblenden in die Ferne
float pattern_scale(int kind, vec4 p8, vec4 p9) {
    if (kind == 1) return p8.z + p8.w;
    if (kind == 2) return p9.w;
    if (kind == 3) return 32.0;
    if (kind == 4) return p8.y + p8.z;
    if (kind == 5) return min(p8.y, p8.z) + p8.w;
    // Naturstein früher in die Mischfarbe: ab 6 px je Stein (Review 3t)
    return p8.y * 0.25;
}
// Farbe in 3D (6b, 7a): `far` ist die Mischfarbe (Ferne), `surf` die
// Farbe der Oberfläche (Putz, Sichtbeton). Unter 1,5 px Musterteil ohne
// Hash (P3), bis 3 px weich (P5). Waagerechte Flächen: Platten und
// Naturstein mit Raster, Putz mit Korn, die übrigen in der Mischfarbe.
// Probe `--musterprobe`: feste Bildpunktgröße (mm) statt der Ableitung,
// 0 = aus
uniform float u_px_fix;
vec3 pattern_rgb(vec3 p, vec3 n, vec3 far, vec3 surf, vec4 p8, vec4 p9, vec4 p10, vec4 p11, vec4 p12, vec4 p13, vec4 p14) {
    vec3 q = pattern_uv(p, n);
    float px = max(length(vec2(dFdx(q.x), dFdy(q.x))), length(vec2(dFdx(q.y), dFdy(q.y))));
    px = max(px, 1e-6);
    if (u_px_fix > 0.0) px = u_px_fix;
    int kind = int(p8.x + 0.5);
    if (q.z < 0.5 && kind != 2 && kind != 5 && kind != 6) return far;
    float fade = smoothstep(1.5, 3.0, pattern_scale(kind, p8, p9) / px);
    if (fade <= 0.0) return far;
    vec3 c;
    if (kind == 1) {
        c = masonry_rgb(q.x, q.y, px, p8, p9, p10, p11, p12, p13, p14);
    } else if (kind == 2) {
        c = plaster_rgb(q.x, q.y, surf * 255.0, p9);
    } else if (kind == 3) {
        c = concrete_rgb(q.x, q.y, px, smoothstep(0.5, 1.0, 1.0 / px), surf * 255.0, p8, p9, p12);
    } else if (kind == 4) {
        c = timber_rgb(q.x, q.y, px, p8, p9, p10);
    } else if (kind == 5) {
        c = tiles_rgb(q.x, q.y, px, p8, p9, p10, p11);
    } else {
        c = stone_rgb(q.x, q.y, px, p8, p9, p10, p11);
    }
    return mix(far, clamp(c / 255.0, 0.0, 1.0), fade);
}
// Abstand (mm) zur nächsten Linie im Raster `pitch`
float grid_dist(float x, float pitch) {
    return abs(x - floor(x / pitch + 0.5) * pitch);
}
// Fugenlinien der Ansicht wie `joint_lines`: Tinte 0..1 mit Stiftbreite
// `w` px; weich aus zwischen 3 und 1,5 px Musterteil, darunter ohne
// Rechnung (P3)
float pattern_lines(vec3 p, vec3 n, vec4 p8, vec4 p9, float w) {
    vec3 q = pattern_uv(p, n);
    int kind = int(p8.x + 0.5);
    if (kind == 2 || (kind == 3 && p8.w <= 0.0)) return 0.0;
    if (q.z < 0.5 && kind != 5 && kind != 6) return 0.0;
    float px = max(length(vec2(dFdx(q.x), dFdy(q.x))), length(vec2(dFdx(q.y), dFdy(q.y))));
    px = max(px, 1e-6);
    float fade = smoothstep(1.5, 3.0, (kind == 3 ? min(p8.y, p8.z) : pattern_scale(kind, p8, p9)) / px);
    if (fade <= 0.0) return 0.0;
    float d;
    if (kind == 1) {
        float course = p8.z + p8.w;
        float rf = floor(q.y / course);
        float dv = min(q.y - rf * course, (rf + 1.0) * course - q.y);
        vec4 st = masonry_stone(q.x, int(rf), p8, p9);
        d = min(min(st.y, st.z - st.y), dv);
    } else if (kind == 3) {
        d = min(grid_dist(q.x, p8.y), grid_dist(q.y, p8.z));
    } else if (kind == 4) {
        d = grid_dist(p8.w > 0.5 ? q.x : q.y, p8.y + p8.z);
    } else if (kind == 5) {
        float pu = p8.y + p8.w;
        float pv = p8.z + p8.w;
        float rf = floor(q.y / pv);
        float off = (p9.x > 0.5 && (int(rf) & 1) == 1) ? pu * 0.5 : 0.0;
        d = min(grid_dist(q.x + off, pu), grid_dist(q.y, pv));
    } else {
        d = max(stone_cell(q.x, q.y, p8.y, p8.w / 100.0, uint(p9.z + 0.5)).z, 0.0);
    }
    return clamp(w * 0.5 + 0.5 - d / px, 0.0, 1.0) * fade;
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
// Kantenart der feinen Linie (sk_model::edge_kind::FINE)
const int FINE = 2;
flat out vec3 v_color;
// Strichmuster: Lage längs der Kante (px ab Anfang), Länge, Breite
noperspective out float v_dist;
flat out vec4 v_p0;
flat out vec4 v_p1;
flat out vec2 v_len_w;
// Höhe im Modell (mm) für „unter dem Gelände“ (S11)
noperspective out float v_z;
// Farbe gestrichelt unter dem Gelände: die feine Linie (Stift „Fein“)
flat out vec3 v_color_below;
uniform int u_below;
uniform float u_terrain;
void main() {
    int k = clamp(int(a_kind + 0.5), 0, 7);
    // Unter dem Gelände und gestrichelt: Breite und Farbe der feinen Linie
    // (Kantenart FINE), nur der Strich unter dem Gelände (S11, §8 14:25).
    // Kanten, die das Gelände kreuzen, teilt der Netzbau an OK Gelände.
    bool unter = u_below == 2 && max(a_a.z, a_b.z) < u_terrain + 0.5
        && min(a_a.z, a_b.z) < u_terrain - 0.5;
    int kl = unter ? FINE : k;
    v_color = u_edge_color[kl];
    v_color_below = u_edge_color[FINE];
    v_p0 = unter ? vec4(0.0) : u_edge_dash[2 * k];
    v_p1 = unter ? vec4(0.0) : u_edge_dash[2 * k + 1];
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
    float w = u_edge_width[kl];
    vec2 off = (n * a_corner.y + d * (at_b ? 1.0 : -1.0)) * (w * 0.5);
    v_dist = at_b ? len + w * 0.5 : -w * 0.5;
    v_len_w = vec2(len, w);
    v_z = at_b ? a_b.z : a_a.z;
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
noperspective in float v_z;
flat in vec3 v_color_below;
out vec4 o_color;
uniform float u_alpha;
// Ansichten (S11): unter dem Gelände 1 ausgeblendet, 2 gestrichelt (Strich,
// Lücke px)
uniform int u_below;
uniform vec2 u_below_dash;
uniform float u_terrain;
// Blasse Kanten im Zeichenmodus: mit dem Papier (rgb) vorgemischt und
// deckend (a = 1), damit sich deckungsgleiche Kanten nicht stapeln
uniform vec4 u_premix;
void main() {
    if (v_p0.x + v_p0.y > 0.0 && !dash_ink(v_dist, v_len_w.x, v_p0, v_p1, v_len_w.y)) discard;
    vec3 color = v_color;
    if (u_below != 0 && v_z < u_terrain - 0.5) {
        if (u_below == 1) discard;
        float per = u_below_dash.x + u_below_dash.y;
        if (per > 0.0 && mod(max(v_dist, 0.0), per) >= u_below_dash.x) discard;
        color = v_color_below;
    }
    if (u_premix.a > 0.5) {
        o_color = vec4(mix(u_premix.rgb, color, u_alpha), 1.0);
    } else {
        o_color = vec4(color, u_alpha);
    }
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
uniform float u_alpha;
// 1: Zeilen von unten (gezeichnetes Bild, Vorschau „Muster“)
uniform float u_flip;
void main() {
    if (u_flip > 0.5) {
        // Teilbilder decken ihren Bereich; die Lücken verdeckt das Fenster
        o_color = vec4(texture(u_tex, v_ndc * 0.5 + 0.5).rgb, 1.0) * u_alpha;
        return;
    }
    o_color = texture(u_tex, vec2(v_ndc.x * 0.5 + 0.5, 0.5 - v_ndc.y * 0.5)) * u_alpha;
}
"#;

/// Festgehaltenes Bild, um `u_offset` (Anteil der Höhe) nach unten und
/// `u_offset_x` (Anteil der Breite) nach rechts versetzt. Beim Wenden
/// (`u_turn` > 0) waagerecht um `u_squash` gestaucht (Achse `u_axis` in NDC),
/// die Zeichnung abgedunkelt um `u_shade`, daneben deckend der Grund aus der
/// Bildecke.
const SNAPSHOT_FS: &str = r#"#version 330 core
in vec2 v_ndc;
out vec4 o_color;
uniform sampler2D u_tex;
uniform float u_alpha;
uniform float u_offset;
uniform float u_offset_x;
uniform float u_turn;
uniform float u_squash;
uniform float u_axis;
uniform float u_shade;
void main() {
    if (u_turn > 0.0) {
        float x = u_axis + (v_ndc.x - u_axis) / max(u_squash, 1e-4);
        vec3 ground = texture(u_tex, vec2(0.002, 0.998)).rgb;
        if (abs(x) > 1.0) {
            o_color = vec4(ground, 1.0);
        } else {
            // Nur die Zeichnung dunkelt ab, der Grund bleibt
            vec3 c = texture(u_tex, vec2(x * 0.5 + 0.5, v_ndc.y * 0.5 + 0.5)).rgb;
            bool bare = all(lessThan(abs(c - ground), vec3(0.01)));
            o_color = vec4(bare ? c : c * u_shade, 1.0);
        }
        return;
    }
    vec2 uv = vec2(v_ndc.x * 0.5 + 0.5 - u_offset_x, v_ndc.y * 0.5 + 0.5 + u_offset);
    if (uv.y < 0.0 || uv.y > 1.0 || uv.x < 0.0 || uv.x > 1.0) discard;
    o_color = vec4(texture(u_tex, uv).rgb * u_alpha, u_alpha);
}
"#;

/// Ersatz für [`PATTERN_GLSL`], wenn der Treiber die Muster nicht übersetzt:
/// dieselben Funktionen, die der Flächen-Shader ruft, ohne Muster und ohne
/// Ganzzahl-Bitrechnung.
pub const PATTERN_FALLBACK_GLSL: &str = r#"
vec3 unpack_rgb(float f) {
    float r = floor(f / 65536.0);
    float g = floor((f - r * 65536.0) / 256.0);
    float b = f - r * 65536.0 - g * 256.0;
    return vec3(r, g, b) / 255.0;
}
float pattern_lines(vec3 p, vec3 n, vec4 p8, vec4 p9, float w) {
    return 0.0;
}
vec3 pattern_rgb(vec3 p, vec3 n, vec3 far, vec3 surf, vec4 p8, vec4 p9, vec4 p10, vec4 p11, vec4 p12, vec4 p13, vec4 p14) {
    return far;
}
"#;

impl Renderer {
    /// Konnte der Flächen-Shader die Muster nicht übersetzen, die Meldung
    /// des Treibers (zum Anzeigen); es wird ohne Muster gezeichnet.
    pub fn pattern_error(&self) -> Option<&str> {
        self.pattern_error.as_deref()
    }

    pub fn new(gl: Gl, style: Style) -> Result<Renderer, String> {
        unsafe {
            // Schatten in Himmel und Flächen (S5); übersetzt der Treiber sie
            // nicht, geht es ohne Schatten weiter
            let mut shadow_error = None;
            let mit_schatten = |fs: &dyn Fn(&str) -> String,
                                vs: &str,
                                fehler: &mut Option<String>|
             -> Result<Program, String> {
                match program(&gl, vs, &fs(SCHATTEN_GLSL)) {
                    Ok(p) => Ok(p),
                    Err(e) => {
                        let p = program(&gl, vs, &fs(SCHATTEN_AUS_GLSL))?;
                        fehler.get_or_insert(e);
                        Ok(p)
                    }
                }
            };
            let sky = mit_schatten(
                &|sh| format!("#version 330 core\n{sh}{SKY_FS}"),
                FULLSCREEN_VS,
                &mut shadow_error,
            )?;
            // Muster im Flächen-Shader; scheitert der Treiber daran, zeichnet
            // das Programm ohne Muster weiter statt nicht zu starten (3q)
            let face_fs = |muster: &'static str| {
                move |sh: &str| format!("#version 330 core\n{muster}{sh}{FACE_FS}")
            };
            let mut ohne = None;
            let (faces, pattern_error) =
                match mit_schatten(&face_fs(PATTERN_GLSL), FACE_VS, &mut ohne) {
                    Ok(p) => (p, None),
                    Err(e) => (
                        mit_schatten(&face_fs(PATTERN_FALLBACK_GLSL), FACE_VS, &mut shadow_error)?,
                        Some(e),
                    ),
                };
            if let Some(e) = ohne {
                shadow_error.get_or_insert(e);
            }
            let edges = program(&gl, EDGE_VS, &with_dash(EDGE_FS))?;
            let overlay = program(&gl, FULLSCREEN_VS, OVERLAY_FS)?;
            let helpers = program(&gl, HELPER_VS, &with_dash(HELPER_FS))?;
            let snap_prog = program(&gl, FULLSCREEN_VS, SNAPSHOT_FS)?;
            let shadow_prog = program(&gl, SHADOW_VS, SHADOW_FS)?;
            let shadow_dummy = depth_texture(&gl, 1, &[u32::MAX]);
            gl.glActiveTexture(TEXTURE0 + SHADOW_UNIT);
            gl.glBindTexture(TEXTURE_2D, shadow_dummy);
            gl.glActiveTexture(TEXTURE0);
            let mut shadow_max = 0;
            gl.glGetIntegerv(MAX_TEXTURE_SIZE, &mut shadow_max);
            let mut vao = 0u32;
            gl.glGenVertexArrays(1, &mut vao);
            let mut max_samples = 0;
            gl.glGetIntegerv(MAX_SAMPLES, &mut max_samples);
            Ok(Renderer {
                gl,
                sky,
                faces,
                pattern_error,
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
                bond_tex: 0,
                looks: Looks::default(),
                snap_prog,
                snapshot: None,
                ghost: None,
                preview: None,
                preview_dirty: false,
                preview_meshes: Vec::new(),
                preview_looks: Looks::default(),
                preview_looks_tex: 0,
                preview_bond_tex: 0,
                preview_target: None,
                preview_fade: 0.0,
                preview_exact: false,
                shadow_prog,
                shadow_map: None,
                shadow_dummy,
                shadow_max,
                sun: None,
                shadow_karte: None,
                shadow_dirty: false,
                paper_shade: None,
                shadow_paper: false,
                below: (0, [0.0; 2]),
                terrain: 0.0,
                shadow_failed: shadow_error.is_some(),
                shadow_error: shadow_error.map(|e| format!("Schatten aus: {e}")),
                shadow_query: 0,
                shadow_query_open: None,
                shadow_ms: None,
                shadow_ms_voll: None,
                shadow_entwurf: None,
                mesh_bounds: Vec::new(),
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
        unsafe {
            upload_looks(&self.gl, &mut self.looks_tex, looks);
            if self.bond_tex == 0 || looks.bond != self.looks.bond {
                upload_bond(&self.gl, &mut self.bond_tex, &looks.bond);
            }
        }
        self.looks = looks.clone();
    }

    /// Netz in Platz `slot` blass mit Deckkraft `alpha` zeichnen (Isolieren,
    /// Paket 3): nach allem Deckenden, nur die vorderste blasse Fläche je
    /// Bildpunkt, ohne Schraffur. `None`: alle Plätze deckend.
    pub fn set_ghost(&mut self, ghost: Option<(usize, f32)>) {
        self.ghost = ghost;
    }

    /// Ersetzt das Netz in Platz `slot` (z. B. 0 = Modell, 1 = Vorschau).
    pub fn set_mesh(&mut self, slot: usize, mesh: &MeshData) {
        while self.meshes.len() <= slot {
            self.meshes.push(GpuMesh::default());
            self.mesh_bounds.push(None);
        }
        unsafe { upload_mesh(&self.gl, &mut self.meshes[slot], mesh) };
        self.mesh_bounds[slot] = huelle(&mesh.faces);
        self.shadow_dirty = true;
    }

    /// Sonne in der 3D-Ansicht (S5); `None` ohne Sonne (festes Licht aus
    /// dem Stil, ohne Schatten).
    pub fn set_sun(&mut self, sun: Option<SunLight>) {
        if sun != self.sun {
            self.sun = sun;
            self.shadow_dirty = true;
        }
    }

    /// Schatten der Ansicht im Papiermodus (S7); `None`: ohne.
    pub fn set_paper_shade(&mut self, s: Option<PaperShade>) {
        // Die Karte hängt nur am Licht; Ton und Schraffur nicht (Review 3cc)
        let licht = |p: &Option<PaperShade>| p.map(|p| p.zum_licht);
        if licht(&s) != licht(&self.paper_shade) {
            self.shadow_dirty = true;
        }
        self.paper_shade = s;
    }

    /// Richtung zum Licht für die Karte dieses Bildes: auf Papier das
    /// Licht der Ansicht, sonst die Sonne.
    fn shadow_licht(&self) -> Option<[f64; 3]> {
        if self.shadow_paper {
            self.paper_shade.map(|p| p.zum_licht)
        } else {
            self.sun.map(|s| s.zur_sonne)
        }
    }

    /// Karte für das nächste Bild: aus dem Licht und dem Hüllquader der
    /// deckenden Netze (ohne das blasse).
    fn shadow_karte(&self) -> Option<schatten::Karte> {
        let d = self.shadow_licht().filter(|_| !self.shadow_failed)?;
        let ghost = self.ghost.map(|g| g.0);
        let q = self
            .mesh_bounds
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != ghost)
            .filter_map(|(_, b)| *b)
            .reduce(|(a, b), (c, d)| {
                (
                    sk_math::vec3(a.x.min(c.x), a.y.min(c.y), a.z.min(c.z)),
                    sk_math::vec3(b.x.max(d.x), b.y.max(d.y), b.z.max(d.z)),
                )
            })?;
        let n = self.shadow_entwurf.unwrap_or(self.shadow_voll());
        schatten::karte(sk_math::vec3(d[0], d[1], d[2]), q, n)
    }

    /// Volle Kartengröße auf diesem Treiber.
    fn shadow_voll(&self) -> u32 {
        (schatten::GROESSE as i32).min(self.shadow_max.max(1024)) as u32
    }

    /// Beim Ziehen an Sonne oder Schatten (S6): Hat der Tiefen-Durchgang
    /// in voller Größe zuletzt mehr als [`schatten::ENTWURF_AB_MS`]
    /// gebraucht, gilt beim Greifen [`schatten::ENTWURF`], und zwar bis
    /// zum Loslassen, ohne Wechsel mitten im Zug.
    pub fn set_shadow_draft(&mut self, on: bool) {
        if !on {
            self.shadow_entwurf = None;
        } else if self.shadow_entwurf.is_none() {
            let voll = self.shadow_voll();
            let lang = self
                .shadow_ms_voll
                .is_some_and(|ms| ms > schatten::ENTWURF_AB_MS);
            self.shadow_entwurf = Some(if lang {
                voll.min(schatten::ENTWURF)
            } else {
                voll
            });
        }
    }

    /// Neue Messung des Tiefen-Durchgangs auf der Grafikkarte, einmal:
    /// Kartengröße und Zeit (ms). Sie stammt aus einem früheren Bild und
    /// wird gelesen, sobald die Grafikkarte sie meldet.
    pub fn take_shadow_pass_ms(&mut self) -> Option<(u32, f64)> {
        self.shadow_ms.take()
    }

    /// Kann dieser Treiber keine Schatten (S5)?
    pub fn shadow_failed(&self) -> bool {
        self.shadow_failed
    }

    /// Meldung, wenn die Schattenkarte nicht angelegt werden konnte (einmal).
    pub fn take_shadow_error(&mut self) -> Option<String> {
        self.shadow_error.take()
    }

    /// Schattenkarte zeichnen, wenn sie sich geändert hat; danach liegt
    /// sie (oder die leere) auf [`SHADOW_UNIT`].
    unsafe fn free_shadow_map(&mut self) {
        if let Some(m) = self.shadow_map.take() {
            self.gl.glDeleteFramebuffers(1, &m.fbo);
            self.gl.glDeleteTextures(1, &m.tex);
        }
    }

    unsafe fn render_shadow(&mut self) {
        // Ergebnis der letzten Zeitabfrage, sobald es da ist (ohne Warten)
        if let Some(n) = self.shadow_query_open {
            let gl = &self.gl;
            let mut da = 0u64;
            gl.glGetQueryObjectui64v(self.shadow_query, QUERY_RESULT_AVAILABLE, &mut da);
            if da != 0 {
                let mut ns = 0u64;
                gl.glGetQueryObjectui64v(self.shadow_query, QUERY_RESULT, &mut ns);
                let ms = ns as f64 * 1e-6;
                self.shadow_ms = Some((n, ms));
                if n == self.shadow_voll() {
                    self.shadow_ms_voll = Some(ms);
                }
                self.shadow_query_open = None;
            }
        }
        let karte = self.shadow_karte();
        let neu = karte != self.shadow_karte || self.shadow_dirty;
        self.shadow_karte = karte;
        self.shadow_dirty = false;
        // Ohne Licht keine Karte im Speicher (§8 10:50, Nachtrag 11:17)
        if self.shadow_licht().is_none() {
            self.free_shadow_map();
        }
        let gl = &self.gl;
        let Some(k) = karte else {
            gl.glActiveTexture(TEXTURE0 + SHADOW_UNIT);
            gl.glBindTexture(TEXTURE_2D, self.shadow_dummy);
            gl.glActiveTexture(TEXTURE0);
            return;
        };
        let n = k.groesse as i32;
        if self.shadow_map.as_ref().is_none_or(|m| m.size != n) {
            if let Some(m) = self.shadow_map.take() {
                gl.glDeleteFramebuffers(1, &m.fbo);
                gl.glDeleteTextures(1, &m.tex);
            }
            let tex = depth_texture(gl, n, &[]);
            let mut fbo = 0;
            gl.glGenFramebuffers(1, &mut fbo);
            gl.glBindFramebuffer(FRAMEBUFFER, fbo);
            gl.glFramebufferTexture2D(FRAMEBUFFER, DEPTH_ATTACHMENT, TEXTURE_2D, tex, 0);
            gl.glDrawBuffer(NONE);
            gl.glReadBuffer(NONE);
            let status = gl.glCheckFramebufferStatus(FRAMEBUFFER);
            if status != FRAMEBUFFER_COMPLETE {
                gl.glDeleteFramebuffers(1, &fbo);
                gl.glDeleteTextures(1, &tex);
                gl.glBindFramebuffer(FRAMEBUFFER, 0);
                // Ohne Karte bleibt es hell; nicht jedes Bild erneut versuchen
                self.shadow_failed = true;
                self.shadow_karte = None;
                self.shadow_error = Some(format!(
                    "Schatten aus: Schattenkarte unvollständig (Status {status:#x})."
                ));
                gl.glActiveTexture(TEXTURE0 + SHADOW_UNIT);
                gl.glBindTexture(TEXTURE_2D, self.shadow_dummy);
                gl.glActiveTexture(TEXTURE0);
                return;
            }
            self.shadow_map = Some(ShadowMap { fbo, tex, size: n });
        }
        let m = self.shadow_map.as_ref().unwrap();
        if neu {
            let messen = self.shadow_query_open.is_none();
            if messen {
                if self.shadow_query == 0 {
                    gl.glGenQueries(1, &mut self.shadow_query);
                }
                gl.glBeginQuery(TIME_ELAPSED, self.shadow_query);
                self.shadow_query_open = Some(n as u32);
            }
            gl.glBindFramebuffer(FRAMEBUFFER, m.fbo);
            gl.glViewport(0, 0, n, n);
            gl.glDisable(SCISSOR_TEST);
            gl.glDisable(BLEND);
            gl.glDisable(CULL_FACE);
            gl.glDisable(MULTISAMPLE);
            gl.glEnable(DEPTH_TEST);
            gl.glDepthFunc(LESS);
            gl.glDepthMask(TRUE);
            gl.glClearDepth(1.0);
            gl.glClear(DEPTH_BUFFER_BIT);
            gl.glEnable(POLYGON_OFFSET_FILL);
            gl.glPolygonOffset(1.0, 2.0);
            let p = self.shadow_prog.id;
            gl.glUseProgram(p);
            mat(gl, p, c"u_m", &k.matrix.to_f32());
            vec3(gl, p, c"u_c", k.mitte.to_f32());
            let ghost = self.ghost.map(|g| g.0);
            for (i, gm) in self.meshes.iter().enumerate() {
                if Some(i) != ghost && gm.faces.count > 0 {
                    gl.glBindVertexArray(gm.faces.vao);
                    gl.glDrawArrays(TRIANGLES, 0, gm.faces.count);
                }
            }
            gl.glDisable(POLYGON_OFFSET_FILL);
            gl.glBindFramebuffer(FRAMEBUFFER, 0);
            if messen {
                gl.glEndQuery(TIME_ELAPSED);
            }
        }
        gl.glActiveTexture(TEXTURE0 + SHADOW_UNIT);
        gl.glBindTexture(TEXTURE_2D, m.tex);
        gl.glActiveTexture(TEXTURE0);
    }

    /// Schatten-Uniforms für Himmel (`c` relativ zum Auge) oder Flächen
    /// (`c` im Modell); ohne Karte aus.
    unsafe fn shadow_uniforms(&self, p: GLuint, c: [f32; 3]) {
        let gl = &self.gl;
        gl.glUniform1i(loc(gl, p, c"u_shadow"), SHADOW_UNIT as GLint);
        let Some(k) = self.shadow_karte.filter(|_| self.shadow_map.is_some()) else {
            gl.glUniform1i(loc(gl, p, c"u_shadow_on"), 0);
            return;
        };
        gl.glUniform1i(loc(gl, p, c"u_shadow_on"), 1);
        mat(gl, p, c"u_shadow_m", &k.matrix.to_f32());
        gl.glUniform1f(loc(gl, p, c"u_shadow_size"), k.groesse as f32);
        vec3(gl, p, c"u_shadow_sun", k.zur_sonne.to_f32());
        gl.glUniform2f(
            loc(gl, p, c"u_shadow_offset"),
            (schatten::VERSATZ_NORMALE * k.texel) as f32,
            (schatten::VERSATZ_SONNE * k.texel) as f32,
        );
        vec3(gl, p, c"u_shadow_c", c);
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
                tw: 0,
                th: 0,
                alpha: 1.0,
            });
        }
        let o = &mut self.overlays[slot];
        (o.x, o.y, o.w, o.h) = (x, y, width as i32, height as i32);
        (o.tw, o.th, o.alpha) = (width as i32, height as i32, 1.0);
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

    /// Lage, gezeichnete Größe und Deckkraft eines schon hochgeladenen
    /// Bildes, ohne es neu zu übertragen (Animationen). Weicht die Größe vom
    /// Bild ab, wird es geglättet gestreckt.
    pub fn place_overlay(&mut self, slot: usize, x: i32, y: i32, w: i32, h: i32, alpha: f32) {
        if let Some(o) = self.overlays.get_mut(slot) {
            (o.x, o.y, o.w, o.h) = (x, y, w, h);
            o.alpha = alpha.clamp(0.0, 1.0);
        }
    }

    /// Größe des hochgeladenen Bildes (0, 0 ohne Bild).
    pub fn overlay_size(&self, slot: usize) -> (i32, i32) {
        self.overlays.get(slot).map_or((0, 0), |o| (o.tw, o.th))
    }

    /// Hält das zuletzt gezeichnete Bild der Modellansicht fest (ohne
    /// Oberfläche), um es beim nächsten Bildern überzublenden.
    /// Aussehens-Tabelle der Vorschau (Schlüssel der Vorschaunetze).
    pub fn set_preview_looks(&mut self, looks: &Looks) {
        if *looks == self.preview_looks && self.preview_looks_tex != 0 {
            return;
        }
        unsafe {
            upload_looks(&self.gl, &mut self.preview_looks_tex, looks);
            if self.preview_bond_tex == 0 || looks.bond != self.preview_looks.bond {
                upload_bond(&self.gl, &mut self.preview_bond_tex, &looks.bond);
            }
        }
        self.preview_looks = looks.clone();
        self.preview_dirty = true;
    }

    /// Netz `slot` der Vorschau.
    pub fn set_preview_mesh(&mut self, slot: usize, mesh: &MeshData) {
        while self.preview_meshes.len() <= slot {
            self.preview_meshes.push(GpuMesh::default());
        }
        let gm = &mut self.preview_meshes[slot];
        unsafe { upload_mesh(&self.gl, gm, mesh) };
        self.preview_dirty = true;
    }

    /// Vorschau zeigen (neu zeichnen beim nächsten Bild) oder ausblenden.
    pub fn set_preview(&mut self, p: Option<Preview>) {
        self.preview = p;
        self.preview_dirty = true;
    }

    /// Hält das jetzige Vorschaubild fest; es liegt mit Deckkraft
    /// [`Renderer::set_preview_fade`] über dem neuen (Übergang beim
    /// Vorlagenwechsel in `anim_ms`).
    pub fn hold_preview(&mut self) {
        if let Some(t) = &self.preview_target {
            let gl = &self.gl;
            unsafe {
                gl.glBindFramebuffer(READ_FRAMEBUFFER, t.fbo);
                gl.glBindFramebuffer(DRAW_FRAMEBUFFER, t.prev_fbo);
                gl.glBlitFramebuffer(
                    0,
                    0,
                    t.w,
                    t.h,
                    0,
                    0,
                    t.w,
                    t.h,
                    COLOR_BUFFER_BIT,
                    NEAREST as GLenum,
                );
                gl.glBindFramebuffer(FRAMEBUFFER, 0);
            }
            self.preview_fade = 1.0;
        }
    }

    pub fn set_preview_fade(&mut self, alpha: f32) {
        self.preview_fade = alpha.clamp(0.0, 1.0);
    }

    /// Verschiebt die Vorschau mit dem Fenster, ohne neu zu zeichnen.
    pub fn move_preview(&mut self, x: i32, y: i32) {
        if let Some(p) = &mut self.preview {
            (p.at[0], p.at[1]) = (x, y);
        }
    }

    /// Bild der Vorschau in der Größe `w` × `h` bereitstellen.
    fn ensure_preview_target(&mut self, w: i32, h: i32) -> Result<(), String> {
        if self
            .preview_target
            .as_ref()
            .is_some_and(|t| (t.w, t.h) == (w, h))
        {
            return Ok(());
        }
        let gl = &self.gl;
        unsafe {
            if let Some(t) = self.preview_target.take() {
                gl.glDeleteFramebuffers(1, &t.ms_fbo);
                gl.glDeleteRenderbuffers(2, t.ms.as_ptr());
                gl.glDeleteFramebuffers(1, &t.fbo);
                gl.glDeleteTextures(1, &t.tex);
                gl.glDeleteFramebuffers(1, &t.prev_fbo);
                gl.glDeleteTextures(1, &t.prev_tex);
            }
            let (mut ms_fbo, mut ms) = (0, [0u32; 2]);
            gl.glGenFramebuffers(1, &mut ms_fbo);
            gl.glGenRenderbuffers(2, ms.as_mut_ptr());
            gl.glBindFramebuffer(FRAMEBUFFER, ms_fbo);
            gl.glBindRenderbuffer(RENDERBUFFER, ms[0]);
            gl.glRenderbufferStorageMultisample(RENDERBUFFER, self.samples, RGBA8, w, h);
            gl.glFramebufferRenderbuffer(FRAMEBUFFER, COLOR_ATTACHMENT0, RENDERBUFFER, ms[0]);
            gl.glBindRenderbuffer(RENDERBUFFER, ms[1]);
            gl.glRenderbufferStorageMultisample(
                RENDERBUFFER,
                self.samples,
                DEPTH_COMPONENT24,
                w,
                h,
            );
            gl.glFramebufferRenderbuffer(FRAMEBUFFER, DEPTH_ATTACHMENT, RENDERBUFFER, ms[1]);
            let status = gl.glCheckFramebufferStatus(FRAMEBUFFER);
            // Bild und voriges Bild als Texturen mit eigenem Puffer
            let target = |gl: &Gl| {
                let (mut fbo, mut tex) = (0, 0);
                gl.glGenTextures(1, &mut tex);
                gl.glBindTexture(TEXTURE_2D, tex);
                gl.glTexImage2D(
                    TEXTURE_2D,
                    0,
                    RGBA8 as GLint,
                    w,
                    h,
                    0,
                    RGBA,
                    UNSIGNED_BYTE,
                    std::ptr::null(),
                );
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE);
                gl.glGenFramebuffers(1, &mut fbo);
                gl.glBindFramebuffer(FRAMEBUFFER, fbo);
                gl.glFramebufferTexture2D(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, tex, 0);
                gl.glClearColor(0.0, 0.0, 0.0, 0.0);
                gl.glClear(COLOR_BUFFER_BIT);
                (fbo, tex)
            };
            let (fbo, tex) = target(gl);
            let (prev_fbo, prev_tex) = target(gl);
            gl.glBindFramebuffer(FRAMEBUFFER, 0);
            self.preview_target = Some(PreviewTarget {
                ms_fbo,
                ms,
                fbo,
                tex,
                prev_fbo,
                prev_tex,
                w,
                h,
            });
            if status != FRAMEBUFFER_COMPLETE {
                return Err(format!(
                    "Vorschaupuffer unvollständig (Status {status:#x})."
                ));
            }
        }
        Ok(())
    }

    /// Zeichnet die Teilbilder der Vorschau in ihr Bild (nur nach einer
    /// Änderung).
    fn render_preview(&mut self) -> Result<(), String> {
        let Some(p) = self.preview.clone() else {
            return Ok(());
        };
        let (w, h) = (p.at[2].max(1), p.at[3].max(1));
        self.ensure_preview_target(w, h)?;
        let Some(tg) = self.preview_target.as_ref() else {
            return Ok(());
        };
        let gl = &self.gl;
        let st = &self.style;
        let looks = &self.preview_looks;
        unsafe {
            gl.glBindFramebuffer(FRAMEBUFFER, tg.ms_fbo);
            gl.glViewport(0, 0, w, h);
            gl.glDisable(SCISSOR_TEST);
            gl.glDisable(BLEND);
            gl.glDisable(CULL_FACE);
            gl.glEnable(MULTISAMPLE);
            gl.glClearColor(0.0, 0.0, 0.0, 0.0);
            gl.glClearDepth(1.0);
            gl.glDepthMask(TRUE);
            gl.glClear(COLOR_BUFFER_BIT | DEPTH_BUFFER_BIT);
            gl.glEnable(SCISSOR_TEST);
            gl.glEnable(DEPTH_TEST);
            for it in &p.items {
                let Some(m) = self.preview_meshes.get(it.mesh) else {
                    continue;
                };
                let [rx, ry, rw, rh] = it.rect;
                let vy = h - ry - rh;
                gl.glViewport(rx, vy, rw, rh);
                let [cx, cy, cw, ch] = it.clip.unwrap_or(it.rect);
                gl.glScissor(cx, h - cy - ch, cw, ch);
                let view = &it.view;
                if let Some(c) = view.paper {
                    gl.glClearColor(c[0], c[1], c[2], 1.0);
                    gl.glClear(COLOR_BUFFER_BIT);
                    gl.glClearColor(0.0, 0.0, 0.0, 0.0);
                }
                if it.sky && view.paper.is_none() {
                    gl.glDepthFunc(ALWAYS);
                    let q = self.sky.id;
                    gl.glUseProgram(q);
                    mat(gl, q, c"u_inv_vp", &view.inv_view_proj);
                    mat(gl, q, c"u_vp", &view.view_proj);
                    // Boden auf OK Gelände: das Auge steht um so viel höher darüber
                    gl.glUniform1f(loc(gl, q, c"u_eye_z"), view.eye_z - self.terrain);
                    // gl_FragCoord zählt im ganzen Bild: Horizont um den Bereich versetzt
                    gl.glUniform1f(loc(gl, q, c"u_horizon_px"), view.horizon_px + vy as f32);
                    gl.glUniform1f(loc(gl, q, c"u_height"), rh as f32);
                    gl.glUniform1f(loc(gl, q, c"u_softness"), st.horizon_softness);
                    vec3(gl, q, c"u_ground", st.ground);
                    let n = st.sky.len().min(SKY_MAX);
                    let mut pos = [0.0f32; SKY_MAX];
                    let mut col = [0.0f32; 3 * SKY_MAX];
                    for (i, s) in st.sky[..n].iter().enumerate() {
                        pos[i] = s.0;
                        col[3 * i..3 * i + 3].copy_from_slice(&s.1);
                    }
                    gl.glUniform1i(loc(gl, q, c"u_sky_n"), n as i32);
                    gl.glUniform1fv(loc(gl, q, c"u_sky_pos"), n as i32, pos.as_ptr());
                    gl.glUniform3fv(loc(gl, q, c"u_sky_col"), n as i32, col.as_ptr());
                    gl.glUniform1i(loc(gl, q, c"u_overlay"), 0);
                    // Vorschau ohne Schatten
                    gl.glUniform1i(loc(gl, q, c"u_shadow_on"), 0);
                    gl.glUniform1f(loc(gl, q, c"u_shade"), 1.0);
                    gl.glBindVertexArray(self.empty_vao);
                    gl.glDrawArrays(TRIANGLES, 0, 3);
                }
                // Flächen
                gl.glDepthFunc(LESS);
                gl.glEnable(POLYGON_OFFSET_FILL);
                gl.glPolygonOffset(1.0, 1.0);
                let q = self.faces.id;
                gl.glUseProgram(q);
                mat(gl, q, c"u_vp", &view.view_proj);
                vec3(gl, q, c"u_origin", view.origin_rel);
                vec3(gl, q, c"u_light", st.light);
                let ambient = if it.lit { st.ambient } else { 1.0 };
                gl.glUniform1f(loc(gl, q, c"u_ambient"), ambient);
                gl.glUniform1i(loc(gl, q, c"u_shadow_on"), 0);
                let exact = if self.preview_exact { 1e-3 } else { 0.0 };
                gl.glUniform1f(loc(gl, q, c"u_px_fix"), exact);
                let drawing = view.paper.is_some();
                gl.glUniform1i(loc(gl, q, c"u_drawing"), drawing as GLint);
                gl.glUniform1i(loc(gl, q, c"u_paper_shade"), 0);
                gl.glUniform1i(loc(gl, q, c"u_patterns"), view.patterns as GLint);
                let ink = looks.pattern_ink;
                gl.glUniform4f(loc(gl, q, c"u_pattern_ink"), ink[0], ink[1], ink[2], ink[3]);
                gl.glActiveTexture(TEXTURE0 + 1);
                gl.glBindTexture(TEXTURE_2D, self.preview_looks_tex);
                gl.glUniform1i(loc(gl, q, c"u_looks"), 1);
                gl.glActiveTexture(TEXTURE0 + 2);
                gl.glBindTexture(TEXTURE_2D, self.preview_bond_tex);
                gl.glUniform1i(loc(gl, q, c"u_bond"), 2);
                gl.glActiveTexture(TEXTURE0);
                gl.glUniform1f(loc(gl, q, c"u_alpha"), 1.0);
                gl.glBindVertexArray(m.faces.vao);
                gl.glDrawArrays(TRIANGLES, 0, m.faces.count);
                gl.glDisable(POLYGON_OFFSET_FILL);
                // Kanten
                if m.edges.count > 0 {
                    gl.glDepthFunc(LEQUAL);
                    let q = self.edges.id;
                    gl.glUseProgram(q);
                    mat(gl, q, c"u_vp", &view.view_proj);
                    vec3(gl, q, c"u_origin", view.origin_rel);
                    let edges = if drawing {
                        &looks.drawing
                    } else {
                        &looks.model
                    };
                    gl.glUniform2f(loc(gl, q, c"u_viewport"), rw as f32, rh as f32);
                    let n = EDGE_KINDS as i32;
                    gl.glUniform1fv(loc(gl, q, c"u_edge_width"), n, edges.width.as_ptr());
                    let colors = edges.color.as_flattened();
                    gl.glUniform3fv(loc(gl, q, c"u_edge_color"), n, colors.as_ptr());
                    let dash = edges.dash.as_flattened();
                    gl.glUniform4fv(loc(gl, q, c"u_edge_dash"), 2 * n, dash.as_ptr());
                    gl.glUniform1f(loc(gl, q, c"u_near"), view.near);
                    gl.glUniform1f(loc(gl, q, c"u_alpha"), 1.0);
                    gl.glUniform4f(loc(gl, q, c"u_premix"), 0.0, 0.0, 0.0, 0.0);
                    gl.glBindVertexArray(m.edges.vao);
                    gl.glDrawArraysInstanced(TRIANGLES, 0, 6, m.edges.count);
                }
            }
            gl.glDisable(SCISSOR_TEST);
            gl.glBindFramebuffer(READ_FRAMEBUFFER, tg.ms_fbo);
            gl.glBindFramebuffer(DRAW_FRAMEBUFFER, tg.fbo);
            gl.glBlitFramebuffer(0, 0, w, h, 0, 0, w, h, COLOR_BUFFER_BIT, NEAREST as GLenum);
            gl.glBindFramebuffer(FRAMEBUFFER, 0);
            gl.glBindVertexArray(0);
        }
        Ok(())
    }

    /// Probe `--musterprobe` (B7): zeichnet `mesh` mit dem Flächen-Shader in
    /// ein `w` × `h`-Bild, ohne Licht, ohne Kanten und ohne Ausblenden in
    /// die Ferne (volle Nähe), und liest es als RGBA8 zurück, Zeile 0 oben.
    pub fn pattern_probe(
        &mut self,
        looks: &Looks,
        mesh: &MeshData,
        view: View,
        (w, h): (i32, i32),
    ) -> Result<Vec<u8>, String> {
        if let Some(e) = &self.pattern_error {
            return Err(format!("Muster im Shader nicht übersetzt: {e}"));
        }
        let keep = self.preview.take();
        self.set_preview_looks(looks);
        self.set_preview_mesh(0, mesh);
        self.preview = Some(Preview {
            at: [0, 0, w, h],
            before: usize::MAX,
            items: vec![PreviewItem {
                rect: [0, 0, w, h],
                clip: None,
                view,
                mesh: 0,
                lit: false,
                sky: false,
            }],
        });
        self.preview_exact = true;
        let r = self.render_preview();
        self.preview_exact = false;
        self.preview = keep;
        self.preview_dirty = true;
        r?;
        let Some(tg) = self.preview_target.as_ref() else {
            return Err("Vorschaupuffer fehlt.".into());
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        unsafe {
            let gl = &self.gl;
            gl.glBindFramebuffer(READ_FRAMEBUFFER, tg.fbo);
            gl.glReadBuffer(COLOR_ATTACHMENT0);
            gl.glPixelStorei(PACK_ALIGNMENT, 1);
            gl.glReadPixels(
                0,
                0,
                w,
                h,
                RGBA,
                UNSIGNED_BYTE,
                px.as_mut_ptr() as *mut c_void,
            );
            gl.glBindFramebuffer(READ_FRAMEBUFFER, 0);
        }
        let row = (w * 4) as usize;
        Ok(px.chunks(row).rev().flatten().copied().collect())
    }

    pub fn capture_scene(&mut self) {
        let Some(t) = &self.target else {
            return;
        };
        let (w, h, src) = (t.width, t.height, t.fbo);
        if self.snapshot.as_ref().is_some_and(|s| (s.w, s.h) != (w, h)) {
            self.release_snapshot();
        }
        let gl = &self.gl;
        unsafe {
            if self.snapshot.is_none() {
                let (mut fbo, mut tex) = (0, 0);
                gl.glGenTextures(1, &mut tex);
                gl.glBindTexture(TEXTURE_2D, tex);
                gl.glTexImage2D(
                    TEXTURE_2D,
                    0,
                    RGBA8 as GLint,
                    w,
                    h,
                    0,
                    RGBA,
                    UNSIGNED_BYTE,
                    std::ptr::null(),
                );
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, NEAREST);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, NEAREST);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_S, CLAMP_TO_EDGE);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_WRAP_T, CLAMP_TO_EDGE);
                gl.glGenFramebuffers(1, &mut fbo);
                gl.glBindFramebuffer(FRAMEBUFFER, fbo);
                gl.glFramebufferTexture2D(FRAMEBUFFER, COLOR_ATTACHMENT0, TEXTURE_2D, tex, 0);
                self.snapshot = Some(Snapshot {
                    fbo,
                    tex,
                    w,
                    h,
                    alpha: 0.0,
                    offset: 0.0,
                    offset_x: 0.0,
                    turn: None,
                });
            }
            if let Some(s) = &mut self.snapshot {
                // Mehrfachabtastung dabei auflösen
                gl.glBindFramebuffer(READ_FRAMEBUFFER, src);
                gl.glBindFramebuffer(DRAW_FRAMEBUFFER, s.fbo);
                gl.glBlitFramebuffer(0, 0, w, h, 0, 0, w, h, COLOR_BUFFER_BIT, NEAREST as GLenum);
                gl.glBindFramebuffer(FRAMEBUFFER, 0);
                (s.alpha, s.offset, s.offset_x, s.turn) = (1.0, 0.0, 0.0, None);
            }
        }
    }

    /// Deckkraft und Versatz (Pixel nach unten) des festgehaltenen Bildes.
    pub fn set_snapshot(&mut self, alpha: f32, offset_px: f32) {
        self.set_snapshot_xy(alpha, 0.0, offset_px);
    }

    /// Wie [`Renderer::set_snapshot`], dazu ein Versatz nach rechts (Pixel).
    pub fn set_snapshot_xy(&mut self, alpha: f32, dx: f32, dy: f32) {
        if let Some(s) = &mut self.snapshot {
            (s.alpha, s.offset, s.offset_x, s.turn) = (alpha.clamp(0.0, 1.0), dy, dx, None);
        }
    }

    /// Blatt wenden: das festgehaltene Bild deckt alles, waagerecht um
    /// `squash` (1 = ganz, 0 = Strich) zur Achse `axis_px` gestaucht und auf
    /// `shade` abgedunkelt; daneben der Grund aus der Bildecke.
    pub fn set_snapshot_turn(&mut self, squash: f32, axis_px: f32, shade: f32) {
        if let Some(s) = &mut self.snapshot {
            s.alpha = 1.0;
            s.turn = Some((squash.clamp(0.0, 1.0), axis_px, shade));
        }
    }

    /// Gibt das festgehaltene Bild frei.
    pub fn release_snapshot(&mut self) {
        if let Some(s) = self.snapshot.take() {
            unsafe {
                self.gl.glDeleteFramebuffers(1, &s.fbo);
                self.gl.glDeleteTextures(1, &s.tex);
            }
        }
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
        // Grenzen der Textur, nicht der gezeichneten Größe (Review 3x)
        if w <= 0 || h <= 0 || x < 0 || y < 0 || x + w > o.tw || y + h > o.th {
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
        if self.preview.is_some() && self.preview_dirty {
            self.preview_dirty = false;
            self.render_preview()?;
        }
        // Schatten in 3D von der Sonne, auf Papier vom Licht der Ansicht
        // (S7)
        self.shadow_paper = view.paper.is_some();
        unsafe { self.render_shadow() };
        self.ensure_target(w, h)?;
        // Mitte der Karte relativ zum Auge (Boden) und Umgebungsanteil
        let sky_c = self.shadow_karte.map_or([0.0; 3], |k| {
            let o = view.origin_rel;
            [
                (o[0] as f64 + k.mitte.x) as f32,
                (o[1] as f64 + k.mitte.y) as f32,
                (o[2] as f64 + k.mitte.z) as f32,
            ]
        });
        let face_c = self.shadow_karte.map_or([0.0; 3], |k| k.mitte.to_f32());
        let ambient = self.sun.map_or(self.style.ambient, |s| s.ambient);
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
            // Boden auf OK Gelände: das Auge steht um so viel höher darüber
            gl.glUniform1f(loc(gl, p, c"u_eye_z"), view.eye_z - self.terrain);
            gl.glUniform1f(loc(gl, p, c"u_horizon_px"), view.horizon_px);
            gl.glUniform1f(loc(gl, p, c"u_height"), h as f32);
            gl.glUniform1f(loc(gl, p, c"u_softness"), st.horizon_softness);
            vec3(gl, p, c"u_ground", st.ground);
            let n = st.sky.len().min(SKY_MAX);
            // Ohne neue Vec je Bild (Review H2)
            let mut pos = [0.0f32; SKY_MAX];
            let mut col = [0.0f32; 3 * SKY_MAX];
            for (i, s) in st.sky[..n].iter().enumerate() {
                pos[i] = s.0;
                col[3 * i..3 * i + 3].copy_from_slice(&s.1);
            }
            gl.glUniform1i(loc(gl, p, c"u_sky_n"), n as i32);
            gl.glUniform1fv(loc(gl, p, c"u_sky_pos"), n as i32, pos.as_ptr());
            gl.glUniform3fv(loc(gl, p, c"u_sky_col"), n as i32, col.as_ptr());
            gl.glUniform1i(loc(gl, p, c"u_overlay"), 0);
            gl.glUniform1f(loc(gl, p, c"u_opacity"), st.ground_opacity.clamp(0.0, 1.0));
            self.shadow_uniforms(p, sky_c);
            gl.glUniform1f(loc(gl, p, c"u_shade"), ambient);
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
            gl.glUniform1f(loc(gl, p, c"u_ambient"), ambient);
            self.shadow_uniforms(p, face_c);
            let drawing = view.paper.is_some();
            gl.glUniform1i(loc(gl, p, c"u_drawing"), drawing as GLint);
            // Schatten der Ansicht (S7) nur auf Papier mit Karte
            let ps = self
                .paper_shade
                .filter(|_| drawing && self.shadow_karte.is_some() && self.shadow_map.is_some());
            gl.glUniform1i(
                loc(gl, p, c"u_paper_shade"),
                ps.map_or(0, |s| if s.hatch { 2 } else { 1 }),
            );
            if let Some(s) = ps {
                let l = s.zum_licht;
                gl.glUniform3f(
                    loc(gl, p, c"u_paper_light"),
                    l[0] as f32,
                    l[1] as f32,
                    l[2] as f32,
                );
                let t = s.tone;
                gl.glUniform4f(loc(gl, p, c"u_shade_tone"), t[0], t[1], t[2], t[3]);
                gl.glUniform2f(loc(gl, p, c"u_hatch"), s.hatch_px[0], s.hatch_px[1]);
                let k = s.hatch_ink;
                gl.glUniform3f(loc(gl, p, c"u_hatch_ink"), k[0], k[1], k[2]);
            }
            gl.glUniform1i(loc(gl, p, c"u_patterns"), view.patterns as GLint);
            let below = if drawing { self.below.0 } else { 0 };
            gl.glUniform1i(loc(gl, p, c"u_below"), below);
            gl.glUniform1f(loc(gl, p, c"u_terrain"), self.terrain);
            if let Some(pc) = view.paper {
                gl.glUniform3f(loc(gl, p, c"u_paper_rgb"), pc[0], pc[1], pc[2]);
            }
            let ink = self.looks.pattern_ink;
            gl.glUniform4f(loc(gl, p, c"u_pattern_ink"), ink[0], ink[1], ink[2], ink[3]);
            gl.glActiveTexture(TEXTURE0 + 1);
            gl.glBindTexture(TEXTURE_2D, self.looks_tex);
            gl.glUniform1i(loc(gl, p, c"u_looks"), 1);
            gl.glActiveTexture(TEXTURE0 + 2);
            gl.glBindTexture(TEXTURE_2D, self.bond_tex);
            gl.glUniform1i(loc(gl, p, c"u_bond"), 2);
            gl.glActiveTexture(TEXTURE0);
            gl.glUniform1f(loc(gl, p, c"u_alpha"), 1.0);
            let ghost = self.ghost.filter(|g| g.0 < self.meshes.len());
            let opaque = |i: &usize| ghost.is_none_or(|g| g.0 != *i);
            for (_, m) in self.meshes.iter().enumerate().filter(|(i, _)| opaque(i)) {
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
            gl.glUniform1i(loc(gl, p, c"u_below"), below);
            gl.glUniform1f(loc(gl, p, c"u_terrain"), self.terrain);
            let d = self.below.1;
            gl.glUniform2f(loc(gl, p, c"u_below_dash"), d[0], d[1]);
            let n = EDGE_KINDS as i32;
            gl.glUniform1fv(loc(gl, p, c"u_edge_width"), n, edges.width.as_ptr());
            let colors = edges.color.as_flattened();
            gl.glUniform3fv(loc(gl, p, c"u_edge_color"), n, colors.as_ptr());
            let dash = edges.dash.as_flattened();
            gl.glUniform4fv(loc(gl, p, c"u_edge_dash"), 2 * n, dash.as_ptr());
            gl.glUniform1f(loc(gl, p, c"u_near"), view.near);
            gl.glUniform1f(loc(gl, p, c"u_alpha"), 1.0);
            gl.glUniform4f(loc(gl, p, c"u_premix"), 0.0, 0.0, 0.0, 0.0);
            for (_, m) in self
                .meshes
                .iter()
                .enumerate()
                .filter(|(i, m)| opaque(i) && m.edges.count > 0)
            {
                gl.glBindVertexArray(m.edges.vao);
                gl.glDrawArraysInstanced(TRIANGLES, 0, 6, m.edges.count);
            }

            // Blasses (Review 3a G4): erst nur Tiefe, damit je Bildpunkt nur
            // die vorderste blasse Fläche zählt, dann Farbe darüber gemischt,
            // ohne Tiefe zu schreiben; Kanten wie die Flächen
            if let Some((g, alpha)) = ghost.filter(|g| g.1 > 0.0) {
                let m = &self.meshes[g];
                let p = self.faces.id;
                gl.glUseProgram(p);
                gl.glEnable(POLYGON_OFFSET_FILL);
                gl.glPolygonOffset(1.0, 1.0);
                gl.glDepthFunc(LESS);
                gl.glColorMask(FALSE, FALSE, FALSE, FALSE);
                gl.glBindVertexArray(m.faces.vao);
                gl.glDrawArrays(TRIANGLES, 0, m.faces.count);
                gl.glColorMask(TRUE, TRUE, TRUE, TRUE);
                gl.glDepthFunc(LEQUAL);
                gl.glDepthMask(FALSE);
                gl.glEnable(BLEND);
                gl.glBlendFunc(SRC_ALPHA, ONE_MINUS_SRC_ALPHA);
                gl.glUniform1f(loc(gl, p, c"u_alpha"), alpha);
                gl.glDrawArrays(TRIANGLES, 0, m.faces.count);
                gl.glUniform1f(loc(gl, p, c"u_alpha"), 1.0);
                gl.glDisable(POLYGON_OFFSET_FILL);
                if m.edges.count > 0 {
                    let p = self.edges.id;
                    gl.glUseProgram(p);
                    gl.glUniform1f(loc(gl, p, c"u_alpha"), alpha);
                    // Zeichenmodus: vorgemischt und deckend (Prüfung p3-4, w)
                    if let Some(c) = view.paper {
                        gl.glUniform4f(loc(gl, p, c"u_premix"), c[0], c[1], c[2], 1.0);
                        gl.glDisable(BLEND);
                    }
                    gl.glBindVertexArray(m.edges.vao);
                    gl.glDrawArraysInstanced(TRIANGLES, 0, 6, m.edges.count);
                    gl.glUniform1f(loc(gl, p, c"u_alpha"), 1.0);
                    gl.glUniform4f(loc(gl, p, c"u_premix"), 0.0, 0.0, 0.0, 0.0);
                }
                gl.glDisable(BLEND);
                gl.glDepthMask(TRUE);
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

            // Tiefe wieder nur vom Deckenden: Hilfslinien, Auswahl und
            // Fang hinter Blassem bleiben sichtbar (Review 3h, Hinweis). Erst
            // nach dem Boden, sonst tönt er den Geist über leerem Boden (3l);
            // ohne Hilfslinien braucht niemand die Tiefe
            if ghost.is_some_and(|g| g.1 > 0.0) && self.helper_mesh.count > 0 {
                gl.glClear(DEPTH_BUFFER_BIT);
                let p = self.faces.id;
                gl.glUseProgram(p);
                gl.glEnable(POLYGON_OFFSET_FILL);
                gl.glPolygonOffset(1.0, 1.0);
                gl.glDepthFunc(LESS);
                gl.glColorMask(FALSE, FALSE, FALSE, FALSE);
                for (_, m) in self.meshes.iter().enumerate().filter(|(i, _)| opaque(i)) {
                    gl.glBindVertexArray(m.faces.vao);
                    gl.glDrawArrays(TRIANGLES, 0, m.faces.count);
                }
                gl.glColorMask(TRUE, TRUE, TRUE, TRUE);
                gl.glDisable(POLYGON_OFFSET_FILL);
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
            // Altes Bild beim Geschosswechsel: blendet aus und gleitet weg
            if let Some(sn) = self.snapshot.as_ref().filter(|s| s.alpha > 0.0) {
                gl.glViewport(0, 0, w, h);
                gl.glEnable(BLEND);
                gl.glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
                let p = self.snap_prog.id;
                gl.glUseProgram(p);
                gl.glActiveTexture(TEXTURE0);
                gl.glBindTexture(TEXTURE_2D, sn.tex);
                gl.glUniform1i(loc(gl, p, c"u_tex"), 0);
                gl.glUniform1f(loc(gl, p, c"u_alpha"), sn.alpha);
                gl.glUniform1f(loc(gl, p, c"u_offset"), sn.offset / sn.h.max(1) as f32);
                gl.glUniform1f(loc(gl, p, c"u_offset_x"), sn.offset_x / sn.w.max(1) as f32);
                let (turn, (squash, axis, shade)) = match sn.turn {
                    Some(t) => (1.0, t),
                    None => (0.0, (1.0, 0.0, 1.0)),
                };
                gl.glUniform1f(loc(gl, p, c"u_turn"), turn);
                gl.glUniform1f(loc(gl, p, c"u_squash"), squash);
                gl.glUniform1f(loc(gl, p, c"u_axis"), 2.0 * axis / sn.w.max(1) as f32 - 1.0);
                gl.glUniform1f(loc(gl, p, c"u_shade"), shade);
                gl.glBindVertexArray(self.empty_vao);
                gl.glDrawArrays(TRIANGLES, 0, 3);
                gl.glDisable(BLEND);
            }
            // Oberfläche (Titelleiste, Paneele) obenauf
            gl.glEnable(BLEND);
            gl.glBlendFunc(ONE, ONE_MINUS_SRC_ALPHA);
            let p = self.overlay.id;
            gl.glUseProgram(p);
            gl.glActiveTexture(TEXTURE0);
            gl.glUniform1i(loc(gl, p, c"u_tex"), 0);
            gl.glBindVertexArray(self.empty_vao);
            let u_alpha = loc(gl, p, c"u_alpha");
            let u_flip = loc(gl, p, c"u_flip");
            // Vorschau (Fenster „Muster“) unter ihrem Fensterbild, aus ihrer
            // Textur (Zeilen von unten)
            let preview = self
                .preview
                .as_ref()
                .zip(self.preview_target.as_ref())
                .map(|(pv, tg)| (pv.before, pv.at, (tg.tex, tg.prev_tex)));
            let fade = self.preview_fade;
            let draw_preview = |at: [i32; 4], (tex, prev): (GLuint, GLuint)| {
                gl.glViewport(at[0], win_h as i32 - at[1] - at[3], at[2], at[3]);
                gl.glUniform1f(u_flip, 1.0);
                for (t, a) in [(tex, 1.0), (prev, fade)] {
                    if a > 0.0 {
                        gl.glBindTexture(TEXTURE_2D, t);
                        gl.glUniform1f(u_alpha, a);
                        gl.glDrawArrays(TRIANGLES, 0, 3);
                    }
                }
                gl.glUniform1f(u_flip, 0.0);
            };
            for (i, o) in self.overlays.iter().enumerate() {
                if let Some((_, at, tex)) = preview.filter(|p| p.0 == i) {
                    draw_preview(at, tex);
                }
                if o.w <= 0 || o.h <= 0 || o.alpha <= 0.0 {
                    continue;
                }
                gl.glViewport(o.x, win_h as i32 - o.y - o.h, o.w, o.h);
                gl.glBindTexture(TEXTURE_2D, o.tex);
                // gestreckte Bilder (Animation) geglättet, sonst Pixel für Pixel
                let f = if (o.w, o.h) == (o.tw, o.th) || (o.tw, o.th) == (1, 1) {
                    NEAREST
                } else {
                    LINEAR
                };
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, f);
                gl.glTexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, f);
                gl.glUniform1f(u_alpha, o.alpha);
                gl.glDrawArrays(TRIANGLES, 0, 3);
            }
            if let Some((_, at, tex)) = preview.filter(|p| p.0 >= self.overlays.len()) {
                draw_preview(at, tex);
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
/// Flächen und Kanten eines Netzes in seine Puffer.
/// Tiefentextur `n` × `n` mit Vergleich „≤“ für `sampler2DShadow`, Texel
/// für Texel (wie [`schatten::Tiefenbild::sonne`]); `data` leer oder
/// `n` · `n` Werte (`u32::MAX` = frei).
unsafe fn depth_texture(gl: &Gl, n: i32, data: &[u32]) -> GLuint {
    let mut tex = 0;
    gl.glGenTextures(1, &mut tex);
    gl.glBindTexture(TEXTURE_2D, tex);
    let ptr = if data.is_empty() {
        std::ptr::null()
    } else {
        data.as_ptr() as *const c_void
    };
    gl.glPixelStorei(UNPACK_ALIGNMENT, 4);
    gl.glTexImage2D(
        TEXTURE_2D,
        0,
        DEPTH_COMPONENT24 as GLint,
        n,
        n,
        0,
        DEPTH_COMPONENT,
        UNSIGNED_INT,
        ptr,
    );
    for (p, v) in [
        (TEXTURE_MIN_FILTER, NEAREST),
        (TEXTURE_MAG_FILTER, NEAREST),
        (TEXTURE_WRAP_S, CLAMP_TO_EDGE),
        (TEXTURE_WRAP_T, CLAMP_TO_EDGE),
        (TEXTURE_COMPARE_MODE, COMPARE_REF_TO_TEXTURE),
        (TEXTURE_COMPARE_FUNC, LEQUAL as GLint),
    ] {
        gl.glTexParameteri(TEXTURE_2D, p, v);
    }
    tex
}

/// Hüllquader der Flächen eines Netzes (Modell, mm).
fn huelle(faces: &[[f32; 9]]) -> Option<(sk_math::Vec3, sk_math::Vec3)> {
    let mut it = faces
        .iter()
        .map(|v| sk_math::vec3(v[0] as f64, v[1] as f64, v[2] as f64));
    let p = it.next()?;
    Some(it.fold((p, p), |(a, b), p| {
        (
            sk_math::vec3(a.x.min(p.x), a.y.min(p.y), a.z.min(p.z)),
            sk_math::vec3(b.x.max(p.x), b.y.max(p.y), b.z.max(p.z)),
        )
    }))
}

unsafe fn upload_mesh(gl: &Gl, gm: &mut GpuMesh, mesh: &MeshData) {
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

/// Aussehens-Tabelle als Gleitkomma-Textur (Schlüssel × [`LOOK_ROWS`]).
unsafe fn upload_looks(gl: &Gl, tex: &mut GLuint, looks: &Looks) {
    let keys = looks.keys.max(1);
    let mut texels = looks.texels.clone();
    texels.resize(keys * LOOK_ROWS, [0.0; 4]);
    if *tex == 0 {
        gl.glGenTextures(1, tex);
        gl.glBindTexture(TEXTURE_2D, *tex);
        for (p, v) in [
            (TEXTURE_MIN_FILTER, NEAREST),
            (TEXTURE_MAG_FILTER, NEAREST),
            (TEXTURE_WRAP_S, CLAMP_TO_EDGE),
            (TEXTURE_WRAP_T, CLAMP_TO_EDGE),
        ] {
            gl.glTexParameteri(TEXTURE_2D, p, v);
        }
    }
    gl.glBindTexture(TEXTURE_2D, *tex);
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

/// Verbandstabellen als Ganzzahl-Textur (R8UI, 128 breit); ohne wilden
/// Verband eine leere Tabelle, damit die Textur vollständig ist.
unsafe fn upload_bond(gl: &Gl, tex: &mut GLuint, bond: &[u8]) {
    let mut data = bond.to_vec();
    let n = data.len().div_ceil(BOND_TABLE_BYTES).max(1);
    data.resize(n * BOND_TABLE_BYTES, 0);
    if *tex == 0 {
        gl.glGenTextures(1, tex);
        gl.glBindTexture(TEXTURE_2D, *tex);
        for (p, v) in [
            (TEXTURE_MIN_FILTER, NEAREST),
            (TEXTURE_MAG_FILTER, NEAREST),
            (TEXTURE_WRAP_S, CLAMP_TO_EDGE),
            (TEXTURE_WRAP_T, CLAMP_TO_EDGE),
        ] {
            gl.glTexParameteri(TEXTURE_2D, p, v);
        }
    }
    gl.glBindTexture(TEXTURE_2D, *tex);
    gl.glPixelStorei(UNPACK_ALIGNMENT, 1);
    gl.glTexImage2D(
        TEXTURE_2D,
        0,
        R8UI as GLint,
        128,
        (128 * n) as i32,
        0,
        RED_INTEGER,
        UNSIGNED_BYTE,
        data.as_ptr() as *const c_void,
    );
    gl.glPixelStorei(UNPACK_ALIGNMENT, 4);
    gl.glBindTexture(TEXTURE_2D, 0);
}

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

thread_local! {
    /// Uniform-Positionen je Programm und Name (Review H1): einmal beim
    /// Treiber erfragt statt in jedem Bild. Schlüssel ist die Adresse des
    /// festen Namens; ein neues Programm mit derselben Nummer leert seine
    /// Einträge ([`program`]). Gilt für einen GL-Kontext je Thread, wie
    /// heute (ein `Renderer`).
    static LOCS: std::cell::RefCell<std::collections::HashMap<(GLuint, usize), GLint>> =
        Default::default();
}

unsafe fn loc(gl: &Gl, p: GLuint, name: &'static std::ffi::CStr) -> GLint {
    let key = (p, name.as_ptr() as usize);
    if let Some(l) = LOCS.with(|c| c.borrow().get(&key).copied()) {
        return l;
    }
    let l = gl.glGetUniformLocation(p, name.as_ptr() as *const GLchar);
    LOCS.with(|c| c.borrow_mut().insert(key, l));
    l
}

unsafe fn mat(gl: &Gl, p: GLuint, name: &'static std::ffi::CStr, m: &[f32; 16]) {
    gl.glUniformMatrix4fv(loc(gl, p, name), 1, FALSE, m.as_ptr());
}

unsafe fn vec3(gl: &Gl, p: GLuint, name: &'static std::ffi::CStr, v: [f32; 3]) {
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
    LOCS.with(|c| c.borrow_mut().retain(|k, _| k.0 != p));
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

    /// Richtung zum Licht (Modellkoordinaten, Länge 1), etwa zur Sonne
    /// (Sonnenstand S4).
    pub fn set_light(&mut self, l: [f32; 3]) {
        self.style.light = l;
    }

    /// Deckkraft des Bodens (0: Gelände ausgeblendet, Paket 3).
    pub fn set_ground_opacity(&mut self, v: f32) {
        self.style.ground_opacity = v;
    }

    /// Höhe von OK Gelände im Modell (mm, Gelände Thema 1): Boden in 3D
    /// und Grenze für „unter dem Gelände“ in den Ansichten.
    pub fn set_terrain(&mut self, z: f32) {
        self.terrain = z;
    }

    /// Ansichten auf Papier (S11): Teile unter dem Gelände (z < 0) wie
    /// alles (`None`), ausgeblendet (`Some(None)`) oder gestrichelt mit
    /// Strich und Lücke in Bildpunkten.
    pub fn set_below_ground(&mut self, m: Option<Option<[f32; 2]>>) {
        self.below = match m {
            None => (0, [0.0; 2]),
            Some(None) => (1, [0.0; 2]),
            Some(Some(d)) => (2, d),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feine_kante_im_shader() {
        assert!(EDGE_VS.contains(&format!("const int FINE = {EDGE_FINE};")));
    }

    /// Kopf jeder Funktion in `glsl` (`typ name(…) {`), nach Name.
    fn heads(glsl: &str) -> Vec<(&str, &str)> {
        glsl.lines()
            .filter(|l| !l.starts_with(' ') && l.trim_end().ends_with('{') && l.contains('('))
            .filter_map(|l| {
                let name = l.split('(').next()?.split_whitespace().last()?;
                Some((name, l.trim_end()))
            })
            .collect()
    }

    /// Review 3q: Der Ersatz ohne Muster deckt jede Musterfunktion, die der
    /// Flächen-Shader ruft, mit derselben Signatur.
    #[test]
    fn ersatz_ohne_muster_deckt_den_flaechen_shader() {
        let fallback = heads(PATTERN_FALLBACK_GLSL);
        for (name, head) in heads(PATTERN_GLSL) {
            if FACE_FS.contains(&format!("{name}(")) {
                assert!(fallback.contains(&(name, head)), "Ersatz fehlt: {head}");
            }
        }
    }
}

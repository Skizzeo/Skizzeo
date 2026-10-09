//! Skizzeo – 3D-Gebäudemodellierer.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(test)]
mod abnahme;
#[cfg(test)]
mod abnahme_einstellungen;
#[cfg(test)]
mod abnahme_pd;
#[cfg(test)]
mod abnahme_s1;
#[cfg(test)]
mod abnahme_s2;
#[cfg(test)]
mod abnahme_s4;
mod ansicht_schatten;
mod attr_pick;
mod autosave;
mod ava_view;
mod backup_card;
mod camera;
mod cards;
mod catalog;
mod catalog_view;
mod cli;
mod delete;
mod document;
mod draw_table;
mod flush_pick;
mod frame_time;
#[cfg(all(test, target_os = "linux"))]
mod gpu_probe;
mod help;
mod hints;
mod kosten_view;
mod link_view;
mod lohn_blatt;
mod lv_blatt;
mod material_view;
mod measure_input;
mod meldung;
mod menu;
mod musterprobe;
mod nav;
mod nordpfeil;
mod pattern_view;
#[cfg(test)]
mod perf;
mod picking;
mod prefs;
mod preis_blatt;
mod projektdaten;
mod quantity;
mod scene;
mod schedule_view;
mod section;
mod selection;
mod settings;
mod sonne_view;
mod terrace_label;
mod tree_panel;
mod type_look;
mod type_menu;
mod ui;
mod umfang_view;
mod verwaltung;
mod visible;
mod wahl_blatt;
mod wall_edit;
mod wall_tool;
mod wheel;
mod wheel_view;
mod window_kit;
mod windows;

use camera::Camera;
use document::Document;
use draw_table::DrawTable;
use menu::Command;
use nav::Navigation;
use scene::Scene;
use section::Sections;
use selection::Selection;
use sk_math::{vec3, Vec3};
use sk_model::Category;
use sk_paint::Rgba;
use sk_platform::{CaptionArea, Config, Event, Key, MouseButton, Surface, WindowCommand};
use sk_render::{gl::Gl, Renderer, Style};
use sk_ui::{
    logo,
    theme::{Environment, Theme},
    titlebar::{Button, TitleBar},
};
use std::f64::consts::{FRAC_PI_2, PI};
use std::time::Instant;
use ui::{Draft, Field, FieldRow, Grip, Id, LevelEvent, Panel, Ui, ViewKind};
use wall_edit::WallEdit;
use wall_tool::WallTool;

fn main() {
    let screenshot = std::env::args().skip_while(|a| a != "--screenshot").nth(1);
    let config = Config {
        title: "Skizzeo".into(),
        width: 1280,
        height: 800,
        icon: Some(|size| logo::app_icon(size as usize).to_rgba8()),
    };
    if let Err(e) = sk_platform::run(config, move |s| app(s, screenshot)) {
        sk_platform::show_error(&e);
        std::process::exit(1);
    }
}

/// Schreibt Farbschema und „Zuletzt geöffnet“ beim Beenden, falls sie sich
/// geändert haben.
fn save_settings(
    settings: &mut settings::Settings,
    theme: &Theme,
    recent: &menu::Recent,
    (grouping, blatt): (schedule_view::Grouping, cards::Blatt),
    panel: String,
    surface: &Surface,
) {
    if settings.path.is_some() {
        settings.recent = recent.clone();
        // Baumpanel: Karte, zugeklappt, Grenze; gesehene Hinweise (Paket 4)
        settings.panel = panel;
        // Lage des Mengenfensters (F2) und Gliederung der Liste (Paket 1b)
        settings.windows = windows::write_settings_blatt(&surface.layout(), grouping, blatt);
    }
    if let Err(e) = settings.save_if_changed(theme) {
        eprintln!("{e}");
    }
}

/// Kamera beim Start und für ein neues Projekt.
fn start_camera() -> Camera {
    Camera::looking_at(vec3(-6200.0, -8600.0, 3700.0), Vec3::ZERO, 45.0)
}

fn rgb(c: Rgba) -> [f32; 3] {
    let [r, g, b, _] = c.to_f32();
    [r, g, b]
}

/// Renderer-Stil (Himmel, Boden, Licht). Baustoffe und Kanten kommen aus der
/// Zeichentabelle ([`DrawTable::looks`]).
fn style(env: &Environment) -> Style {
    let l = vec3(0.32, -0.48, 0.82).normalized().to_f32();
    Style {
        sky: env.sky.iter().map(|&(d, c)| (d, rgb(c))).collect(),
        ground: rgb(env.ground),
        horizon_softness: env.horizon_softness,
        ground_opacity: env.ground_opacity,
        light: l,
        ambient: 0.84,
    }
}

/// Welche Geometrie eine Ansicht zeigt: Grundriss und Schnitt schneiden,
/// 3D und die vier Ansichten teilen sich dasselbe Netz.
fn geometry(v: ViewKind) -> u8 {
    match v {
        ViewKind::Plan => 1,
        ViewKind::Section => 2,
        _ => 0,
    }
}

/// Oberflächenbilder in der Zeichenreihenfolge.
/// Renderer-Ablagen für Netze: ruhendes Modell, Vorschau des Wandwerkzeugs,
/// gezogener Wandzug, blasses Netz (Isolieren und Übergänge, Paket 3).
const MESH_MODEL: usize = 0;
const MESH_PREVIEW: usize = 1;
const MESH_LIVE: usize = 2;
const MESH_GHOST: usize = 3;
/// Würfel 10 m ohne Gebäude bei eingeschaltetem Sonnenstand (nur Anzeige).
const MESH_WUERFEL: usize = 4;
/// Wofür der Griff an der Schattenspitze gilt: Sonne, Lage, Stand der
/// Ecken, mit Gebäude (sonst am Würfel).
type GriffSchluessel = (sk_model::Sun, sk_model::Location, u64, bool);
/// Tooltip an Leiste und Sonne, wenn der Treiber keine Schatten kann (S5).
const SCHATTEN_FEHLT: &str = "Schatten auf diesem Rechner nicht verfügbar.";
/// Bildabstand für Animationen im Mengenfenster (das Hauptfenster läuft mit vsync).
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

// Oberflächenbilder in Zeichenreihenfolge: Endsymbole unter den Paneelen,
// das Abdunkeln hinter dem Dialog über „Ansichten“, „Eigenschaften“ und
// „Werkzeuge“, unter „Geschosse“ (E16), die Titelleiste ganz oben.
/// Endsymbole der Schnittlinien A und B im Grundriss (je zwei Plätze).
const OVERLAY_MARKS: usize = 0;
const MARKS: usize = 2 * section::CUTS;
/// Nordpfeil (Sonnenstand S2, Gestalt A), unter den Paneelen.
const OVERLAY_NORD: usize = OVERLAY_MARKS + MARKS;
/// Leiste des Sonnenstands-Systems in 3D (Datum, Uhrzeit, Schnellwahl;
/// Sonnenstand S4), unter den Paneelen.
const OVERLAY_SONNE: usize = OVERLAY_NORD + 1;
/// Zahnrad und Feld „Schatten“ der Ansichten (S7).
const OVERLAY_ANSICHT: usize = OVERLAY_SONNE + 1;
/// Name und Fläche der Dachterrassen im Grundriss (wie eine Raumangabe).
const OVERLAY_ROOMS: usize = OVERLAY_ANSICHT + 1;
const ROOMS: usize = 4;
/// Kettensymbole an gestapelten Wänden (OG Phase 2), unter den Paneelen.
const OVERLAY_CHIPS: usize = OVERLAY_ROOMS + ROOMS;
const OVERLAY_VIEWS: usize = OVERLAY_CHIPS + link_view::SLOTS;
/// Paneel „Eigenschaften“.
const OVERLAY_PROPS: usize = OVERLAY_VIEWS + 1;
/// Baumpanel (Paket 4) zwischen „Ansichten“ und „Eigenschaften“.
const OVERLAY_TREE: usize = OVERLAY_VIEWS + 2;
const OVERLAY_TOOLS: usize = OVERLAY_VIEWS + 3;
const OVERLAY_SCRIM: usize = OVERLAY_VIEWS + 4;
/// Paneel „Geschosse“.
const OVERLAY_LEVELS: usize = OVERLAY_VIEWS + 5;
/// Dialog „Gebäude erstellen“.
const OVERLAY_DIALOG: usize = OVERLAY_VIEWS + 6;
const OVERLAY_TITLE: usize = OVERLAY_VIEWS + 7;
/// Hinweis in der Statuszeile (F-17), unten in der Mitte.
const OVERLAY_NOTICE: usize = OVERLAY_VIEWS + 8;
/// Dateimenü (E17), darüber die Nachfrage „Änderungen speichern?“ mit
/// Abdunkeln.
const OVERLAY_MENU: usize = OVERLAY_VIEWS + 9;
const OVERLAY_SAVE_SCRIM: usize = OVERLAY_VIEWS + 10;
const OVERLAY_SAVE: usize = OVERLAY_VIEWS + 11;
/// Einstellungsfenster (E5) und sein Aufklapper (Auswahlliste, Farbwähler,
/// Nachfrage).
const OVERLAY_PREFS: usize = OVERLAY_VIEWS + 12;
const OVERLAY_PREFS_POPUP: usize = OVERLAY_VIEWS + 13;
/// Typ-Liste am Chip (K3).
const OVERLAY_TYPE_MENU: usize = OVERLAY_VIEWS + 14;
/// Geschossbogen im Grundriss (E18): Bogen, Aufleuchten, Schilder und
/// Hinweis an der Spitze.
const OVERLAY_WHEEL: usize = OVERLAY_VIEWS + 15;
/// Löschen (V?-9): Hinweis am Bauteil, Rückfrage „Gebäude löschen“ und
/// Kontextmenü am Bauteil.
const OVERLAY_HINT: usize = OVERLAY_WHEEL + wheel_view::SLOTS;
const OVERLAY_CONFIRM: usize = OVERLAY_HINT + 1;
const OVERLAY_CONTEXT: usize = OVERLAY_HINT + 2;
/// Sicherungen (F-13): Abdunkeln und Startkarte bzw. Liste, über allem
/// außer dem Hinweis an der Maus.
const OVERLAY_CARD_SCRIM: usize = OVERLAY_HINT + 3;
const OVERLAY_CARD: usize = OVERLAY_HINT + 4;
/// Hilfekarte (Paket 9) über allen Fenstern: das vorige Bild beim
/// Überblenden und das jetzige.
const OVERLAY_HELP_OLD: usize = OVERLAY_HINT + 5;
const OVERLAY_HELP: usize = OVERLAY_HINT + 6;
/// Hinweis an der Maus, über allem.
const OVERLAY_TIP: usize = OVERLAY_HINT + 7;
/// Maßzahl am Weg beim „Bündig setzen“ (E20).
const OVERLAY_PICK: usize = OVERLAY_HINT + 8;
/// Maßzahl bzw. Maßeingabe am Gummiband und beim Ziehen (Paket 8).
const OVERLAY_INPUT: usize = OVERLAY_HINT + 9;
/// Live-Pille, die beim ersten Tippen in die Eingabe überblendet.
const OVERLAY_INPUT_OLD: usize = OVERLAY_HINT + 10;
/// Maske „Projektdaten“ (Paket PD-2) mit Abdunkeln, über den Paneelen.
const OVERLAY_PD_SCRIM: usize = OVERLAY_HINT + 11;
const OVERLAY_PD: usize = OVERLAY_HINT + 12;

/// So lange steht die Pille nach dem Loslassen (Nachkorrektur, Paket 8b).
const POST_PILL: std::time::Duration = std::time::Duration::from_millis(1500);

/// Versatz am Band beim Ziehen (OG Phase 2): „Versatz +0,30“, unter 2 cm
/// „bündig“.
fn offset_label(o: f64) -> String {
    if o == 0.0 {
        return "bündig".into();
    }
    let sign = if o > 0.0 { '+' } else { '\u{2212}' };
    format!("Versatz {sign}{}", schedule_view::de(o.abs() / 1000.0, 2))
}

/// Hinweis nach „wieder koppeln“ mit Versatz (OG-16).
fn relink_lines(o: f64) -> Vec<String> {
    let what = if o > 0.0 { "Vorsprung" } else { "Rücksprung" };
    vec![
        "Wieder gekoppelt.".into(),
        format!(
            "Der {what} von {} m bleibt, EG und OG gehen gemeinsam.",
            schedule_view::de(o.abs() / 1000.0, 2)
        ),
    ]
}

/// Blickrichtung (yaw, pitch) der Parallelansichten.
fn view_direction(v: ViewKind) -> (f64, f64) {
    match v {
        ViewKind::Plan => (FRAC_PI_2, -FRAC_PI_2),
        ViewKind::Front | ViewKind::Section => (FRAC_PI_2, 0.0),
        ViewKind::Back => (-FRAC_PI_2, 0.0),
        ViewKind::Left => (0.0, 0.0),
        ViewKind::Right | ViewKind::Persp => (PI, 0.0),
    }
}

/// Parallelkamera, die das ganze Modell (oder ein leeres Baufeld) zeigt.
fn fit_parallel(v: ViewKind, bounds: Option<(Vec3, Vec3)>, w: f64, h: f64) -> Camera {
    fit_parallel_in(v, bounds, w, w, h, w * 0.5)
}

/// Wie [`fit_parallel`], aber das Modell passt in die Breite `w` und steht
/// waagerecht bei `x` (Pixel der Ansicht mit der Breite `vw`), etwa links
/// vom Platz, den der Geschossbogen im Grundriss braucht.
fn fit_parallel_in(
    v: ViewKind,
    bounds: Option<(Vec3, Vec3)>,
    w: f64,
    vw: f64,
    h: f64,
    x: f64,
) -> Camera {
    fit_parallel_dir(view_direction(v), bounds, w, vw, h, x)
}

/// Wie [`fit_parallel_in`] mit freier Blickrichtung (yaw, pitch).
fn fit_parallel_dir(
    (yaw, pitch): (f64, f64),
    bounds: Option<(Vec3, Vec3)>,
    w: f64,
    vw: f64,
    h: f64,
    x: f64,
) -> Camera {
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 3500.0)));
    let center = (lo + hi) * 0.5;
    let probe = Camera::parallel(center, yaw, pitch, 1.0);
    let (r, u) = (probe.right(), probe.up());
    let (mut wr, mut wu) = (0.0f64, 0.0f64);
    for i in 0..8 {
        let p = vec3(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        ) - center;
        wr = wr.max(p.dot(r).abs());
        wu = wu.max(p.dot(u).abs());
    }
    // Nur die Breite zwischen den Paneelen nutzen
    let aspect = (w / h.max(1.0)).max(0.1);
    let half = (wu.max(wr / aspect) * 1.2).max(2000.0);
    // Mitte des Bildes um den Versatz der Ziel-Mitte verschieben
    let shift = (vw * 0.5 - x) * 2.0 * half / h.max(1.0);
    Camera::parallel(center + r * shift, yaw, pitch, half)
}

/// Grundriss eingepasst zwischen „Werkzeuge“ und dem Platz, den der
/// Geschossbogen neben „Eigenschaften“ braucht (e06cada), für ein
/// Hauptfenster `w` × `h` mit der Titelleiste `top` (Pixel): Der Bogen steht
/// nie im Plan, auch nach Andocken des Mengenfensters oder Größeziehen.
pub(crate) fn plan_camera(
    ui: &Ui,
    wheel: &wheel::Wheel,
    bounds: Option<(Vec3, Vec3)>,
    w: u32,
    h: u32,
    top: u32,
) -> Camera {
    let (vw, vh) = (w as f64, h.saturating_sub(top) as f64);
    let tools = ui.rect(Panel::Tools, w, top);
    let s = ui.dpi() as f64;
    let x0 = (tools.x + tools.w) as f64 + 16.0 * s;
    let x1 = wheel.left_beside_props(ui, w, h) as f64 - 16.0 * s;
    if x1 - x0 >= vw * 0.3 {
        fit_parallel_in(ViewKind::Plan, bounds, x1 - x0, vw, vh, (x0 + x1) * 0.5)
    } else {
        let free_w = (vw - 2.0 * (tools.x + tools.w) as f64).max(vw * 0.3);
        fit_parallel(ViewKind::Plan, bounds, free_w, vh)
    }
}

/// Schnitt eingepasst wie der Grundriss (links vom Schnittrad), Blick in
/// die Richtung `dir` des aktiven Schnitts. Alle Schnitte haben einen
/// Maßstab (der breiteste bestimmt ihn) und ±0,00 auf derselben Bildhöhe
/// (E19 §8): Eingepasst wird ein im Grundriss quadratischer Rahmen.
pub(crate) fn section_camera(
    ui: &Ui,
    wheel: &wheel::Wheel,
    bounds: Option<(Vec3, Vec3)>,
    (w, h, top): (u32, u32, u32),
    dir: Vec3,
) -> Camera {
    let bounds = bounds.map(|(lo, hi)| {
        let c = (lo + hi) * 0.5;
        let r = 0.5 * (hi.x - lo.x).max(hi.y - lo.y);
        (vec3(c.x - r, c.y - r, lo.z), vec3(c.x + r, c.y + r, hi.z))
    });
    let (vw, vh) = (w as f64, h.saturating_sub(top) as f64);
    let tools = ui.rect(Panel::Tools, w, top);
    let s = ui.dpi() as f64;
    let yaw = dir.y.atan2(dir.x);
    let x0 = (tools.x + tools.w) as f64 + 16.0 * s;
    let x1 = wheel.left_beside_props(ui, w, h) as f64 - 16.0 * s;
    if x1 - x0 >= vw * 0.3 {
        fit_parallel_dir((yaw, 0.0), bounds, x1 - x0, vw, vh, (x0 + x1) * 0.5)
    } else {
        let free_w = (vw - 2.0 * (tools.x + tools.w) as f64).max(vw * 0.3);
        fit_parallel_dir((yaw, 0.0), bounds, free_w, vw, vh, vw * 0.5)
    }
}

/// Geländelinie in Schnitt und Ansichten (kräftig, über das Gebäude hinaus).
fn ground_line(
    v: ViewKind,
    bounds: Option<(Vec3, Vec3)>,
    scale: f32,
    table: &DrawTable,
) -> Vec<sk_render::Helper> {
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 0.0)));
    let m = 3000.0;
    let (a, b) = match v {
        ViewKind::Left | ViewKind::Right => (vec3(lo.x, lo.y - m, 0.0), vec3(lo.x, hi.y + m, 0.0)),
        _ => (vec3(lo.x - m, lo.y, 0.0), vec3(hi.x + m, lo.y, 0.0)),
    };
    vec![sk_render::Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: table.ground.1,
        width: table.ground.0 * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    }]
}

/// Hilfslinie der gezogenen Ebene in Höhe `z`: in 3D ein Rechteck um den
/// Grundriss des Modells (0,5 m Überstand), in Schnitt und Ansichten eine
/// waagerechte Linie wie die Geländelinie, im Grundriss nichts.
fn level_guide(
    v: ViewKind,
    bounds: Option<(Vec3, Vec3)>,
    z: f64,
    scale: f32,
    theme: &Theme,
) -> Vec<sk_render::Helper> {
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 0.0)));
    let line = |a: Vec3, b: Vec3| sk_render::Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: theme.interact.drag,
        width: theme.size.level_guide * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: true,
    };
    match v {
        ViewKind::Plan => Vec::new(),
        ViewKind::Persp => {
            let m = 500.0;
            let (x0, y0, x1, y1) = (lo.x - m, lo.y - m, hi.x + m, hi.y + m);
            let c = [
                vec3(x0, y0, z),
                vec3(x1, y0, z),
                vec3(x1, y1, z),
                vec3(x0, y1, z),
            ];
            (0..4).map(|i| line(c[i], c[(i + 1) % 4])).collect()
        }
        ViewKind::Left | ViewKind::Right => {
            let m = 3000.0;
            vec![line(vec3(lo.x, lo.y - m, z), vec3(lo.x, hi.y + m, z))]
        }
        _ => {
            let m = 3000.0;
            vec![line(vec3(lo.x - m, lo.y, z), vec3(hi.x + m, lo.y, z))]
        }
    }
}

/// 3D-Kamera schräg von vorne links, die das ganze Modell zeigt.
fn fit_perspective(lo: Vec3, hi: Vec3) -> Camera {
    let center = (lo + hi) * 0.5;
    let radius = ((hi - lo).length() * 0.5).max(1000.0);
    let fov = 45f64;
    let dist = radius / (fov.to_radians() * 0.5).sin() * 1.15;
    let dir = vec3(-0.55, -0.75, 0.42).normalized();
    Camera::looking_at(center + dir * dist, center, fov)
}

/// Weiche Kamerafahrt (Doppelklick in der Mengenliste).
struct Fly {
    from: Camera,
    to: Camera,
    start: Instant,
    ms: f64,
}

impl Fly {
    /// Kamera zum Zeitpunkt `now`; `None`, wenn die Fahrt vorbei ist.
    fn at(&self, now: Instant) -> Option<Camera> {
        let k = now.duration_since(self.start).as_secs_f64() * 1000.0 / self.ms.max(1.0);
        if k >= 1.0 {
            return None;
        }
        // Weich an- und auslaufen
        let e = k * k * (3.0 - 2.0 * k);
        let mut c = self.to.clone();
        c.eye = self.from.eye + (self.to.eye - self.from.eye) * e;
        c.focus = self.from.focus + (self.to.focus - self.from.focus) * e;
        if let (Some(a), Some(b)) = (self.from.ortho, self.to.ortho) {
            // Maßstab logarithmisch, damit das Heranholen gleichmäßig wirkt
            c.ortho = Some((a.ln() + (b.ln() - a.ln()) * e).exp());
        }
        Some(c)
    }
}

/// Kamera, die den Quader `lo`–`hi` aus der jetzigen Richtung zeigt.
fn zoom_camera(cam: &Camera, lo: Vec3, hi: Vec3, w: f64, h: f64) -> Camera {
    let center = (lo + hi) * 0.5;
    let (r, u, f) = (cam.right(), cam.up(), cam.forward());
    let (mut wr, mut wu, mut wf) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..8 {
        let p = vec3(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        ) - center;
        wr = wr.max(p.dot(r).abs());
        wu = wu.max(p.dot(u).abs());
        wf = wf.max(p.dot(f).abs());
    }
    let aspect = (w / h.max(1.0)).max(0.1);
    // Halbe Bildhöhe, die das Bauteil mit Rand braucht (mindestens 0,8 m)
    let half = (wu.max(wr / aspect) * 1.8).max(800.0);
    match cam.ortho {
        Some(_) => {
            let mut c = Camera::parallel(center, cam.yaw, cam.pitch, half.max(1500.0));
            c.fov_y = cam.fov_y;
            c
        }
        None => {
            // In 3D mehr Umgebung lassen, sonst verdecken nahe Wände das Bauteil
            let dist = half * 1.4 / (cam.fov_y * 0.5).tan() + wf;
            let mut c = cam.clone();
            c.eye = center - f * dist;
            c.focus = dist;
            c
        }
    }
}

/// Stand eines Endsymbols: links, hervorgehoben, Skalierung, Stände von
/// Farbschema und Zeichentabelle.
type MarkKey = (usize, bool, bool, bool, u32, (u64, u64));
type RoomKey = (String, u32, u64);

struct App {
    renderer: Renderer,
    scene: Scene,
    /// Datei des Projekts und gespeicherter Stand.
    doc: Document,
    title: TitleBar,
    ui: Ui,
    cam: Camera,
    /// Zuletzt eingepasste Kamera (bei neuer Fenstergröße neu einpassen,
    /// solange sie unverändert ist).
    fitted: Option<Camera>,
    /// Letzte 3D-Kamera, um aus den Parallelansichten zurückzukehren.
    cam3d: Camera,
    /// Die gemerkte 3D-Kamera stammt aus einer Zeit ohne Modell.
    cam3d_empty: bool,
    nav: Navigation,
    tool: WallTool,
    edit: WallEdit,
    /// Schnittlinien A und B (im Grundriss verschiebbar, Pfeil spiegelt)
    /// für die Ansicht „Schnitt“.
    sect: Sections,
    /// Nordpfeil: Aufziehen, Drehen, Verschieben (Sonnenstand S2).
    nord: nordpfeil::Nordpfeil,
    /// Sonnenstands-System: Sonne ziehen, Leiste (Sonnenstand S4).
    sonne: sonne_view::Sonnensystem,
    /// Gewähltes Bauteil.
    sel: Selection,
    /// Stand, für den das Paneel „Eigenschaften“ zuletzt gefüllt wurde
    /// (Bauteil, Modellrevision).
    props_key: Option<(sk_model::ElementId, u64)>,
    /// Stand (Revision, aktives Geschoss) der Fangkanten des Hintergrunds.
    snaps_key: Option<(u64, sk_model::StoreyId)>,
    w: u32,
    h: u32,
    overlay_dirty: bool,
    /// Nur Fensterbreite geändert: Titelleiste neu zeichnen, Paneele nur verschieben.
    layout_dirty: bool,
    /// Inhalt des Paneels „Eigenschaften“ hat sich geändert.
    props_dirty: bool,
    /// Nur diese Knöpfe der Paneele bzw. der Titelleiste neu zeichnen (Hover, Drücken).
    dirty_buttons: Vec<Id>,
    dirty_title: Vec<Button>,
    /// Farbschema der Oberfläche.
    theme: Theme,
    /// Zuletzt gezeichnete Titelleiste, für das Neuzeichnen einzelner Knöpfe.
    title_img: Option<sk_paint::Canvas>,
    redraw: bool,
    /// Modellnetz muss vor dem nächsten Bild neu erzeugt werden.
    mesh_dirty: bool,
    /// Live-Netz (gezogener Wandzug) muss neu erzeugt werden.
    live_dirty: bool,
    /// Wandzüge, die gerade im Live-Netz statt im ruhenden Netz liegen (der
    /// gezogene und die, die an ihm hängen).
    live_runs: Vec<sk_model::RunId>,
    /// Die Vorschau-Ablage enthält ein Netz.
    preview_shown: bool,
    /// Stand der Zeichentabelle, aus dem der Renderer-Stil stammt.
    /// Stand der hochgeladenen Aussehens-Tabelle: Attribute, Farbschema, Skalierung.
    looks_key: Option<(u64, u64, u32, u64)>,
    /// Startwerte des wilden Verbands, deren Tabelle noch im Hintergrund
    /// rechnet (Flächen zeigen die Mischfarbe), und die gerade
    /// einblendenden (Startwert, Beginn).
    bond_wait: Vec<u32>,
    bond_fades: Vec<(u32, Instant)>,
    /// Bytepuffer für die Paneelbilder (wiederverwendet wie ihre Leinwände).
    panel_px: Vec<u8>,
    /// Zuletzt hochgeladene Endsymbole der Schnittlinien (Linie, Anfang,
    /// gespiegelt, hervorgehoben, Skalierung).
    mark_keys: [Option<MarkKey>; MARKS],
    /// Zuletzt hochgeladenes Bild des Nordpfeils.
    nord_bild: Option<nordpfeil::Bild>,
    /// Zuletzt hochgeladene Leiste des Sonnenstands, der Würfel ist
    /// hochgeladen, Lichtrichtung von der Sonne (`None`: die feste).
    sonne_bild: Option<sonne_view::LeistenBild>,
    /// Schatten der Ansichten (S7): Zahnrad und Feld, zuletzt hochgeladenes
    /// Bild.
    ansicht_schatten: ansicht_schatten::Schalter,
    schatten_bild: Option<ansicht_schatten::Bild>,
    /// Ecken des Gebäudes über dem Boden bei eingeschaltetem Sonnenstand:
    /// aus ihnen kommt die Schattenspitze (S6).
    ecken: Vec<Vec3>,
    /// Der zuletzt gesuchte Griff an der Schattenspitze und wofür (Sonne,
    /// Lage, Stand der Ecken): Hover und Bild suchen nur neu, wenn sich
    /// davon etwas ändert, nicht je Mausbewegung über alle Ecken (Review 3cb).
    griff_cache: Option<(GriffSchluessel, Option<sonne_view::Griff>)>,
    ecken_stand: u64,
    wuerfel: bool,
    licht: Option<[f32; 3]>,
    /// Zuletzt hochgeladene Terrassenangaben (Text, Skalierung,
    /// Farbschema) und ihre Bildgröße.
    room_keys: [Option<(RoomKey, u32, u32)>; ROOMS],
    /// Letzte Mausposition im Fenster (Pixel).
    mouse_at: Option<(f64, f64)>,
    /// Hinweis an der Maus: erscheint nach [`TIP_DELAY`] Ruhe über derselben Stelle.
    tip: Option<Tip>,
    notice: Option<Notice>,
    /// Dateimenü am Logo, Tastenkürzel, Nachfrage „Änderungen speichern?“
    /// und der Befehl, der nach der Antwort folgt (E17).
    menu: menu::FileMenu,
    menu_dirty: bool,
    shortcuts: menu::Shortcuts,
    save_dlg: Option<menu::SaveDialog>,
    after_save: Option<Command>,
    /// Maske „Projektdaten“ (Paket PD-2): bei Datei › Neu und am Knopf.
    projektdaten: Option<projektdaten::Maske>,
    /// Liste „Zuletzt geöffnet“; ohne Einstellungsdatei bleibt sie leer.
    recent: menu::Recent,
    recent_on: bool,
    /// Beenden bestätigt: die Schleife endet.
    quit: bool,
    /// Einstellungsfenster (E5), was es sich für die Sitzung merkt, und ob
    /// sein Bild bzw. sein Aufklapper neu zu zeichnen ist.
    prefs: Option<prefs::Prefs>,
    prefs_mem: prefs::Memory,
    prefs_dirty: bool,
    prefs_popup_dirty: bool,
    /// Stand der Vorschau im Fenster „Muster“ (Lage, Szene), `None` = aus.
    pattern_preview: Option<([i32; 4], pattern_view::Input)>,
    /// Einstellungsdatei (Ort, Stand beim Laden).
    settings: settings::Settings,
    /// Firmenkatalog (K2); `None` ohne Einstellungen: eingebauter Startbestand.
    company: Option<catalog::Company>,
    /// Bauteilkatalog (K3), wie das Einstellungsfenster in dessen Ebenen.
    catalog: Option<catalog_view::Catalog>,
    /// Fenster „Baustoffe …“ (Paket 5), wie der Bauteilkatalog vorn; ob
    /// „Mehr“ in dieser Sitzung offen war.
    materials: Option<material_view::MaterialView>,
    /// Fenster „Verwaltung …“ (KA-3a2).
    verwaltung: Option<verwaltung::Verwaltung>,
    /// Verwaltungskennwort in dieser Sitzung eingegeben (KA-3b1): Rolle
    /// Administrator bis Skizzeo schließt.
    admin: bool,
    mat_more: bool,
    /// Typ, den das Werkzeug zeichnet, je Typart (K3: Außen-, Innenwand);
    /// `None`: der Standardtyp.
    tool_type: [Option<sk_model::LayerSetId>; 2],
    /// Typ-Liste am Chip (K3) und ob ihr Bild neu zu zeichnen ist.
    type_menu: Option<type_menu::TypeMenu>,
    type_menu_dirty: bool,
    /// Stand (Revision, Farbschema), für den der Chip im Werkzeug gilt.
    tool_chip_key: Option<(u64, u64)>,
    /// Geschossbogen im Grundriss (E18), seine Bilder, die Uhr seiner
    /// Animation (ms seit dem Start), angefangene Mausradrasten, ein Druck
    /// auf den Bogen (das Loslassen gehört ihm) und der Höhenversatz des
    /// Grundrisses beim Wechsel (Pixel nach unten).
    wheel: wheel::Wheel,
    wheel_view: wheel_view::WheelView,
    clock: Instant,
    wheel_acc: f64,
    wheel_press: bool,
    view_shift: f32,
    /// Schnittbild waagerecht: Versatz (Pixel), Stauchung, Achse (Pixel).
    view_squeeze: (f32, f32, f32),
    /// Blatt wenden beim Spiegeln (E19): Beginn und Achse (Pixel).
    turn: Option<(u64, f32)>,
    /// Gemeinsamer Hover- und Auswahlzustand beider Fenster (F2).
    picking: picking::Picking,
    /// Mengenfenster (F2, B7).
    quantity: quantity::QuantityWindow,
    /// Baumpanel (Paket 4) und sein Zustand in der App: Hover kommt aus dem
    /// Baum, die Maus steht darüber, Auswahl kam aus dem Baum (dann kein
    /// Aufklappen), zuletzt gesehene Auswahl, gesehene Entdecken-Hinweise.
    tree: tree_panel::TreePanel,
    hover_from_tree: bool,
    in_tree: bool,
    tree_picked: bool,
    tree_primary: Option<sk_model::ElementId>,
    tree_props: bool,
    hints_seen: std::collections::BTreeSet<String>,
    /// Der Hover kommt aus der Liste (dann zeigt das Modell den Umriss);
    /// ein Hover im Modell selbst zeigt nur die Zeile in der Liste.
    hover_from_list: bool,
    /// Weiche Kamerafahrt zu Bauteilen (Doppelklick in der Liste).
    fly: Option<Fly>,
    /// Mengenfenster animiert (Rollen, Aufleuchten, Punkte): nächstes Bild bald.
    quantity_busy: bool,
    /// Knopf „Mengenermittlung“ geklickt (wird nach den Ereignissen geöffnet).
    quantity_wanted: bool,
    /// `--geschosswechsel N`: noch so viele Wechsel am Geschossbogen, dann
    /// beenden (Zeitmessung mit `--zeiten`); Richtung des nächsten.
    auto_switch: Option<(u32, bool)>,
    /// Kettensymbole (OG Phase 2): ihre Bilder, die zuletzt gezeigten und das
    /// unter der Maus.
    link_view: link_view::LinkView,
    chips: Vec<link_view::Chip>,
    chip_hover: Option<sk_model::ElementId>,
    /// Die Statuszeile zeigt den Hinweis zum Ziehen einer gestapelten Wand.
    drag_notice: bool,
    /// „Bündig setzen“ gleitet: rückende Wand, ihr Zug, Versatz am
    /// Anfang, Beginn, Zielwand.
    flush_anim: Option<(
        sk_model::ElementId,
        sk_model::RunId,
        f64,
        Instant,
        sk_model::ElementId,
    )>,
    /// Zielwahl beim „Bündig setzen“ (E20) und ihr Maß im Bild (Schlüssel
    /// des gezeichneten Bildes).
    pick: Option<flush_pick::FlushPick>,
    /// Während des Gleitens gelöste, vorher gekoppelte OG-Wand: Kette und
    /// Paneel zeigen den Zustand vor dem Gleiten.
    flush_keep: Option<sk_model::ElementId>,
    pick_label: Option<(String, u32, u64)>,
    /// Bild der Pille (Paket 8): Text, Eingabe, Fehler, Feld, Skalierung,
    /// Farbschema.
    input_pill: Option<(String, bool, bool, usize, u32, u64)>,
    /// Seit wann die Pille nach dem Loslassen steht (Nachkorrektur).
    post_at: Option<Instant>,
    /// Bytes und Größe des gezeigten Pillenbilds und der Beginn des
    /// Überblendens von der Live-Pille zur Eingabe (Darstellung §2.4).
    input_px: Option<(Vec<u8>, u32, u32)>,
    input_swap: Option<Instant>,
    /// Zuletzt in der Statuszeile genannter Fehler der Maßeingabe.
    input_error: Option<meldung::Meldung>,
    /// Fehlschläge des Sicherns in Folge (F-13 §8) und ein Befehl aus einem
    /// Verweis, der das Fenster braucht („Jetzt speichern“).
    save_fail: autosave::FailNotice,
    queued_command: Option<Command>,
    /// Löschen (V?-9): Hinweis am Bauteil (und seine gezeigte Deckkraft),
    /// Kontextmenü, Rückfrage „Gebäude löschen“, jeweils ob ihr Bild neu zu
    /// zeichnen ist; Beginn des Aus- bzw. Einblendens und des Aufleuchtens
    /// abgelehnter Bauteile (Uhr des Geschossbogens, ms).
    hint: Option<delete::HintCard>,
    hint_dirty: bool,
    hint_alpha: f32,
    context: Option<delete::ContextMenu>,
    context_dirty: bool,
    confirm: Option<delete::ConfirmCard>,
    confirm_dirty: bool,
    erase_fade: Option<u64>,
    erase_flash: Option<(u64, Vec<sk_model::ElementId>)>,
    /// Automatisch sichern (F-13); `None` ohne `%APPDATA%` und bei
    /// Bildvergleichen.
    autosave: Option<autosave::AutoSave>,
    /// Startkarte nach einem Absturz bzw. Liste „Sicherungen …“, ob ihr
    /// Bild neu zu zeichnen ist, Beginn ihres Ausblendens (Uhr des
    /// Geschossbogens, ms) und die Sicherungen der zuletzt gezeigten Liste.
    card: Option<backup_card::BackupCard>,
    card_dirty: bool,
    card_fade: Option<u64>,
    /// Lage des Kartenbilds (für das Ausblenden).
    card_at: (i32, i32),
    backups: Vec<autosave::Entry>,
    /// Hilfekarte (Paket 9), ihr Bild (Schlüssel, Lage links oben ohne
    /// Schatten, Rand), ihr Grund je Größe, Ein- bzw. Ausblenden (Beginn
    /// in ms der Uhr, einblenden) und Überblenden beim Themenwechsel.
    help: help::HelpCard,
    help_img: Option<(HelpKey, (f32, f32), f32)>,
    help_ground: Option<help::Ground>,
    help_fade: Option<(u64, bool)>,
    help_swap: Option<u64>,
    /// Bytes und Lage des gezeigten Bildes (für das Überblenden).
    help_px: Option<(Vec<u8>, i32, i32, u32, u32)>,
    /// Die Maus wurde außerhalb der Karte gedrückt und ist noch unten: Bewegen
    /// und Loslassen gehören dem, der das Drücken bekam (Review 3w).
    help_press_elsewhere: bool,
    /// „Bildzeit messen (10 s)“ läuft.
    frame_measure: Option<frame_time::Measure>,
}

/// Schlüssel des Kartenbilds: Inhalt, Skalierung, Farbschema, Fenstergröße
/// und ob ein Fenster offen ist (Lage).
type HelpKey = (help::ImageKey, u32, u64, (u32, u32, bool));

/// Hinweis an der Maus (Text, Lage, seit wann gewünscht, schon sichtbar).
struct Tip {
    text: String,
    at: (f64, f64),
    since: Instant,
    shown: bool,
}

/// Hinweis in der Statuszeile: Text, seit wann er steht, wo (Fenster).
struct Notice {
    text: meldung::Meldung,
    since: Option<Instant>,
    rect: (f64, f64, f64, f64),
    /// So lange steht er; ein Klick öffnet den Bauteilkatalog (`catalog`).
    time: std::time::Duration,
    catalog: bool,
    /// Fehler (Maßeingabe): Punkt in `field_invalid` statt im Akzent.
    error: bool,
}

/// Mehrere Meldungen als Absätze eines Dialogs.
fn meldungen(m: &[meldung::Meldung]) -> String {
    m.iter()
        .map(|x| x.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Nach „Kennwort eingeben …“ im Kostenreiter (Bedienbarkeit 16.1).
const KENNWORT_STIMMT: &str = "Kennwort stimmt. „Auch für neue Häuser“ schreibt jetzt in den Entwurf; gültig wird er mit „Freigeben“.";

/// Schluss, wenn „Auch für neue Häuser“ scheitert (Bedienbarkeit 7.2).
const NICHTS_GEAENDERT: &str = "Nichts geändert; „Nur dieses Haus“ geht weiterhin.";

/// So lange steht ein Hinweis in der Statuszeile.
const NOTICE_TIME: std::time::Duration = std::time::Duration::from_secs(8);

/// Verzögerung bis zum Hinweis an der Maus.
const TIP_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

impl App {
    fn top(&self) -> u32 {
        self.title.height()
    }

    /// Größe der 3D-Ansicht und Skalierung.
    fn view_size(&self) -> (f64, f64, f64) {
        let th = self.top() as f64;
        (self.w as f64, self.h as f64 - th, self.title.scale as f64)
    }

    /// Merkt das Modellnetz zum Neuaufbau vor. Erzeugt wird es höchstens einmal
    /// je Bild in [`App::build_mesh`], egal wie viele Ereignisse es ändern.
    fn upload_model(&mut self) {
        self.mesh_dirty = true;
        self.live_dirty = true;
        self.redraw = true;
    }

    /// Nur der gezogene Wandzug hat sich geändert.
    fn upload_live(&mut self) {
        self.live_dirty = true;
        self.redraw = true;
    }

    /// Verbandstabellen des wilden Verbands (Koordinator 19:55): Eine
    /// Tabelle, die im Hintergrund fertig geworden ist, blendet in `anim_ms`
    /// ein. Gibt an, ob gerade eingeblendet wird.
    fn sync_bonds(&mut self) -> bool {
        let now = Instant::now();
        let anim = self.theme.size.anim_ms;
        let bonds = self.scene.table().bonds.clone();
        for seed in bonds {
            let ready = sk_model::proctex::bond_table_ready(seed).is_some();
            let waiting = self.bond_wait.iter().position(|s| *s == seed);
            match (ready, waiting) {
                (false, None) => self.bond_wait.push(seed),
                (true, Some(i)) => {
                    self.bond_wait.remove(i);
                    if anim > 0.0 {
                        self.bond_fades.push((seed, now));
                    }
                }
                _ => {}
            }
        }
        let n = self.bond_fades.len();
        self.bond_fades
            .retain(|f| now.duration_since(f.1).as_secs_f32() * 1000.0 < anim);
        if !self.bond_fades.is_empty() {
            self.redraw = true;
        }
        n > 0
    }

    /// Fertig gewordene Verbandstabelle oder laufendes Einblenden: neu zeichnen.
    fn bonds_busy(&mut self) {
        let done = self
            .bond_wait
            .iter()
            .any(|&s| sk_model::proctex::bond_table_ready(s).is_some());
        if done || !self.bond_fades.is_empty() {
            self.redraw = true;
        }
    }

    /// Erzeugt die vorgemerkten Netze. Beim Ziehen liegt der gezogene Wandzug in
    /// einem eigenen Live-Netz; nur dieses wird dann je Bild neu erzeugt und
    /// hochgeladen, das ruhende Netz bleibt auf der Grafikkarte.
    fn build_mesh(&mut self) {
        // Attribute oder Skalierung geändert: nur die Tabelle neu, die Netze bleiben
        let fading = self.sync_bonds();
        let t = self.scene.table();
        let key = (
            t.rev,
            t.theme_rev,
            self.title.scale.to_bits(),
            sk_model::proctex::bond_generation(),
        );
        if self.looks_key != Some(key) || fading {
            self.looks_key = Some(key);
            let now = Instant::now();
            let ms = self.theme.size.anim_ms.max(1.0);
            let fades = &self.bond_fades;
            let fade = |seed: u32| {
                fades.iter().find(|f| f.0 == seed).map_or(1.0, |f| {
                    let u = (now.duration_since(f.1).as_secs_f32() * 1000.0 / ms).min(1.0);
                    1.0 - (1.0 - u).powi(3)
                })
            };
            self.renderer
                .set_looks(&t.looks_with(self.title.scale, fade));
        }
        let live = self
            .edit
            .dragging_run()
            .or(self.flush_anim.map(|f| f.1))
            .map_or_else(Vec::new, |r| self.scene.live_set(r));
        if live != self.live_runs {
            self.live_runs = live;
            self.mesh_dirty = true;
            self.live_dirty = true;
        }
        let plane = self.plane();
        if self.mesh_dirty {
            self.mesh_dirty = false;
            let mesh = self.scene.mesh(self.ui.view, plane, &self.live_runs);
            self.renderer.set_mesh(MESH_MODEL, &mesh);
            self.ecken = if self.sonne_an().is_some() {
                sonne_view::ecken(&mesh.faces)
            } else {
                Vec::new()
            };
            self.ecken_stand += 1;
            let ghost = self.scene.ghost_mesh(self.ui.view, plane, &self.live_runs);
            self.renderer.set_mesh(MESH_GHOST, &ghost);
        }
        if self.live_dirty {
            self.live_dirty = false;
            let mesh = self.scene.mesh_runs(self.ui.view, plane, &self.live_runs);
            self.renderer.set_mesh(MESH_LIVE, &mesh);
        }
    }

    /// Paneel „Eigenschaften“ an Auswahl und Modell angleichen. Beim Ziehen bleibt
    /// es stehen; die neuen Mengen kommen beim Loslassen.
    fn sync_props(&mut self) {
        // Beim Gleiten („Bündig setzen“) bleibt das Paneel auf dem Stand davor
        if self.edit.is_dragging()
            || self.ui.level_dragging().is_some()
            || self.flush_anim.is_some()
        {
            return;
        }
        if self.picking.validate(&self.scene) {
            self.redraw = true;
            self.quantity.dirty = true;
        }
        if self.sel.set(self.picking.primary()) {
            self.redraw = true;
        }
        let key = self.sel.id.map(|id| (id, self.scene.model().revision()));
        if key == self.props_key {
            return;
        }
        // Anderes Bauteil: Eigenschaften wieder von oben
        if key.map(|k| k.0) != self.props_key.map(|k| k.0) {
            self.ui.reset_props_scroll();
        }
        self.props_key = key;
        self.ui
            .set_props(self.sel.id.and_then(|id| selection::props(&self.scene, id)));
        self.props_dirty = true;
    }

    /// Paneel „Geschosse“ an das Modell angleichen. Beim Ziehen wird nur sein
    /// Bild erneuert, die Größe bleibt bis zum Loslassen.
    fn sync_levels(&mut self) {
        let found = self.scene.foundation_active();
        let upper = !self.scene.ground_active() && !found;
        if (upper, found) != (self.ui.upper_active, self.ui.foundation_active) {
            (self.ui.upper_active, self.ui.foundation_active) = (upper, found);
            self.overlay_dirty = true;
        }
        // Im Fundament zeichnet das Wandwerkzeug nicht (E18)
        if found && self.tool.enabled {
            self.tool.set_enabled(false);
            self.redraw = true;
        }
        // Außenwände entstehen aus dem EG (E16); Innenwände auch im OG
        if upper && self.tool.enabled && self.tool.category == Category::ExteriorWall {
            self.tool.set_enabled(false);
            self.redraw = true;
        }
        let (z, h) = self.scene.work_plane();
        if self.tool.set_plane(z, h) {
            self.redraw = true;
        }
        // Fangkanten des Hintergrunds (nur bei Modell- oder Geschosswechsel neu)
        let key = (self.scene.model().revision(), self.scene.active_storey());
        if self.snaps_key != Some(key) {
            self.snaps_key = Some(key);
            self.tool.snaps = self.scene.background_snaps();
        }
        if self.ui.set_levels(self.scene.levels()) {
            match self.ui.level_dragging() {
                Some(g) => self.dirty_buttons.push(Id::Grip(g)),
                None => self.overlay_dirty = true,
            }
        }
    }

    /// Wählt das Bauteil (oder nichts). Ein Bauteil eines anderen Gebäudes
    /// macht dessen Geschoss aktiv (B12).
    fn select(&mut self, id: Option<sk_model::ElementId>) {
        // Die Mengenliste folgt beim nächsten Abgleich (nur geänderte Zeilen)
        self.picking.select_only(id);
        if self.sel.set(id) {
            self.redraw = true;
        }
        if id.is_some_and(|e| self.scene.follow_selection(e)) {
            if self.ui.view == ViewKind::Plan {
                self.upload_model();
            }
            self.sync_levels();
        }
    }

    /// Knopf „Gebäude“ ohne Gebäude im Modell: Dialog „Gebäude erstellen“
    /// (E16). Das Gebäude steht sofort im Paneel „Geschosse“.
    fn open_building_dialog(&mut self) {
        if !self.tool_allowed() {
            self.set_view(ViewKind::Persp);
        }
        self.tool.set_enabled(false);
        self.set_wall_kind(Category::ExteriorWall);
        self.scene.open_building_dialog();
        self.ui.dialog = true;
        self.ui.hover = None;
        self.sync_levels();
        self.sync_dialog_fields();
        // Eingabe beginnt in „Dicke OG-Decke“ (oberste Zeile)
        let out = self.ui.focus_field(Field::Draft(Draft::FloorOg));
        self.apply_ui(&out);
        self.overlay_dirty = true;
    }

    /// Dialogfelder an die Vorgaben des entstehenden Gebäudes angleichen.
    fn sync_dialog_fields(&mut self) {
        let d = self.scene.building_draft();
        let rows = Draft::ALL
            .into_iter()
            .map(|k| {
                let (value, (min, max)) = match k {
                    Draft::FloorOg => (d.floor_og, scene::DRAFT_FLOOR),
                    Draft::ClearOg => (d.clear_og, scene::DRAFT_CLEAR),
                    Draft::FloorEg => (d.floor_eg, scene::DRAFT_FLOOR),
                    Draft::ClearEg => (d.clear_eg, scene::DRAFT_CLEAR),
                    Draft::Slab => (d.slab, scene::DRAFT_SLAB),
                };
                FieldRow {
                    field: Field::Draft(k),
                    label: k.label(),
                    value,
                    min,
                    max,
                    zero: false,
                }
            })
            .collect();
        if self.ui.set_dialog_fields(rows) {
            self.overlay_dirty = true;
        }
    }

    /// Dialog schließen: mit „Zeichnen beginnen“ beginnt das Polygon im selben
    /// Schritt, sonst bleibt nichts zurück.
    fn close_building_dialog(&mut self, start: bool) {
        // Jede gültige Eingabe galt schon; eine ungültige verfällt
        if self
            .ui
            .edit
            .as_ref()
            .is_some_and(|e| matches!(e.field, Field::Draft(_)))
        {
            self.ui.edit = None;
        }
        self.ui.dialog = false;
        self.ui.hover = None;
        if start {
            self.tool.set_enabled(true);
        } else {
            self.scene.cancel_building();
            self.tool.set_enabled(false);
        }
        self.sync_levels();
        self.overlay_dirty = true;
        self.refresh_cursor();
    }

    /// Wandeingabe nur auf dem Boden (3D und Grundriss).
    fn tool_allowed(&self) -> bool {
        matches!(self.ui.view, ViewKind::Persp | ViewKind::Plan)
    }

    /// Das Band lässt sich in allen Ansichten ziehen, nur nicht während einer
    /// Wandeingabe oder an der Schnittlinie.
    fn edit_enabled(&self) -> bool {
        !self.tool.is_active()
            && !self.sect.is_busy()
            && self.pick.is_none()
            && !self.nord.is_busy()
    }

    /// Schnittlinie greifen nur im Grundriss und ohne angefangenen Wandzug.
    fn sect_enabled(&self) -> bool {
        self.ui.view == ViewKind::Plan && !self.tool.is_active() && !self.nord.aktiv
    }

    /// Stand des Nordpfeils im Modell (Sonnenstand S2).
    fn nord_stand(&self, vh: f64, sc: f64) -> nordpfeil::Stand {
        let m = self.scene.model();
        nordpfeil::Stand {
            nord: m.location().north,
            fuss: nordpfeil::platz(&self.cam, vh, sc, m.north_foot(), self.scene.bounds()),
            gesetzt: m.north_foot(),
        }
    }

    /// Ereignis an den Nordpfeil (3D und Grundriss, ohne Wandeingabe und
    /// ohne gegriffene Schnittlinie); `true`, wenn er es genommen hat.
    fn nord_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        let en = self.tool_allowed() && !self.tool.enabled && !self.sect.is_busy();
        let st = self.nord_stand(vh, sc);
        self.nord.gebaeude = self.scene.bounds();
        let out = self.nord.handle(ev, st, &self.cam, vw, vh, sc, en);
        self.redraw |= out.redraw;
        self.nord_commit(out.commit);
        // Klick auf den Pfeil schaltet den Sonnenstand (§8 09:25)
        if out.klick {
            let on = self.sonne_an().is_none();
            self.sonne_schalten(on, true);
        }
        out.consumed
    }

    /// Ein Schritt „Nordrichtung geändert“ bzw. „Nordpfeil verschoben“.
    /// Nach dem ersten Aufziehen ist der Sonnenstand an (§8 09:25).
    fn nord_commit(&mut self, c: Option<nordpfeil::Setzen>) {
        if let Some((label, n, f)) = c {
            let neu = self.scene.model().location().north.is_none();
            if self.scene.nordpfeil_setzen(label, n, f) {
                self.overlay_dirty = true;
                if neu {
                    self.sonne_schalten(true, false);
                }
            }
            self.redraw = true;
        }
    }

    /// Sonnenstand eingeschaltet (Sonnenstand S4): Datum und Uhrzeit. Ohne
    /// Nordrichtung (etwa nach Strg+Z des ersten Aufziehens) ruht er.
    fn sonne_an(&self) -> Option<sk_model::Sun> {
        let m = self.scene.model();
        m.sun().filter(|s| s.on && m.location().north.is_some())
    }

    /// Der Himmel bei eingeschaltetem Sonnenstand in 3D: um das Gebäude,
    /// ohne Gebäude um den Würfel.
    fn himmel(&self) -> Option<sonne_view::Himmel> {
        let s = self
            .sonne_an()
            .filter(|_| self.ui.view == ViewKind::Persp)?;
        let q = self
            .scene
            .bounds()
            .unwrap_or_else(sonne_view::wuerfel_quader);
        Some(sonne_view::himmel(self.scene.model().location(), &s, q))
    }

    /// Griff an der Schattenspitze (S6): in 3D, solange die Sonne Schatten
    /// wirft und der Rechner ihn zeichnen kann; ohne Gebäude am Würfel.
    fn griff(&mut self) -> Option<sonne_view::Griff> {
        let s = self
            .sonne_an()
            .filter(|_| self.ui.view == ViewKind::Persp && !self.renderer.shadow_failed())?;
        let haus = self.scene.bounds().is_some();
        let ort = *self.scene.model().location();
        let schluessel = (s, ort, self.ecken_stand, haus);
        if let Some((k, g)) = &self.griff_cache {
            if *k == schluessel {
                return g.clone();
            }
        }
        let wuerfel;
        let punkte = if haus {
            &self.ecken[..]
        } else {
            wuerfel = sonne_view::ecken(&sonne_view::wuerfel_netz().faces);
            &wuerfel[..]
        };
        let g = sonne_view::griff(&ort, &s, punkte);
        self.griff_cache = Some((schluessel, g.clone()));
        g
    }

    /// Sonnenstand an- oder ausschalten (Kachel, Klick auf den Pfeil). Beim
    /// ersten Mal in einer Datei gilt heute 12:00. `zu_3d`: beim Einschalten
    /// in die 3D-Ansicht wechseln, wo das System zu sehen ist.
    fn sonne_schalten(&mut self, on: bool, zu_3d: bool) {
        let s = match self.scene.model().sun() {
            Some(s) => sk_model::Sun { on, ..s },
            None if on => sonne_view::anfang(sonne_view::uhr()),
            None => return,
        };
        self.scene.set_sun(s);
        self.sonne.reset();
        // Die Ecken für die Schattenspitze kommen mit dem Netz
        self.mesh_dirty |= on;
        if on && zu_3d && self.ui.view != ViewKind::Persp {
            self.set_view(ViewKind::Persp);
        }
        self.overlay_dirty = true;
        self.redraw = true;
    }

    /// Wahl des Schattens der aktuellen Ansicht (S7) mit ihrem Platz;
    /// `None` außerhalb der vier Ansichten.
    fn ansicht_wahl(&self) -> Option<(usize, sk_model::ViewShade)> {
        let i = ansicht_schatten::platz(self.ui.view)?;
        Some((
            i,
            self.scene
                .model()
                .view_shade(i, self.settings.vorgaben.schatten),
        ))
    }

    /// Stand der Sonne für die Ansichten (S7).
    fn ansicht_sonne(&self) -> sk_model::Sun {
        ansicht_schatten::sonne_der_ansichten(self.scene.model().sun())
    }

    /// Lage von Zahnrad und Feld in der Ansicht.
    fn ansicht_lage(&self, sc: f64) -> ansicht_schatten::Lage {
        let l = self.scene.model().location();
        let (_, vs) = self
            .ansicht_wahl()
            .unwrap_or((0, self.settings.vorgaben.schatten));
        let tief = vs.light == sk_model::ShadeLight::Sun
            && ansicht_schatten::sonne_waehlbar(l)
            && ansicht_schatten::sonne_zu_tief(l, self.ansicht_sonne());
        let rechts = self.ui.rect(Panel::Views, self.w, self.top()).x;
        ansicht_schatten::lage(rechts, tief, sc as f32, &self.theme)
    }

    /// Ereignis an Zahnrad und Feld „Schatten“ (S7, nur in den vier
    /// Ansichten); `true`, wenn sie es genommen haben.
    fn ansicht_handle(&mut self, ev: &Event, sc: f64) -> bool {
        let Some((i, vs)) = self.ansicht_wahl() else {
            if self.ansicht_schatten.offen || self.ansicht_schatten.hover.is_some() {
                self.ansicht_schatten.reset();
                self.redraw = true;
            }
            return false;
        };
        let lage = self.ansicht_lage(sc);
        let ok = ansicht_schatten::sonne_waehlbar(self.scene.model().location());
        let eigen = self.scene.model().view_shade_own(i).is_some();
        let out = self.ansicht_schatten.handle(ev, &lage, vs, ok, eigen);
        self.redraw |= out.redraw;
        let neu = out.wahl.or(out.alle.then_some(vs));
        if let Some(s) =
            neu.and_then(|w| ansicht_schatten::festschreiben(self.scene.model().sun(), w))
        {
            self.scene.set_sun(s);
        }
        if let Some(w) = out.wahl {
            self.scene
                .set_view_shade(i, w, self.settings.vorgaben.schatten);
            self.redraw = true;
        }
        if out.vorgabe {
            let v = self.settings.vorgaben.schatten;
            self.scene.set_view_shade(i, v, v);
            self.redraw = true;
        }
        if out.alle {
            for k in 0..sk_model::SHADE_VIEWS.len() {
                self.scene
                    .set_view_shade(k, vs, self.settings.vorgaben.schatten);
            }
            self.redraw = true;
        }
        out.consumed
    }

    /// Ereignis an Sonne und Leiste (3D, Sonnenstand an); `true`, wenn sie
    /// es genommen haben.
    fn sonne_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        let (Some(sun), Some(h)) = (self.sonne_an(), self.himmel()) else {
            // Auch ein offenes Feld der Leiste: sonst nähme es unsichtbar
            // jede Taste (Review 3bx)
            if self.sonne.hover.is_some()
                || self.sonne.ueber_sonne
                || self.sonne.ueber_schatten
                || self.sonne.is_busy()
                || self.sonne.eingabe.is_some()
            {
                self.sonne.reset();
                self.redraw = true;
            }
            return false;
        };
        let griff = self.griff();
        let lb = sonne_view::Lagebild {
            sun,
            himmel: &h,
            griff: griff.as_ref(),
            cam: &self.cam,
            wh: (vw, vh),
            scale: sc,
            fonts: &self.ui.fonts,
            theme: &self.theme,
        };
        let out = self.sonne.handle(ev, &lb);
        self.redraw |= out.redraw;
        if let Some(s) = out.sun {
            self.scene.set_sun(s);
            self.redraw = true;
        }
        out.consumed
    }

    /// Grundriss: Höhe der Wandfüße des aktiven Geschosses (mm); nur sie
    /// haben ein Band und ein Kettensymbol.
    fn plan_z(&self) -> Option<f64> {
        if self.ui.view != ViewKind::Plan {
            return None;
        }
        let m = self.scene.model();
        m.storey(self.scene.active_storey()).map(|s| s.elevation)
    }

    /// Schnittebene der aktuellen Ansicht (nur im Schnitt).
    fn plane(&self) -> Option<(Vec3, Vec3)> {
        match self.ui.view {
            ViewKind::Section => self.sect.plane(self.scene.active_cut()),
            _ => None,
        }
    }

    /// Eine Schnittlinie wurde verschoben oder gespiegelt: Stand für die
    /// Datei merken; zeigt die Ansicht diesen Schnitt, neu rechnen.
    fn cut_changed(&mut self, so: &section::SectionOutcome) {
        let Some(i) = so.line.filter(|_| so.changed) else {
            return;
        };
        self.scene.set_cut(i, self.sect.lines[i].cut());
        // Kein Paneel zeigt Lage oder Blickrichtung: nicht alle Paneele neu
        // zeichnen, das kostet je Mausbewegung bis 17 ms (Review 1x)
        self.redraw = true;
        if self.ui.view == ViewKind::Section && i == self.scene.active_cut() {
            self.fit_camera();
            self.upload_model();
            self.refresh_cursor();
        }
    }

    /// Für Gelände- und Höhenlinien: Schnitt B blickt wie die Seitenansicht
    /// längs x.
    fn side_like(&self, v: ViewKind) -> ViewKind {
        if v == ViewKind::Section && self.scene.active_cut() == section::CUT_B {
            ViewKind::Left
        } else {
            v
        }
    }

    /// Knopf „Blickrichtung“ am Schnittrad: den gezeigten Schnitt spiegeln,
    /// das Bild wendet sich wie ein Blatt (E19 §4).
    fn mirror_cut(&mut self) {
        let i = self.scene.active_cut();
        self.sect.lines[i].ensure(&self.scene);
        self.sect.lines[i].mirror();
        if !self.wheel.instant() && self.w > 0 {
            // Achse: Mitte des Gebäudes im Bild
            let (vw, vh, _) = self.view_size();
            let c = self
                .scene
                .bounds()
                .map_or(vec3(0.0, 0.0, 0.0), |(lo, hi)| (lo + hi) * 0.5);
            let axis = self.cam.project(c, vw, vh).map_or(vw * 0.5, |p| p.0) as f32;
            self.renderer.capture_scene();
            self.erase_fade = None;
            self.turn = Some((self.now(), axis));
        }
        let so = section::SectionOutcome {
            changed: true,
            line: Some(i),
            ..Default::default()
        };
        self.cut_changed(&so);
        self.redraw = true;
    }

    fn refresh_cursor(&mut self) {
        self.edit.section = self.plane();
        self.edit.plan_z = self.plan_z();
        let (vw, vh, sc) = self.view_size();
        self.tool.refresh(&self.cam, vw, vh, sc);
        let en = self.edit_enabled();
        self.edit.refresh(&self.scene, &self.cam, vw, vh, sc, en);
        self.redraw = true;
    }

    fn set_view(&mut self, v: ViewKind) {
        if v == self.ui.view {
            return;
        }
        if self.ui.view == ViewKind::Persp {
            self.cam3d = self.cam.clone();
            self.cam3d_empty = self.scene.bounds().is_none();
        }
        let same_mesh = geometry(v) == geometry(self.ui.view);
        self.ui.view = v;
        // Der Knopf „Schnitt“ öffnet den zuletzt benutzten Schnitt (zuerst
        // A); der Bogen blättert dort durch die Schnitte, im Grundriss durch
        // die Geschosse
        self.wheel.set_track(if v == ViewKind::Section {
            wheel::Track::Cuts
        } else {
            wheel::Track::Levels
        });
        self.fit_camera();
        if matches!(v, ViewKind::Plan | ViewKind::Section) {
            self.sect.ensure(&self.scene);
        }
        if !self.tool_allowed() {
            self.nord.set_aktiv(false);
        }
        self.ansicht_schatten.reset();
        if !self.tool_allowed() && self.tool.enabled {
            self.tool.set_enabled(false);
            self.ui.building = false;
        }
        if same_mesh {
            self.redraw = true;
        } else {
            self.upload_model();
        }
        self.overlay_dirty = true;
        self.refresh_cursor();
    }

    /// Kamera der aktiven Ansicht neu setzen und merken, dass sie eingepasst ist.
    fn fit_camera(&mut self) {
        self.cam = self.camera_for(self.ui.view);
        self.fitted = Some(self.cam.clone());
    }

    /// Neue Fenstergröße (Größeziehen, Mengenfenster an- oder abgedockt):
    /// Steht der Grundriss noch so, wie er eingepasst wurde, wird er für die
    /// neue Größe neu eingepasst. Hat der Benutzer verschoben oder gezoomt,
    /// bleibt seine Ansicht.
    fn refit_plan(&mut self) {
        if self.ui.view == ViewKind::Plan && self.fitted.as_ref() == Some(&self.cam) {
            self.fit_camera();
            self.redraw = true;
        }
    }

    /// Kamera beim Wechsel in die Ansicht `v`.
    fn camera_for(&self, v: ViewKind) -> Camera {
        let (vw, vh, _) = self.view_size();
        let tools = self.ui.rect(Panel::Tools, self.w, self.top());
        let free_w = (vw - 2.0 * (tools.x + tools.w) as f64).max(vw * 0.3);
        match v {
            ViewKind::Persp => match self.scene.bounds() {
                // In einer Parallelansicht gezeichnet: Modell ganz zeigen
                Some((lo, hi)) if self.cam3d_empty => fit_perspective(lo, hi),
                _ => self.cam3d.clone(),
            },
            ViewKind::Plan => plan_camera(
                &self.ui,
                &self.wheel,
                self.scene.bounds(),
                self.w,
                self.h,
                self.top(),
            ),
            ViewKind::Section => {
                let i = self.scene.active_cut();
                let flip = self.sect.lines.get(i).is_some_and(|l| l.flip);
                section_camera(
                    &self.ui,
                    &self.wheel,
                    self.scene.bounds(),
                    (self.w, self.h, self.top()),
                    section::view_dir(i, flip),
                )
            }
            _ => fit_parallel(v, self.scene.bounds(), free_w, vh),
        }
    }

    /// Ersetzt das Modell (Neu, Öffnen). Verlauf, Auswahl und angefangene
    /// Eingaben gehen weg; die Kamera zeigt das ganze Modell.
    fn replace_scene(&mut self, model: sk_model::Model) {
        self.install_scene(Scene::with_model(model));
    }

    /// Wie [`App::replace_scene`] mit fertiger Szene.
    fn install_scene(&mut self, scene: Scene) {
        self.scene = scene;
        self.ui.dialog = false;
        self.snaps_key = None;
        self.scene.set_theme(&self.theme);
        self.tool.set_enabled(self.tool.enabled);
        self.set_wall_kind(self.tool.category);
        self.nav = Navigation::default();
        self.edit = WallEdit::default();
        self.sect = Sections::default();
        self.sect.load(&self.scene);
        self.sel = Selection::default();
        self.tree.reset();
        self.props_key = None;
        self.ui.set_props(None);
        self.props_dirty = true;
        self.live_runs.clear();
        // Tabelle sicher neu setzen, auch wenn die neue denselben Stand hat
        self.looks_key = None;
        self.cam3d = start_camera();
        self.cam3d_empty = true;
        self.fit_camera();
        if matches!(self.ui.view, ViewKind::Plan | ViewKind::Section) {
            self.sect.ensure(&self.scene);
        }
        self.upload_model();
        self.overlay_dirty = true;
        self.refresh_cursor();
        // Anderes Projekt: der Takt des Sicherns beginnt neu
        let now = self.clock.elapsed();
        if let Some(a) = self.autosave.as_mut() {
            a.reset(now);
        }
    }

    /// Befehl aus Menü, Titelleiste oder Kürzel ausführen. Neu, Öffnen,
    /// Schließen und Beenden fragen bei ungespeicherten Änderungen nach.
    fn run_command(&mut self, c: Command, surface: &Surface) {
        self.menu.close();
        self.menu_dirty = true;
        match c {
            Command::New | Command::Close | Command::Open | Command::Quit => {
                self.confirm_then(c, surface)
            }
            Command::OpenRecent(i) => match self.recent.get(i) {
                // Fehlt die Datei, fällt sie aus der Liste
                Some(p) if !p.is_file() => self.recent.remove(i),
                Some(_) => self.confirm_then(c, surface),
                None => {}
            },
            Command::Save | Command::SaveAs => {
                self.save(surface, c == Command::SaveAs);
            }
            Command::Undo | Command::Redo => self.history(c == Command::Redo),
            Command::OpenMenu => {
                self.menu.open();
                self.menu.vorschlaege = self.company.as_mut().map_or(0, |c| c.vorschlaege_zahl());
                self.tip = None;
                self.renderer.set_overlay(OVERLAY_TIP, 0, 0, 0, 0, &[]);
            }
            Command::ClearRecent => self.recent.clear(),
            Command::Settings => self.open_prefs(),
            Command::Catalog => self.open_catalog(),
            Command::Materials => self.open_materials(None),
            Command::Verwaltung => self.open_verwaltung(None),
            Command::Backups => self.open_backups(),
            Command::OpenBackup(_) => self.confirm_then(c, surface),
            Command::Delete => self.delete_selection(),
            Command::Help => self.show_help(true),
            Command::MeasureFrameTime => self.start_frame_measure(),
        }
    }

    /// Ein Fenster liegt vorn (Einstellungen, Bauteilkatalog, Baustoffe).
    fn modal(&self) -> bool {
        self.prefs.is_some()
            || self.catalog.is_some()
            || self.materials.is_some()
            || self.verwaltung.is_some()
            || self.projektdaten.is_some()
    }

    /// Bauteilkatalog öffnen (K3); er liegt vorn wie das Einstellungsfenster.
    fn open_catalog(&mut self) {
        if self.modal() {
            return;
        }
        self.close_type_menu(false);
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        let mut c = catalog_view::Catalog::open(&self.scene, self.company.as_ref());
        c.set_nutzer(self.rolle() == sk_cost::Rolle::Nutzer);
        self.catalog = Some(c);
        self.prefs_dirty = true;
        self.overlay_dirty = true;
    }

    /// Offener Bauteilkatalog: nimmt Maus und Tasten wie das
    /// Einstellungsfenster ([`App::handle_prefs`]).
    fn handle_catalog(&mut self, e: Event, surface: &Surface) -> bool {
        let th = self.top() as f64;
        let window_button = |a: &App, x: f64, y: f64| {
            y < th
                && matches!(
                    a.title.button_at(x, y, a.w),
                    Some(Button::Minimize | Button::Maximize | Button::Close)
                )
        };
        match e {
            Event::CloseRequested { .. } => {
                // Wie „Abbrechen“: nichts übernommen
                self.catalog = None;
                self.highlight(Vec::new());
                self.paint_prefs();
                return false;
            }
            Event::Resized { .. }
            | Event::ScaleChanged(_)
            | Event::Maximized(_)
            | Event::Focus(_)
            | Event::Redraw => {
                self.prefs_dirty = true;
                self.prefs_popup_dirty = true;
                return false;
            }
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let over = if window_button(self, x, y) {
                    self.title.button_at(x, y, self.w)
                } else {
                    None
                };
                if over != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(over));
                    self.title.hover = over;
                }
            }
            Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. }
                if window_button(self, x, y) || self.title.pressed.is_some() =>
            {
                return false;
            }
            _ => {}
        }
        let win = self.prefs_win();
        let standard = self.settings.company_place().is_some_and(|(_, s)| s);
        let Some(c) = self.catalog.as_mut() else {
            return false;
        };
        let mut cx = catalog_view::Ctx {
            scene: &mut self.scene,
            theme: &self.theme,
            fonts: &self.ui.fonts,
            win,
            company: self.company.as_mut(),
            company_standard: standard,
        };
        let out = c.handle(&e, &mut cx);
        if out.closed {
            self.catalog = None;
            self.overlay_dirty = true;
        }
        if out.moved {
            if let Some(c) = &self.catalog {
                let (x, y) = c.origin(&self.theme, &win);
                self.renderer.move_overlay(OVERLAY_PREFS, x, y);
                self.redraw = true;
            }
        }
        self.prefs_dirty |= out.repaint || out.closed;
        self.prefs_popup_dirty |= out.popup || out.closed;
        if let Some(h) = out.highlight {
            self.highlight(h);
        } else if out.closed {
            self.highlight(Vec::new());
        }
        if out.pick_company {
            self.pick_company(surface);
        }
        if out.applied {
            self.upload_model();
            self.props_key = None;
            self.tool_chip_key = None;
            self.sync_levels();
            self.refresh_cursor();
        }
        if let Some(g) = out.open_material {
            self.open_materials(Some(g));
        }
        self.sync_caption(surface);
        true
    }

    /// Fenster „Baustoffe …“ öffnen (Paket 5), auf Wunsch mit diesem
    /// Baustoff gewählt.
    fn open_materials(&mut self, select: Option<sk_model::Guid>) {
        if self.modal() {
            return;
        }
        self.close_type_menu(false);
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        let mut m = material_view::MaterialView::open_with(
            &self.scene,
            self.company.as_ref(),
            select,
            self.mat_more,
        );
        m.set_nutzer(self.rolle() == sk_cost::Rolle::Nutzer);
        self.materials = Some(m);
        self.prefs_dirty = true;
        self.overlay_dirty = true;
    }

    /// Verwaltung mit Kennwort (KA-3b2/3b3): Eingaben in den Entwurf
    /// schreiben, eine Änderung oder den ganzen Entwurf verwerfen, freigeben.
    fn verwaltung_entwurf(&mut self, out: &verwaltung::Out, schreiben: bool) {
        let (Some(v), Some(c)) = (self.verwaltung.as_mut(), self.company.as_mut()) else {
            return;
        };
        let h = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Manual);
        if schreiben {
            let ops = v.ops().to_vec();
            match c.fuer_entwurf(&h, &ops) {
                Ok(_) => v.entwurf_gespeichert(c),
                Err(m) => {
                    v.neu_grundlage(c);
                    v.fehler(m.to_string());
                }
            }
        }
        if let Some(satz) = &out.satz_verwerfen {
            match c.entwurf_satz_verwerfen(satz) {
                Ok(()) => v.entwurf_gespeichert(c),
                Err(m) => {
                    v.neu_grundlage(c);
                    v.vorschau_fehler(m.to_string());
                }
            }
        }
        if out.entwurf_verwerfen {
            match c.entwurf_verwerfen() {
                Ok(_) => v.entwurf_gespeichert(c),
                Err(m) => {
                    v.neu_grundlage(c);
                    v.vorschau_fehler(m.to_string());
                }
            }
        }
        if out.freigeben {
            match self.scene.freigeben(verwaltung::FREIGEGEBEN, c, &h) {
                Ok(hinweis) => {
                    let stand =
                        sk_cost::lesen::firma_oder_werk(self.scene.model(), Some(c.library()))
                            .firma_stand
                            .unwrap_or(0);
                    v.entwurf_gespeichert(c);
                    v.freigegeben_als(stand);
                    // Mit der Freigabe kann das Kennwort weg sein
                    self.rolle_angleichen();
                    self.overlay_dirty = true;
                    self.upload_model();
                    self.props_key = None;
                    if let Some(text) = hinweis {
                        self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                        self.notice = Some(Notice {
                            text,
                            since: None,
                            rect: (0.0, 0.0, 0.0, 0.0),
                            time: NOTICE_TIME,
                            catalog: false,
                            error: false,
                        });
                    }
                    self.quantity.dirty = true;
                    self.redraw = true;
                }
                Err(m) => {
                    if let Some(v) = self.verwaltung.as_mut() {
                        if let Some(c) = &self.company {
                            v.neu_grundlage(c);
                        }
                        v.vorschau_fehler(m.to_string());
                    }
                }
            }
        }
    }

    /// Rolle an Szene und Firmenkatalog angleichen: Beide lehnen Firmen- und
    /// Entwurfsänderungen eines Nutzers selbst ab (KA-3b1).
    fn rolle_angleichen(&mut self) {
        let rolle = self.rolle();
        self.scene.rolle = rolle;
        if let Some(c) = self.company.as_mut() {
            c.set_nutzer(rolle == sk_cost::Rolle::Nutzer);
        }
    }

    /// Rolle dieser Sitzung (KA-3b1): mit Verwaltungskennwort Nutzer, bis
    /// es hier eingegeben ist; am Einzelplatz jeder Administrator.
    fn rolle(&self) -> sk_cost::Rolle {
        match &self.company {
            Some(c) if !self.admin && sk_cost::verwaltung::hat_kennwort(c.library()) => {
                sk_cost::Rolle::Nutzer
            }
            _ => sk_cost::Rolle::Admin,
        }
    }

    /// Fenster „Verwaltung …“ öffnen (KA-3a2), auf Wunsch mit diesem
    /// Eintrag gewählt („Bauleistung öffnen ↗“ im AVA).
    fn open_verwaltung(&mut self, wahl: Option<verwaltung::Knoten>) {
        if self.modal() {
            return;
        }
        self.close_type_menu(false);
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        // Mit Kennwort zeigt die Verwaltung den Entwurf, wie er jetzt ist
        if let Some(c) = self.company.as_mut() {
            c.entwurf_laden();
        }
        let mut v = verwaltung::Verwaltung::open(&self.scene, self.company.as_ref(), wahl);
        v.set_haus_name(&self.doc.name());
        if self.rolle() == sk_cost::Rolle::Nutzer {
            v.sperren();
        }
        self.verwaltung = Some(v);
        self.prefs_dirty = true;
        self.overlay_dirty = true;
    }

    /// „Kennwort eingeben …“ im Preis- oder Lohnblatt (Bedienbarkeit 16.1):
    /// nur die Abfrage „Verwaltung öffnen“; stimmt das Kennwort, schließt
    /// sie, und das offene Blatt schreibt wieder „Auch für neue Häuser“.
    fn kennwort_abfragen(&mut self) {
        if self.rolle() != sk_cost::Rolle::Nutzer {
            return;
        }
        self.open_verwaltung(None);
        if let Some(v) = self.verwaltung.as_mut() {
            v.nur_kennwort();
        }
    }

    /// Ablauf `kind=user` aus dem Reiter Kosten (paket-ka3b §3): nur das
    /// Blatt des Assistenten, ohne Kennwort; „Eintragen“ schreibt ins Haus
    /// als einen Rückgängig-Schritt.
    fn ablauf_im_haus(&mut self, g: sk_model::Guid) {
        if self.modal() {
            return;
        }
        self.close_type_menu(false);
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        let mut v = verwaltung::Verwaltung::open(&self.scene, self.company.as_ref(), None);
        if v.ablauf_im_haus(g) {
            self.verwaltung = Some(v);
            self.prefs_dirty = true;
            self.overlay_dirty = true;
        }
    }

    /// Offenes Fenster „Verwaltung …“: Maus und Tasten wie „Baustoffe …“;
    /// OK schreibt über `Scene::fuer_firma` (Bausteingrenze §5).
    fn handle_verwaltung(&mut self, e: Event, surface: &Surface) -> bool {
        let th = self.top() as f64;
        let window_button = |a: &App, x: f64, y: f64| {
            y < th
                && matches!(
                    a.title.button_at(x, y, a.w),
                    Some(Button::Minimize | Button::Maximize | Button::Close)
                )
        };
        match e {
            Event::CloseRequested { .. } => {
                self.verwaltung = None;
                self.paint_prefs();
                return false;
            }
            Event::Resized { .. }
            | Event::ScaleChanged(_)
            | Event::Maximized(_)
            | Event::Focus(_)
            | Event::Redraw => {
                self.prefs_dirty = true;
                return false;
            }
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let over = if window_button(self, x, y) {
                    self.title.button_at(x, y, self.w)
                } else {
                    None
                };
                if over != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(over));
                    self.title.hover = over;
                }
            }
            Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. }
                if window_button(self, x, y) || self.title.pressed.is_some() =>
            {
                return false;
            }
            _ => {}
        }
        let win = self.prefs_win();
        let Some(v) = self.verwaltung.as_mut() else {
            return false;
        };
        let mut cx = verwaltung::Ctx {
            fonts: &self.ui.fonts,
            win,
        };
        let mut out = v.handle(&e, &mut cx);
        if out.frei {
            self.admin = true;
            self.scene.rolle = sk_cost::Rolle::Admin;
            if let Some(c) = self.company.as_mut() {
                c.set_nutzer(false);
            }
            if let Some(k) = self.quantity.kosten.as_mut() {
                k.set_vorschlag(false, true);
            }
            self.quantity.dirty = true;
        }
        if let Some(haus) = out.haus.take() {
            let h = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Manual);
            let label = self.scene.bezeichnung(haus.name);
            // `check`-Schritte sperren wie in der Verwaltung (Review 3ax)
            let firma = self.company.as_ref().map(|c| c.library());
            let gesperrt = self
                .scene
                .ablauf_pruefen(firma, &h, &haus.ops, &haus.regeln);
            if let Some(satz) = gesperrt {
                if let Some(v) = self.verwaltung.as_mut() {
                    v.ablauf_fehler(format!("Nicht eingetragen: {satz}"));
                }
                self.sync_caption(surface);
                return true;
            }
            match self.kosten_folge(label, &h, &haus.ops) {
                Some((m, true)) => {
                    if let Some(v) = self.verwaltung.as_mut() {
                        v.ablauf_fehler(m.to_string());
                    }
                }
                _ => {
                    self.verwaltung = None;
                    self.overlay_dirty = true;
                    self.status(meldung::Meldung::mit("{}", &[&haus.schluss]), NOTICE_TIME);
                    self.quantity.dirty = true;
                    self.redraw = true;
                }
            }
            self.prefs_dirty = true;
            self.sync_caption(surface);
            return true;
        }
        if out.ok {
            let ops = v.ops().to_vec();
            let h = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Manual);
            let r = match self.company.as_mut() {
                Some(c) => self.scene.fuer_firma(verwaltung::STEP, c, &h, &ops),
                None => Err(meldung::Meldung::satz("Kein Firmenkatalog geladen.")),
            };
            match r {
                Ok(hinweis) => {
                    // Wer das Kennwort setzt, kennt es
                    if ops
                        .iter()
                        .any(|o| matches!(o, sk_cost::Op::KennwortSetzen { pw } if !pw.ist_leer()))
                    {
                        self.admin = true;
                    }
                    self.rolle_angleichen();
                    self.verwaltung = None;
                    self.overlay_dirty = true;
                    self.upload_model();
                    self.props_key = None;
                    if let Some(text) = hinweis {
                        self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                        self.notice = Some(Notice {
                            text,
                            since: None,
                            rect: (0.0, 0.0, 0.0, 0.0),
                            time: NOTICE_TIME,
                            catalog: false,
                            error: false,
                        });
                    }
                    self.quantity.dirty = true;
                    self.redraw = true;
                }
                Err(m) => {
                    if let Some(v) = self.verwaltung.as_mut() {
                        if let Some(c) = &self.company {
                            v.neu_grundlage(c);
                        }
                        v.fehler(m.to_string());
                    }
                }
            }
            self.prefs_dirty = true;
        }
        let mut entwurf = out.entwurf;
        if let Some(stand) = out.zurueck {
            let r = match self.company.as_ref() {
                Some(c) => c.umkehr(self.scene.model(), stand),
                None => Err(meldung::Meldung::satz("Kein Firmenkatalog geladen.")),
            };
            if let Some(v) = self.verwaltung.as_mut() {
                v.zuruecknehmen(stand, r.map_err(|m| m.to_string()));
                entwurf |= v.entwurf_faellig();
            }
            self.prefs_dirty = true;
        }
        if entwurf || out.freigeben || out.entwurf_verwerfen || out.satz_verwerfen.is_some() {
            self.verwaltung_entwurf(&out, entwurf);
            self.prefs_dirty = true;
        }
        if out.closed {
            if let Some(m) = self.verwaltung.take().and_then(|v| v.beim_schliessen()) {
                self.status(m, NOTICE_TIME);
            } else if out.frei {
                self.status(meldung::Meldung::satz(KENNWORT_STIMMT), NOTICE_TIME);
            }
            self.overlay_dirty = true;
        }
        if out.moved {
            if let Some(v) = &self.verwaltung {
                let (x, y) = v.origin(&self.theme, &win);
                self.renderer.move_overlay(OVERLAY_PREFS, x, y);
                self.redraw = true;
            }
        }
        self.prefs_dirty |= out.repaint || out.closed;
        if let Some(g) = out.open_type {
            // Typpflege bleibt im Bauteilkatalog (paket-ka3a §3)
            self.verwaltung = None;
            self.overlay_dirty = true;
            self.prefs_dirty = true;
            self.open_catalog();
            if let Some(c) = self.catalog.as_mut() {
                c.show_guid(g);
            }
        }
        self.sync_caption(surface);
        true
    }

    /// Offenes Fenster „Baustoffe …“: nimmt Maus und Tasten wie der
    /// Bauteilkatalog ([`App::handle_catalog`]).
    fn handle_materials(&mut self, e: Event, surface: &Surface) -> bool {
        let th = self.top() as f64;
        let window_button = |a: &App, x: f64, y: f64| {
            y < th
                && matches!(
                    a.title.button_at(x, y, a.w),
                    Some(Button::Minimize | Button::Maximize | Button::Close)
                )
        };
        match e {
            Event::CloseRequested { .. } => {
                // Wie „Abbrechen“: nichts übernommen
                if let Some(v) = self.materials.take() {
                    self.mat_more = v.more();
                }
                self.paint_prefs();
                return false;
            }
            Event::Resized { .. }
            | Event::ScaleChanged(_)
            | Event::Maximized(_)
            | Event::Focus(_)
            | Event::Redraw => {
                self.prefs_dirty = true;
                return false;
            }
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let over = if window_button(self, x, y) {
                    self.title.button_at(x, y, self.w)
                } else {
                    None
                };
                if over != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(over));
                    self.title.hover = over;
                }
            }
            Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. }
                if window_button(self, x, y) || self.title.pressed.is_some() =>
            {
                return false;
            }
            _ => {}
        }
        let win = self.prefs_win();
        let standard = self.settings.company_place().is_some_and(|(_, s)| s);
        let Some(v) = self.materials.as_mut() else {
            return false;
        };
        let mut cx = material_view::Ctx {
            scene: &mut self.scene,
            theme: &self.theme,
            fonts: &self.ui.fonts,
            win,
            company: self.company.as_mut(),
            company_standard: standard,
        };
        let out = v.handle(&e, &mut cx);
        if out.closed {
            self.mat_more = v.more();
            self.materials = None;
            self.overlay_dirty = true;
        }
        if out.moved {
            if let Some(v) = &self.materials {
                let (x, y) = v.origin(&self.theme, &win);
                self.renderer.move_overlay(OVERLAY_PREFS, x, y);
                self.redraw = true;
            }
        }
        self.prefs_dirty |= out.repaint || out.closed;
        if out.pick_company {
            self.pick_company(surface);
        }
        if !out.problems.is_empty() {
            // Review 3n/4: nichts still verwerfen
            surface.message(&out.problems.join("\n"), false);
        }
        if out.applied {
            self.upload_model();
            self.props_key = None;
            self.tool_chip_key = None;
            self.sync_levels();
            self.refresh_cursor();
        }
        if let Some(g) = out.open_type {
            self.open_catalog();
            if let Some(c) = self.catalog.as_mut() {
                c.show_guid(g);
            }
        }
        self.sync_caption(surface);
        true
    }

    /// Wände im Modell hervorheben (Rückfrage im Bauteilkatalog); leer: aus.
    fn highlight(&mut self, ids: Vec<sk_model::ElementId>) {
        let on = !ids.is_empty();
        if (on || self.hover_from_list) && self.picking.set_hover(None, ids) {
            self.redraw = true;
        }
        self.hover_from_list = on;
    }

    /// „ändern …“ im Bauteilkatalog: anderer Ort des Firmenkatalogs (F1).
    fn pick_company(&mut self, surface: &Surface) {
        let filters = [
            ("Firmenkatalog (*.szk)", "*.szk"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(p) = surface.open_dialog("Firmenkatalog", &filters) else {
            return;
        };
        let (c, hints) = catalog::Company::laden(&p, false);
        self.show_hints(hints, surface);
        self.settings.set_company_path(p);
        let panel = self.panel_settings();
        save_settings(
            &mut self.settings,
            &self.theme,
            &self.recent,
            (self.quantity.grouping, self.quantity.blatt()),
            panel,
            surface,
        );
        self.company = Some(c);
        if let Some(cat) = self.catalog.as_mut() {
            cat.set_company(self.company.as_ref());
        }
        if let Some(v) = self.materials.as_mut() {
            v.set_company(self.company.as_ref());
        }
        self.prefs_dirty = true;
        self.prefs_popup_dirty = true;
    }

    /// Einstellungsfenster öffnen; ist es offen, bleibt es (es liegt ohnehin
    /// vorn).
    fn open_prefs(&mut self) {
        if self.modal() {
            return;
        }
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        let p = prefs::Prefs::open(&mut self.scene, &self.theme)
            .with_memory(&self.prefs_mem)
            .with_vorgaben(self.settings.vorgaben)
            .with_pattern_error(self.renderer.pattern_error());
        self.prefs = Some(p);
        if let (Some(p), Some(c)) = (self.prefs.as_mut(), &self.company) {
            p.set_company_presets(c.library().presets.clone());
        }
        self.prefs_dirty = true;
        self.overlay_dirty = true;
    }

    /// Lage und Skalierung für das Einstellungsfenster.
    fn prefs_win(&self) -> prefs::Win {
        prefs::Win {
            w: self.w,
            h: self.h,
            top: self.title.height(),
            scale: self.title.scale,
        }
    }

    /// Farbschema hat sich geändert (Einstellungsfenster): Zeichentabelle,
    /// Renderer-Stil, alle Paneele und Bilder neu.
    fn theme_changed(&mut self) {
        self.scene.set_theme(&self.theme);
        self.wheel.set_theme(&self.theme);
        self.wheel_view.forget();
        self.renderer.set_style(style(&self.theme.env));
        // Die Sonne setzt ihr Licht beim nächsten Bild wieder (S4)
        self.licht = None;
        self.sonne_bild = None;
        self.ui.forget_theme();
        self.ui.use_theme(&self.theme);
        self.ui.fit(self.title.scale, self.w, self.h);
        self.looks_key = None;
        self.mark_keys = [None; MARKS];
        self.room_keys = Default::default();
        self.overlay_dirty = true;
        self.prefs_dirty = true;
        self.prefs_popup_dirty = true;
        self.redraw = true;
        self.refresh_cursor();
    }

    /// Offenes Einstellungsfenster: nimmt Maus und Tasten. Die Fensterknöpfe
    /// der Titelleiste, Größe, Fokus und Schließen gehen weiter an
    /// [`App::handle_inner`] (`false`).
    fn handle_prefs(&mut self, e: Event, surface: &Surface) -> bool {
        let th = self.top() as f64;
        let window_button = |a: &App, x: f64, y: f64| {
            y < th
                && matches!(
                    a.title.button_at(x, y, a.w),
                    Some(Button::Minimize | Button::Maximize | Button::Close)
                )
        };
        match e {
            Event::CloseRequested { ask } => {
                // Wie OK, danach fragt „Beenden“ nach dem Projekt
                if ask {
                    if let Some(mut p) = self.prefs.take() {
                        p.ok(&mut self.scene, &self.theme, &mut self.settings);
                        self.prefs_mem = p.memory();
                    }
                    self.paint_prefs();
                }
                return false;
            }
            Event::Resized { .. }
            | Event::ScaleChanged(_)
            | Event::Maximized(_)
            | Event::Focus(_)
            | Event::Redraw => {
                self.prefs_dirty = true;
                self.prefs_popup_dirty = true;
                return false;
            }
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let over = if window_button(self, x, y) {
                    self.title.button_at(x, y, self.w)
                } else {
                    None
                };
                if over != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(over));
                    self.title.hover = over;
                }
            }
            Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. }
                if window_button(self, x, y) || self.title.pressed.is_some() =>
            {
                return false;
            }
            _ => {}
        }
        let win = self.prefs_win();
        let Some(p) = self.prefs.as_mut() else {
            return false;
        };
        let mut cx = prefs::Ctx {
            scene: &mut self.scene,
            theme: &mut self.theme,
            settings: &mut self.settings,
            fonts: &self.ui.fonts,
            win,
        };
        let out = p.handle(&e, &mut cx);
        if out.closed {
            self.prefs_mem = p.memory();
            self.prefs = None;
            self.overlay_dirty = true;
        }
        if out.moved {
            if let Some(p) = &self.prefs {
                let (x, y) = p.origin(&self.theme, &win);
                self.renderer.move_overlay(OVERLAY_PREFS, x, y);
                self.redraw = true;
            }
            self.sync_pattern_preview();
        }
        if out.save_preset {
            self.save_pattern_preset();
        }
        self.prefs_dirty |= out.repaint || out.closed;
        self.prefs_popup_dirty |= out.popup || out.closed;
        if out.theme {
            self.theme_changed();
        }
        if out.model {
            // Stifte wirken über die Zeichentabelle, ohne Netzneubau
            self.redraw = true;
            self.props_key = None;
        }
        self.sync_caption(surface);
        true
    }

    /// Einstellungsfenster und Aufklapper zeichnen oder ausblenden.
    fn paint_prefs(&mut self) {
        self.prefs_dirty = false;
        self.paint_prefs_popup();
        let win = self.prefs_win();
        if let Some(v) = self.verwaltung.as_mut() {
            if let catalog_view::Frame::Full { x, y, w, h, px } =
                v.paint_frame(&self.theme, &self.ui.fonts, &win)
            {
                self.renderer.set_overlay(OVERLAY_PREFS, x, y, w, h, &px);
            }
            self.redraw = true;
            return;
        }
        if let Some(v) = self.materials.as_mut() {
            // Nach dem Überfahren nur die Ausschnitte (B6)
            match v.paint_frame(&self.theme, &self.ui.fonts, &win) {
                catalog_view::Frame::Full { x, y, w, h, px } => {
                    self.renderer.set_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                }
                catalog_view::Frame::Parts(parts) => {
                    for (x, y, w, h, px) in parts {
                        self.renderer.update_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                    }
                }
            }
            self.redraw = true;
            return;
        }
        if let Some(cat) = self.catalog.as_mut() {
            if cat.asking() {
                // Die Rückfrage zeigt die Wände: das Fenster tritt zurück
                self.renderer.set_overlay(OVERLAY_PREFS, 0, 0, 0, 0, &[]);
                self.redraw = true;
                return;
            }
            // Nach Hervorhebungen und Übergängen nur die Ausschnitte (U7)
            match cat.paint_frame(&self.theme, &self.ui.fonts, &win) {
                catalog_view::Frame::Full { x, y, w, h, px } => {
                    self.renderer.set_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                    cat.give_back(px);
                }
                catalog_view::Frame::Parts(parts) => {
                    for (x, y, w, h, px) in parts {
                        self.renderer.update_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                    }
                }
            }
            self.redraw = true;
            return;
        }
        let Some(p) = self.prefs.as_mut() else {
            self.renderer.set_overlay(OVERLAY_PREFS, 0, 0, 0, 0, &[]);
            self.sync_pattern_preview();
            return;
        };
        // Nach dem Überfahren nur Streifen (A302)
        match p.paint_frame(&self.theme, &self.ui.fonts, &win, &self.scene) {
            catalog_view::Frame::Full { x, y, w, h, px } => {
                self.renderer.set_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                p.give_back_bytes(px);
            }
            catalog_view::Frame::Parts(parts) => {
                for (x, y, w, h, px) in parts {
                    self.renderer.update_overlay(OVERLAY_PREFS, x, y, w, h, &px);
                }
            }
        }
        self.sync_pattern_preview();
        self.redraw = true;
    }

    /// Vorschau im Fenster „Muster“ (Paket 7b): Szene und Aussehen nur bei
    /// Änderung neu hochladen, sonst nur Lage und Überblendung setzen. Stift
    /// „Ansichtsmuster“, Papier und Kanten kommen aus der Zeichentabelle.
    fn sync_pattern_preview(&mut self) {
        let win = self.prefs_win();
        let pv = self
            .prefs
            .as_mut()
            .and_then(|p| p.pattern_preview(&self.theme, &win, &self.scene));
        let Some(mut pv) = pv else {
            if self.pattern_preview.take().is_some() {
                self.renderer.set_preview(None);
                self.redraw = true;
            }
            return;
        };
        let t = self.scene.table();
        let s = self.title.scale;
        let (w, c) = t.pattern;
        pv.input.ink = [c[0], c[1], c[2], w * s];
        pv.input.paper = t.paper;
        pv.input.model_edges = t.edge_looks(false, s);
        pv.input.drawing_edges = t.edge_looks(true, s);
        if pv.hold {
            self.renderer.hold_preview();
        }
        // Aussehen immer abgleichen: eine fertig gewordene Verbandstabelle
        // ändert es ohne neue Szene
        self.renderer
            .set_preview_looks(&pattern_view::preview_looks(&pv.input, &self.theme));
        let same = self
            .pattern_preview
            .as_ref()
            .is_some_and(|(at, inp)| at[2..] == pv.at[2..] && *inp == pv.input);
        if same {
            self.renderer.move_preview(pv.at[0], pv.at[1]);
        } else {
            let (meshes, items) = pattern_view::preview_scene(&pv.input);
            for (i, m) in meshes.iter().enumerate() {
                self.renderer.set_preview_mesh(i, m);
            }
            self.renderer.set_preview(Some(sk_render::Preview {
                at: pv.at,
                before: OVERLAY_PREFS,
                items,
            }));
        }
        self.pattern_preview = Some((pv.at, pv.input));
        let fade = self
            .prefs
            .as_ref()
            .map_or(0.0, |p| p.pattern_fade(&self.theme));
        self.renderer.set_preview_fade(fade);
        self.redraw = true;
    }

    /// „Als Vorlage speichern …“: in den Firmenkatalog schreiben und das
    /// Ergebnis im Fuß des Fensters „Muster“ zeigen.
    fn save_pattern_preset(&mut self) {
        let Some(p) = self.prefs.as_mut() else {
            return;
        };
        let Some((name, pattern, base)) = p.take_save_preset() else {
            return;
        };
        let r = match self.company.as_mut() {
            None => Err("Kein Firmenkatalog geladen (Bauteilkatalog ▸ „ändern …“).".to_string()),
            Some(c) => match c.save_preset(&name, &pattern, base) {
                catalog::SaveResult::Saved => Ok(name.trim().to_string()),
                catalog::SaveResult::Changed => Err(
                    "Der Firmenkatalog wurde inzwischen geändert; bitte neu laden und erneut speichern."
                        .to_string(),
                ),
                catalog::SaveResult::Failed(e) => Err(e),
            },
        };
        p.preset_saved(r);
        if let Some(c) = &self.company {
            p.set_company_presets(c.library().presets.clone());
        }
        self.prefs_dirty = true;
    }

    fn paint_prefs_popup(&mut self) {
        self.prefs_popup_dirty = false;
        self.redraw = true;
        let win = self.prefs_win();
        let img = match self.catalog.as_mut() {
            Some(cat) => cat.paint_popup(&self.theme, &self.ui.fonts, &win, self.scene.model()),
            None => self
                .prefs
                .as_mut()
                .and_then(|p| p.paint_popup(&self.theme, &self.ui.fonts, &win, &self.scene)),
        };
        match img {
            Some((c, x, y)) => {
                let px = c.to_premul_rgba8();
                self.renderer.set_overlay(
                    OVERLAY_PREFS_POPUP,
                    x,
                    y,
                    c.width as u32,
                    c.height as u32,
                    &px,
                );
            }
            None => self
                .renderer
                .set_overlay(OVERLAY_PREFS_POPUP, 0, 0, 0, 0, &[]),
        }
    }

    /// Ohne ungespeicherte Änderungen sofort ausführen, sonst nachfragen.
    fn confirm_then(&mut self, c: Command, surface: &Surface) {
        if !self.doc.is_dirty(self.scene.model()) && !self.scene.building_pending() {
            self.perform(c, surface);
            return;
        }
        let (q, detail) = menu::save_question(&self.doc, self.doc.saved_at);
        self.save_dlg = Some(menu::SaveDialog::new(q, detail));
        self.after_save = Some(c);
        self.overlay_dirty = true;
    }

    /// Antwort auf die Nachfrage: speichern (bei „Unbenannt“ mit „Speichern
    /// unter“; dort abgebrochen, bricht auch der Befehl ab), verwerfen oder
    /// nichts tun.
    fn answer_save(&mut self, a: menu::SaveAnswer, surface: &Surface) {
        self.save_dlg = None;
        self.overlay_dirty = true;
        let Some(c) = self.after_save.take() else {
            return;
        };
        let go = match a {
            menu::SaveAnswer::Save => self.save(surface, false),
            menu::SaveAnswer::Discard => true,
            menu::SaveAnswer::Cancel => false,
        };
        if go {
            self.perform(c, surface);
        }
    }

    /// Befehl nach der Nachfrage ausführen.
    fn perform(&mut self, c: Command, surface: &Surface) {
        match c {
            Command::New | Command::Close => {
                self.replace_scene(new_model(self.company.as_ref(), self.settings.vorgaben));
                self.doc = Document::new(self.scene.model().revision());
                // Bei „Neu“ zuerst die Projektdaten (Paket PD-2)
                if c == Command::New {
                    self.open_projektdaten(true);
                }
            }
            Command::Open => {
                if let Some(path) = surface.open_dialog("Öffnen", &document::FILTERS) {
                    self.open_path(surface, path);
                }
            }
            Command::OpenRecent(i) => {
                if let Some(p) = self.recent.get(i).cloned() {
                    self.open_path(surface, p);
                }
            }
            Command::Quit => self.quit = true,
            Command::OpenBackup(i) => {
                if let Some(e) = self.backups.get(i).cloned() {
                    self.restore_backup(&e.path, e.original.as_deref(), surface);
                }
            }
            _ => {}
        }
    }

    /// Rückgängig bzw. Wiederherstellen (Knopf und Kürzel).
    fn history(&mut self, redo: bool) {
        if self.tool.is_active() || self.edit.is_dragging() {
            return;
        }
        // Ein Lösch-Schritt blendet ein bzw. wieder aus (V?-9)
        let label = if redo {
            self.scene.redo_label()
        } else {
            self.scene.undo_label()
        };
        let fade = self.erase_anim() && label.is_some_and(|l| l.ends_with("gelöscht"));
        if fade {
            self.renderer.capture_scene();
        }
        let changed = if redo {
            self.scene.redo()
        } else {
            self.scene.undo()
        };
        if fade && changed {
            self.erase_fade = Some(self.now());
        } else if fade {
            self.renderer.release_snapshot();
        }
        if changed {
            self.hint = None;
            self.hint_dirty = true;
            self.upload_model();
            self.sync_levels();
            self.refresh_cursor();
        }
        // Firmenänderung: Strg+Z nimmt nur dieses Haus zurück, die
        // Statuszeile sagt einmal, was für neue Häuser bleibt (paket-ka2 §4)
        let was = label.and_then(|l| l.strip_suffix(preis_blatt::FUER_NEUE));
        if let (true, false, Some(was)) = (changed, redo, was) {
            self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
            self.notice = Some(Notice {
                text: meldung::Meldung::mit(
                    "Zurückgenommen für dieses Haus. Neue Häuser rechnen weiter mit {}.",
                    &[was],
                ),
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: NOTICE_TIME,
                catalog: false,
                error: false,
            });
        }
    }

    /// Eine Datei kommt in „Zuletzt geöffnet“ (nur mit Einstellungsdatei).
    fn remember(&mut self, path: &std::path::Path) {
        if self.recent_on {
            self.recent.push(path.to_path_buf());
        }
    }

    /// Speichern (bzw. „Speichern unter“, wenn `as_new` oder noch ohne Datei).
    /// `true`, wenn gespeichert wurde.
    fn save(&mut self, surface: &Surface, as_new: bool) -> bool {
        let path = match (&self.doc.path, as_new) {
            (Some(p), false) => p.clone(),
            _ => {
                let suggested = self
                    .doc
                    .path
                    .as_ref()
                    .map_or("Unbenannt".into(), |p| p.display().to_string());
                match surface.save_dialog("Speichern unter", &document::FILTERS, "szo", &suggested)
                {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        match document::save(self.scene.model(), &path) {
            Ok(()) => {
                self.remember(&path);
                self.doc.mark_saved(path, self.scene.model().revision());
                self.doc.saved_at = Some(sk_platform::local_time());
                let now = self.clock.elapsed();
                if let Some(a) = self.autosave.as_mut() {
                    a.saved(&self.doc, now);
                }
                let step = self.save_fail.saved(now);
                self.fail_notice(step);
                true
            }
            Err(e) => {
                surface.message(&e, true);
                false
            }
        }
    }

    fn open_path(&mut self, surface: &Surface, path: std::path::PathBuf) {
        match document::load(&path) {
            Ok(loaded) => {
                self.replace_scene(loaded.model);
                self.remember(&path);
                self.doc = Document::opened(path, self.scene.model().revision());
                self.doc.saved_at = Some(sk_platform::local_time());
                // Im Bildschirmfoto-Modus hielte die Meldung das Bild auf
                let shot = std::env::args().any(|a| a == "--screenshot");
                if !loaded.hints.is_empty() && !shot {
                    surface.message(&document::hints_message(&loaded.hints), false);
                }
            }
            Err(e) => surface.message(&e, true),
        }
    }

    /// Dateiname und `•` in Titelleiste und Taskleiste.
    fn sync_caption(&mut self, surface: &Surface) {
        let caption = self.doc.caption_at(self.scene.shown_revision());
        if caption != self.title.caption {
            surface.set_title(&format!("{caption} – Skizzeo"));
            self.title.caption = caption;
            if self.w > 0 {
                self.paint_title(surface);
                self.redraw = true;
            }
        }
    }

    /// Öffnet das Mengenfenster (F2) oder holt es nach vorn.
    fn open_quantity(&mut self, surface: &Surface) {
        self.quantity.open = true;
        self.quantity.dirty = true;
        let caption = windows::blatt_caption(
            self.quantity.blatt(),
            &self.doc,
            self.scene.shown_revision(),
        );
        self.quantity.title.caption = caption.clone();
        surface.open_quantity(&caption);
        if !self.ui.quantity_open {
            self.ui.quantity_open = true;
            self.dirty_buttons.push(Id::Quantity);
        }
        // Entdecken (Paket 5 §1.1): einmalig die Karte zu „Baustoffe …“
        if self.hints_seen.insert("materials".into()) {
            if let Some(text) = hints::text("materials") {
                let lines = text.lines().map(str::to_string).collect();
                let link = ("Baustoffe öffnen", delete::Link::Materials);
                self.quantity.discover(lines, link, Instant::now());
            }
        }
    }

    /// Mengenfenster schließen (✕ oder Taskleiste).
    fn close_quantity(&mut self, surface: &Surface) {
        surface.close_quantity();
        self.quantity.open = false;
        self.quantity.close_popups();
        if self.ui.quantity_open {
            self.ui.quantity_open = false;
            self.dirty_buttons.push(Id::Quantity);
        }
        if self.hover_from_list && self.picking.set_hover(None, Vec::new()) {
            self.redraw = true;
        }
        self.hover_from_list = false;
    }

    /// Ereignis aus dem Mengenfenster; Hover und Auswahl wirken in beiden Fenstern.
    fn handle_quantity(&mut self, e: Event, surface: &Surface) {
        if !self.quantity.open {
            return;
        }
        // F1 im Mengenfenster: Karte im Hauptfenster mit „Mengen und Kosten“
        // bzw. „Kosten“
        if let Event::Key {
            key: help::KEY_F1,
            down,
            ..
        } = e
        {
            if down && self.save_dlg.is_none() {
                let was = self.help.is_open();
                // im Reiter Kosten das Thema „Kosten“, in AVA „AVA“
                self.help.open_with(match self.quantity.blatt() {
                    cards::Blatt::Kosten => help::Topic::Costs,
                    cards::Blatt::Ava => help::Topic::Ava,
                    cards::Blatt::Mengen => help::Topic::Quantities,
                });
                if !was {
                    self.help_toggled();
                }
            }
            return;
        }
        let Some(out) = self
            .quantity
            .handle(&e, &self.theme, &self.ui.fonts, &mut self.picking)
        else {
            return;
        };
        // Löschen, Menü und Verweise ändern Modell und Auswahl: das
        // Hauptfenster gleicht sich an wie nach eigenen Ereignissen
        let model = matches!(
            out,
            quantity::Out::Delete
                | quantity::Out::Action(..)
                | quantity::Out::Link(_)
                | quantity::Out::Kosten(_)
        );
        match out {
            quantity::Out::Picking { selection } => self.list_picked(selection),
            quantity::Out::Zoom(ids) => self.zoom_to(&ids),
            quantity::Out::SaveCsv => self.save_csv(surface),
            quantity::Out::SavePdf => self.save_pdf(surface),
            quantity::Out::BlattWahl(w) => self.settings.set_lv_blatt(w),
            quantity::Out::Command(c) => surface.quantity_command(c),
            quantity::Out::Close => self.close_quantity(surface),
            quantity::Out::Delete => self.erase(true),
            quantity::Out::OpenContext { x, y } => {
                let (t, f) = (&self.theme, &self.ui.fonts);
                if self
                    .quantity
                    .open_context(&self.scene, &mut self.picking, x, y, t, f)
                {
                    self.list_picked(true);
                }
            }
            quantity::Out::Action(a, target, ids) => self.context_action(a, target, Some(ids)),
            quantity::Out::Link(l) => self.follow_link(l),
            quantity::Out::Kosten(w) => self.kosten_schreiben(w),
            quantity::Out::Verwaltung(g) => {
                self.open_verwaltung(Some(verwaltung::Knoten::Leistung(g)));
                if self.verwaltung.is_some() {
                    surface.command(WindowCommand::Activate);
                }
            }
            quantity::Out::Kennwort => {
                self.kennwort_abfragen();
                if self.verwaltung.is_some() {
                    surface.command(WindowCommand::Activate);
                }
            }
            quantity::Out::Ablauf(g) => {
                self.ablauf_im_haus(g);
                if self.verwaltung.is_some() {
                    surface.command(WindowCommand::Activate);
                }
            }
            // AVA-Kopf (Paket PD-3): die Maske im Hauptfenster
            quantity::Out::Projektdaten(feld) => {
                self.open_projektdaten(false);
                if let Some(m) = self.projektdaten.as_mut() {
                    m.fokus_auf(feld);
                    surface.command(WindowCommand::Activate);
                }
            }
        }
        if model {
            self.sync_ui();
            self.sync_props();
            self.sync_levels();
            self.sync_caption(surface);
        }
    }

    /// Preisblatt (KA-2c, paket-ka2 §4): „Nur dieses Haus“ und „Firmenpreis
    /// zurückholen“ als ein Rückgängig-Schritt über `Scene::kosten_folge`;
    /// „Auch für neue Häuser“ über `Scene::fuer_firma` (Firma zuerst, dann
    /// der Projektschritt). Was nicht geht, sagt die Statuszeile; dann hat
    /// sich nichts geändert.
    fn kosten_schreiben(&mut self, w: kosten_view::Schreiben) {
        let h = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Manual);
        // Mit Kennwort, hier nicht eingegeben: der Firma nur vorschlagen
        let vorschlag = self.rolle() == sk_cost::Rolle::Nutzer;
        let meldung = match w {
            kosten_view::Schreiben::Preis {
                ops,
                gilt: preis_blatt::Gilt::NeueHaeuser,
                ..
            } if vorschlag => self.vorschlagen_melden("Preis der Firma vorgeschlagen", &h, &ops),
            kosten_view::Schreiben::Preis {
                ops,
                gilt: preis_blatt::Gilt::NeueHaeuser,
                text,
            } => {
                let label = self.scene.bezeichnung(text);
                self.fuer_firma_melden(label, &h, &ops)
            }
            kosten_view::Schreiben::Preis { ops, .. } => {
                self.kosten_folge("Preis im Projekt geändert", &h, &ops)
            }
            kosten_view::Schreiben::Zurueck(saetze) => {
                let op = sk_cost::Op::AbweichungZuruecknehmen { saetze };
                self.kosten_folge("Firmenpreis zurückgeholt", &h, &[op])
            }
            kosten_view::Schreiben::Uebernehmen(saetze) => {
                let op = sk_cost::Op::StandUebernehmen { saetze };
                self.kosten_folge("Werte für neue Häuser übernommen", &h, &[op])
            }
            kosten_view::Schreiben::Bauleistung { op, hinweis } => self
                .kosten_folge("Bauleistung gewählt", &h, &[*op])
                .or(hinweis.map(|m| (m, false))),
            kosten_view::Schreiben::Lohn { wert, gilt } => {
                let label = self
                    .scene
                    .bezeichnung(lohn_blatt::LohnBlatt::bezeichnung(wert, gilt));
                let op = sk_cost::Op::FirmenwertSetzen {
                    schluessel: "wage".into(),
                    wert,
                };
                match gilt {
                    preis_blatt::Gilt::NurHaus => self.kosten_folge(label, &h, &[op]),
                    preis_blatt::Gilt::NeueHaeuser if vorschlag => {
                        self.vorschlagen_melden("Lohn der Firma vorgeschlagen", &h, &[op])
                    }
                    preis_blatt::Gilt::NeueHaeuser => self.fuer_firma_melden(label, &h, &[op]),
                }
            }
            kosten_view::Schreiben::Gliederung(untertitel) => {
                let label = if untertitel {
                    "LV nach Geschossen gegliedert"
                } else {
                    "LV ohne Geschosse gegliedert"
                };
                let op = sk_cost::Op::LvGliederungSetzen { untertitel };
                self.kosten_folge(label, &h, &[op])
            }
            kosten_view::Schreiben::Lassen(stand) => {
                let op = sk_cost::Op::AbgleichLassen { stand };
                self.kosten_folge("Werte für neue Häuser nicht übernommen", &h, &[op])
            }
        };
        if let Some((text, error)) = meldung {
            self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
            self.notice = Some(Notice {
                text,
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: NOTICE_TIME,
                catalog: false,
                error,
            });
        }
        self.quantity.dirty = true;
        self.redraw = true;
    }

    /// „Auch für neue Häuser“ über `Scene::fuer_firma_auch_hier`; der Hinweis oder
    /// der Grund, warum nichts geändert wurde.
    fn fuer_firma_melden(
        &mut self,
        label: &'static str,
        h: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Option<(meldung::Meldung, bool)> {
        match self.company.as_mut() {
            Some(c) => match self.scene.fuer_firma_auch_hier(label, c, h, ops) {
                Ok(hinweis) => {
                    let wert = label.strip_suffix(preis_blatt::FUER_NEUE).unwrap_or(label);
                    self.doc.fuer_neue_merken(c.zuletzt_geaendert(), wert);
                    hinweis.map(|m| (m, false))
                }
                Err(e) => Some((e.dazu(NICHTS_GEAENDERT), true)),
            },
            None => Some((
                meldung::Meldung::satz("Kein Firmenkatalog geladen.").dazu(NICHTS_GEAENDERT),
                true,
            )),
        }
    }

    /// „Der Firma vorschlagen“ über `Scene::der_firma_vorschlagen` (KA-3b4).
    fn vorschlagen_melden(
        &mut self,
        label: &'static str,
        h: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Option<(meldung::Meldung, bool)> {
        let name = self.doc.name();
        match self.company.as_mut() {
            Some(c) => match self.scene.der_firma_vorschlagen(label, c, h, ops, &name) {
                Ok(m) => Some((m, false)),
                Err(e) => Some((e.dazu(NICHTS_GEAENDERT), true)),
            },
            None => Some((
                meldung::Meldung::satz("Kein Firmenkatalog geladen.").dazu(NICHTS_GEAENDERT),
                true,
            )),
        }
    }

    /// Projektschritt über `Scene::kosten_folge`; der Befundsatz, wenn der
    /// Plan ablehnt.
    fn kosten_folge(
        &mut self,
        label: &'static str,
        h: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Option<(meldung::Meldung, bool)> {
        let firma = self.company.as_ref().map(|c| c.library());
        let b = self.scene.kosten_folge(label, firma, h, ops).err()?;
        Some((meldung::Meldung::aus_befunden(&b, "Nichts geändert."), true))
    }

    /// Hover oder Auswahl kamen aus der Liste: Hauptfenster angleichen.
    fn list_picked(&mut self, selection: bool) {
        self.hover_from_list = true;
        self.redraw = true;
        if selection {
            if self
                .picking
                .primary()
                .is_some_and(|e| self.scene.follow_selection(e))
            {
                if self.ui.view == ViewKind::Plan {
                    self.upload_model();
                }
                self.sync_levels();
            }
            self.sync_props();
        }
    }

    /// Tabelle des Mengenblatts im Umfang des Blatts, die Kopfzeile zuerst
    /// (KA-1).
    fn quantity_csv(&mut self, by: schedule_view::Grouping) -> Vec<u8> {
        let u = self
            .quantity
            .list
            .as_ref()
            .map_or_else(sk_model::qto::Umfang::projekt, |l| l.leiste.umfang.clone());
        let sched = self.scene.schedule_in(&u);
        let kopf = umfang_view::umfang_text(self.scene.model(), &u, sk_platform::local_date_time());
        schedule_view::csv_mit_kopf(self.scene.model(), &sched, by, &kopf)
    }

    /// „Als Tabelle speichern“: Windows-Dialog, .csv für Excel.
    fn save_csv(&mut self, surface: &Surface) {
        let stem = self
            .doc
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("Unbenannt".to_string(), |s| {
                s.to_string_lossy().into_owned()
            });
        let blatt = self.quantity.blatt();
        let kosten = blatt == cards::Blatt::Kosten;
        let ava = self
            .quantity
            .ava
            .as_ref()
            .filter(|_| blatt == cards::Blatt::Ava);
        let suggested = match ava.and_then(|a| a.los_name()) {
            // „haus LV Rohbau.csv“ (paket-ka4 §4)
            Some(los) => format!("{stem} LV {los}.csv"),
            None if kosten => format!("{stem} Kosten.csv"),
            None => format!("{stem} Mengenermittlung.csv"),
        };
        let filters = [
            ("Tabelle für Excel (*.csv)", "*.csv"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("Als Tabelle speichern", &filters, "csv", &suggested)
        else {
            return;
        };
        let ava = self
            .quantity
            .ava
            .as_ref()
            .filter(|_| blatt == cards::Blatt::Ava);
        let bytes = match self.quantity.kosten.as_ref().filter(|_| kosten) {
            // Blatt AVA: das LV des gewählten Loses (ka-4-fach §3.6)
            _ if ava.is_some() => ava.map(|a| a.csv()).unwrap_or_default(),
            // Reiter Kosten: Kosten-CSV wie die Anzeige (ka-2-fach §2.5)
            Some(k) => {
                let (y, mo, d, h, mi) = sk_platform::local_date_time();
                k.csv(&stem, &format!("{d:02}.{mo:02}.{y}, {h:02}:{mi:02}"))
            }
            None => {
                let by = self.quantity.grouping;
                self.quantity_csv(by)
            }
        };
        if let Err(e) = document::tabelle_schreiben(&path, &bytes) {
            let m = meldung::Meldung::aus_io(
                "Tabelle nicht gespeichert",
                "Tabelle speichern",
                &path,
                &e,
            );
            surface.message(&m, true);
        }
    }

    /// „LV Rohbau als PDF speichern“ aus der AVA-Druckvorschau: genau die
    /// gezeigten Seiten (kosten/lv-blatt-a4.md §9).
    fn save_pdf(&mut self, surface: &Surface) {
        let Some((name, bytes)) = self
            .quantity
            .ava
            .as_ref()
            .and_then(|a| a.pdf(&self.ui.fonts))
        else {
            return;
        };
        let filters = [
            ("PDF-Dokument (*.pdf)", "*.pdf"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("Als PDF speichern", &filters, "pdf", &name) else {
            return;
        };
        if let Err(e) = document::tabelle_schreiben(&path, &bytes) {
            let m = meldung::Meldung::aus_io("PDF nicht gespeichert", "PDF speichern", &path, &e);
            surface.message(&m, true);
        }
    }

    /// Doppelklick in der Liste: die aktive Ansicht holt die Bauteile weich
    /// ins Bild; im Grundriss und Schnitt wechselt dabei das Geschoss.
    fn zoom_to(&mut self, ids: &[sk_model::ElementId]) {
        let Some(&first) = ids.first() else { return };
        if self.scene.follow_selection(first) {
            if self.ui.view == ViewKind::Plan {
                self.upload_model();
            }
            self.sync_levels();
        }
        let plane = self.plane();
        let mut lo = vec3(f64::MAX, f64::MAX, f64::MAX);
        let mut hi = vec3(f64::MIN, f64::MIN, f64::MIN);
        for &id in ids {
            for h in selection::helpers(&self.scene, id, ViewKind::Persp, plane, 1.0, &self.theme) {
                for p in [h.a, h.b] {
                    let p = vec3(p[0] as f64, p[1] as f64, p[2] as f64);
                    lo = vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                    hi = vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
                }
            }
        }
        if lo.x > hi.x {
            return;
        }
        let (vw, vh, _) = self.view_size();
        let to = zoom_camera(&self.cam, lo, hi, vw, vh);
        let ms = self.theme.size.anim_ms;
        if ms <= 0.0 {
            self.cam = to;
        } else {
            self.fly = Some(Fly {
                from: self.cam.clone(),
                to,
                start: Instant::now(),
                ms: ms as f64 * 1.5,
            });
        }
        self.redraw = true;
    }

    /// Mengenfenster an Modell, Auswahl und Datei angleichen und zeigen.
    fn sync_quantity(&mut self, surface: &Surface) {
        if !self.quantity.open {
            return;
        }
        let anim = self.theme.size.anim_ms > 0.0;
        self.quantity.set_docked(surface.layout().docked(), anim);
        // Hinweiskarte mit dem Lohnfeld einmal je Arbeitsplatz beim ersten
        // Öffnen der Kosten (paket-ka2 §4, gemerkt wie die übrigen Hinweise)
        if self.quantity.blatt() == cards::Blatt::Kosten
            && self.hints_seen.insert("kosten_lohn".into())
        {
            self.quantity
                .kosten
                .get_or_insert_with(kosten_view::KostenView::new)
                .lohn_karte();
        }
        let vorschlag = self.rolle() == sk_cost::Rolle::Nutzer;
        let entwurf = !vorschlag
            && self
                .company
                .as_ref()
                .is_some_and(|c| sk_cost::verwaltung::hat_kennwort(c.library()));
        if let Some(k) = self.quantity.kosten.as_mut() {
            k.set_vorschlag(vorschlag, entwurf);
            if let Some(c) = &self.company {
                k.set_ablaeufe(c.haus_ablaeufe());
            }
        }
        let firma = self.company.as_ref().map(|c| (c.library(), c.stand()));
        self.quantity.datei = self.doc.name();
        self.quantity
            .sync_mit(&mut self.scene, &self.picking, firma, anim);
        let now = Instant::now();
        self.quantity_busy = self.quantity.tick(&self.theme, now);
        let caption = windows::blatt_caption(
            self.quantity.blatt(),
            &self.doc,
            self.scene.shown_revision(),
        );
        if caption != self.quantity.title.caption {
            surface.set_quantity_title(&caption);
            self.quantity.title.caption = caption;
            self.quantity.dirty = true;
        }
        let (w, h, area) = (
            self.quantity.w,
            self.quantity.h,
            self.quantity.caption_area(),
        );
        if let Some((px, rows)) = self.quantity.frame(&self.theme, &self.ui.fonts, now) {
            if rows.is_none() {
                surface.set_quantity_caption_area(area);
            }
            surface.present_quantity(w, h, px, rows);
        }
    }

    /// Hover im Modell: die Liste zeigt die Zeile (nur bei offenem Mengenfenster).
    fn model_hover(&mut self, hit: Option<sk_model::ElementId>) {
        if !self.quantity.open && self.tree.collapsed {
            return;
        }
        if std::mem::take(&mut self.hover_from_list) {
            self.redraw = true;
        }
        // Die Liste zeichnet beim Abgleich nur die betroffenen Zeilen neu
        self.picking.set_hover(hit, Vec::new());
    }

    fn click(&mut self, id: Id) {
        match id {
            Id::Building | Id::Interior => {
                let cat = match id {
                    Id::Interior => Category::InteriorWall,
                    _ => Category::ExteriorWall,
                };
                // Derselbe Knopf schaltet aus, der andere wechselt die Wandart
                let on = !(self.tool.enabled && self.tool.category == cat);
                if on && id == Id::Building && self.ui.upper_active {
                    return;
                }
                // Im Fundament gibt es noch nichts zu zeichnen (E18)
                if on && self.ui.foundation_active {
                    return;
                }
                if on && id == Id::Building && self.scene.model().buildings().is_empty() {
                    self.open_building_dialog();
                    return;
                }
                if on && !self.tool_allowed() {
                    self.set_view(ViewKind::Persp);
                }
                if on {
                    self.set_wall_kind(cat);
                    self.nord.set_aktiv(false);
                }
                self.tool.set_enabled(on);
            }
            Id::Ref(r) => self.tool.ref_side = r,
            Id::Ortho => self.tool.ortho = !self.tool.ortho,
            Id::View(v) => self.set_view(v),
            Id::Quantity => self.quantity_wanted = true,
            Id::Projektdaten => self.open_projektdaten(false),
            // Mit Nordrichtung schaltet die Kachel den Sonnenstand (§8 09:25)
            Id::Nord if !self.nord.aktiv && self.scene.model().location().north.is_some() => {
                let on = self.sonne_an().is_none();
                self.sonne_schalten(on, true);
            }
            Id::Nord => {
                let on = !self.nord.aktiv;
                if on {
                    self.tool.set_enabled(false);
                    if !self.tool_allowed() {
                        self.set_view(ViewKind::Persp);
                    }
                }
                self.nord.set_aktiv(on);
                self.redraw = true;
            }
            // Im Grundriss derselbe Wechsel wie am Geschossbogen (E18)
            Id::Storey(st) if self.ui.view == ViewKind::Plan => {
                let t = self.now();
                self.wheel.select(&mut self.scene, st, t);
            }
            Id::Storey(st) => {
                if self.scene.set_active_storey(st) {
                    // Nur der Grundriss hängt vom aktiven Geschoss ab
                    if self.ui.view == ViewKind::Plan {
                        self.upload_model();
                    }
                    self.sync_levels();
                }
            }
            Id::ToolType | Id::PropsType => self.open_type_menu(id),
            Id::PropsLink => {
                if let Some(w) = self.sel.id.and_then(|id| self.scene.stack_wall(id)) {
                    self.toggle_link(w);
                }
            }
            Id::PropsMore => {
                self.ui.toggle_more();
                self.overlay_dirty = true;
            }
            Id::PropsMaterial(g) => self.open_materials(Some(g)),
            Id::PropsFlush => {
                if let Some(w) = self.sel.id.and_then(|id| self.scene.stack_wall(id)) {
                    self.flush(w);
                }
            }
            Id::DialogStart => self.close_building_dialog(true),
            Id::DialogCancel | Id::DialogClose => self.close_building_dialog(false),
            // Zahlenfelder melden sich über `UiOut::submit`, Griffe über
            // `UiOut::level`; der Zähler ist derzeit fest
            Id::Field(_) | Id::Grip(_) | Id::DialogMinus | Id::DialogPlus => {}
        }
        self.overlay_dirty = true;
        self.refresh_cursor();
    }

    /// Ergebnis der Oberfläche übernehmen: geänderte Knöpfe und Felder neu
    /// zeichnen, eine gültige Feldeingabe als Schritt ins Modell.
    fn apply_ui(&mut self, out: &ui::UiOut) {
        self.dirty_buttons.extend(out.changed.iter().copied());
        if out.relayout {
            self.overlay_dirty = true;
        }
        if let Some((Field::Draft(d), mm)) = out.submit {
            if self.scene.set_building_dialog_value(d.key(), mm) {
                self.sync_levels();
                self.sync_dialog_fields();
                self.upload_model();
            }
        } else if let Some((field, mm)) = out.submit.filter(|s| s.0.is_level()) {
            if self.scene.set_level(field, mm) {
                self.upload_model();
            }
        } else if let (Some((field, mm)), Some(id)) = (out.submit, self.sel.id) {
            if let Some(b) = sk_model::edit_blocked(self.scene.model(), &[id]) {
                self.show_locked(b, Some((id, delete::Act::Field)));
            } else if self.scene.set_field(id, field, mm) {
                self.upload_model();
            }
        }
        // Ebene ziehen: ein Schritt, das Modell folgt in jedem Bild
        match out.level {
            Some(LevelEvent::Begin(_)) => self.scene.begin("Geschoss ziehen"),
            Some(LevelEvent::Move(g, z)) => {
                match g {
                    Grip::FoundationBottom => self.scene.drag_foundation_bottom(z),
                    Grip::Top(id) => self.scene.drag_storey_top(id, z),
                }
                self.upload_model();
            }
            Some(LevelEvent::End) => {
                self.scene.commit();
                self.upload_model();
            }
            None => {}
        }
        self.redraw |= !out.changed.is_empty() || out.relayout;
    }

    /// Wandart des Werkzeugs mit dem voreingestellten Aufbau aus der Bibliothek
    /// (Vorschau beim Zeichnen und Anzeige im Paneel).
    fn set_wall_kind(&mut self, cat: Category) {
        let set = self.tool_type_of(cat);
        self.tool
            .set_category(cat, self.scene.model().wall_layers(set));
        self.ui.wall_layers = layer_rows(self.scene.model(), set);
        self.ui.interior = cat == Category::InteriorWall;
        self.tool_chip_key = None;
        self.sync_tool_chip();
        self.overlay_dirty = true;
    }

    /// Typ, den das Werkzeug für die Wandart zeichnet (K3): der gewählte,
    /// solange es ihn gibt, sonst der Standardtyp.
    fn tool_type_of(&self, cat: Category) -> sk_model::LayerSetId {
        let tc = sk_model::TypeCategory::of(cat).unwrap_or(sk_model::TypeCategory::ExteriorWall);
        let m = self.scene.model();
        self.tool_type
            .get(tc as usize)
            .copied()
            .flatten()
            .filter(|id| m.layer_set(*id).is_some_and(|t| t.category == tc))
            .unwrap_or_else(|| m.default_type(tc))
    }

    /// Chip im Werkzeug an Modell und Farbschema angleichen; ein Typ, der
    /// sich ändert (Katalog, Rückgängig), ändert auch die Vorschau beim
    /// Zeichnen.
    fn sync_tool_chip(&mut self) {
        let zeilen = ui::projekt_zeilen(self.scene.model().project());
        if zeilen != self.ui.projekt_zeilen {
            self.ui.projekt_zeilen = zeilen;
            self.overlay_dirty = true;
        }
        // Kachel des Nordpfeils (Sonnenstand S2), auch nach Strg+Z
        let nord = (
            self.nord.aktiv,
            self.scene.model().location().north,
            self.sonne_an().is_some(),
        );
        if (self.ui.nord_aktiv, self.ui.nord, self.ui.sonne_an) != nord {
            (self.ui.nord_aktiv, self.ui.nord, self.ui.sonne_an) = nord;
            self.overlay_dirty = true;
        }
        let key = (self.scene.model().revision(), self.theme.rev);
        if self.tool_chip_key == Some(key) {
            return;
        }
        self.tool_chip_key = Some(key);
        let cat = self.tool.category;
        let set = self.tool_type_of(cat);
        let m = self.scene.model();
        let open = self.ui.tool_chip.as_ref().is_some_and(|c| c.open);
        let chip = m.layer_set(set).map(|t| {
            let mut c = selection::type_chip(m, &self.theme, t);
            c.open = open;
            c
        });
        let layers = m.wall_layers(set);
        if self.tool.layers != layers {
            self.tool.set_category(cat, layers);
            self.redraw = true;
        }
        if chip != self.ui.tool_chip {
            self.ui.tool_chip = chip;
            self.ui.wall_layers = layer_rows(m, set);
            self.overlay_dirty = true;
        }
    }

    /// Öffnet die Typ-Liste am Chip (K3); ein zweiter Klick schließt sie.
    fn open_type_menu(&mut self, chip: Id) {
        let again = self.type_menu.as_ref().is_some_and(|m| m.chip == chip);
        self.close_type_menu(false);
        if again {
            return;
        }
        let top = self.top();
        let anchor = self.ui.button_rect(chip, self.w, top).or_else(|| {
            // Eigenschaften zu niedrig für den Chip: Liste oben am Paneel
            (chip == Id::PropsType && self.ui.has_props()).then(|| {
                let p = self.ui.rect(Panel::Props, self.w, top);
                sk_ui::widgets::Rect::new(p.x, p.y, p.w, 0.0)
            })
        });
        let Some(anchor) = anchor else {
            return;
        };
        let m = self.scene.model();
        let (panel, cat, current, runs, blocked) = match chip {
            Id::ToolType => {
                let cat = self.tool.category;
                let tc =
                    sk_model::TypeCategory::of(cat).unwrap_or(sk_model::TypeCategory::ExteriorWall);
                (
                    Panel::Tools,
                    tc,
                    Some(self.tool_type_of(cat)),
                    Vec::new(),
                    None,
                )
            }
            _ => {
                // Alle gewählten Wände derselben Art wechseln mit
                let Some(e) = self.sel.id.and_then(|id| m.element(id)) else {
                    return;
                };
                let Some(tc) = sk_model::TypeCategory::of(e.category) else {
                    return;
                };
                let mut runs = Vec::new();
                for id in &self.picking.selected {
                    let Some(el) = m.element(*id) else { continue };
                    if sk_model::TypeCategory::of(el.category) != Some(tc) {
                        continue;
                    }
                    if let sk_model::ElementKind::Wall(w) = el.kind {
                        if !runs.contains(&w.run) {
                            runs.push(w.run);
                        }
                    }
                }
                // Typwechsel ändert den ganzen Kopplungsstapel (Paket 4 §2.2)
                let blocked = sk_model::edit_blocked(m, &m.type_set(&runs));
                (Panel::Props, tc, e.layer_set, runs, blocked)
            }
        };
        if let Some(b) = blocked {
            self.show_locked(b, self.sel.id.map(|id| (id, delete::Act::Type)));
            return;
        }
        let m = self.scene.model();
        let panel = self.ui.rect(panel, self.w, top);
        let menu = type_menu::TypeMenu::new(
            chip,
            m,
            &self.theme,
            cat,
            current,
            runs,
            anchor,
            panel,
            self.ui.scale,
            (self.w as f32, self.h as f32),
        );
        self.type_menu = Some(menu);
        if self.ui.set_chip_open(chip, true) {
            self.dirty_buttons.push(chip);
        }
        self.type_menu_dirty = true;
        self.tip = None;
    }

    /// Schließt die Typ-Liste: `keep` übernimmt die Vorschau, sonst ist
    /// alles wie vorher.
    fn close_type_menu(&mut self, keep: bool) {
        let Some(menu) = self.type_menu.take() else {
            return;
        };
        if self.scene.previewing_type() {
            self.scene.end_run_type(keep);
            self.upload_model();
            self.props_key = None;
        }
        if self.ui.set_chip_open(menu.chip, false) {
            self.dirty_buttons.push(menu.chip);
        }
        self.type_menu_dirty = true;
    }

    /// Wählt den Eintrag `i` der Typ-Liste: im Werkzeug der Typ für die
    /// nächsten Wände, in den Eigenschaften ein Rückgängig-Schritt.
    fn choose_type(&mut self, i: usize) {
        let Some(menu) = self.type_menu.as_ref() else {
            return;
        };
        let Some(id) = menu.items.get(i).map(|it| it.id) else {
            return;
        };
        if menu.chip == Id::ToolType {
            let tc = menu.category;
            if let Some(slot) = self.tool_type.get_mut(tc as usize) {
                *slot = Some(id);
            }
            self.close_type_menu(false);
            self.tool_chip_key = None;
            self.sync_tool_chip();
            return;
        }
        if menu.current != Some(id) {
            let runs = menu.runs.clone();
            self.scene.preview_run_type(&runs, id);
            self.close_type_menu(true);
        } else {
            self.close_type_menu(false);
        }
        self.sect.ensure(&self.scene);
    }

    /// Vorschau des überfahrenen Typs in den Eigenschaften (wächst nach
    /// innen); ohne Eintrag unter der Maus wieder der alte Stand.
    fn preview_type(&mut self, hover: Option<usize>) {
        let Some(menu) = self.type_menu.as_ref() else {
            return;
        };
        if menu.chip != Id::PropsType {
            return;
        }
        let id = hover.and_then(|i| menu.items.get(i)).map(|it| it.id);
        match id {
            Some(id) if Some(id) != menu.current => {
                let runs = menu.runs.clone();
                self.scene.preview_run_type(&runs, id);
            }
            _ => self.scene.end_run_type(false),
        }
        self.upload_model();
    }

    /// Offene Typ-Liste: nimmt Maus und Tasten. Ein Klick daneben schließt
    /// sie und bewirkt sonst nichts (wie das Dateimenü).
    fn handle_type_menu(&mut self, e: Event, surface: &Surface) -> bool {
        let Some(menu) = self.type_menu.as_mut() else {
            return false;
        };
        match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let hit = menu.hit(x, y);
                let hover = match hit {
                    type_menu::Hit::Item(i) => Some(i),
                    _ => None,
                };
                let link = hit == type_menu::Hit::Catalog;
                if (hover, link) != (menu.hover, menu.link_hover) {
                    let changed_item = hover != menu.hover;
                    (menu.hover, menu.link_hover) = (hover, link);
                    self.type_menu_dirty = true;
                    if changed_item {
                        self.preview_type(hover);
                    }
                }
                true
            }
            Event::MouseDown { x, y, .. } => {
                if menu.hit(x, y) == type_menu::Hit::Outside {
                    // Ein Klick daneben schließt nur, auch auf dem eigenen Chip
                    self.close_type_menu(false);
                }
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                match menu.hit(x, y) {
                    type_menu::Hit::Item(i) => self.choose_type(i),
                    type_menu::Hit::Catalog => {
                        self.close_type_menu(false);
                        self.run_command(Command::Catalog, surface);
                    }
                    _ => {}
                }
                true
            }
            Event::MouseUp { .. } | Event::Wheel { .. } => true,
            Event::Key { key, down, .. } => {
                if down {
                    match key {
                        Key::Escape => self.close_type_menu(false),
                        Key::Other(0x26 | 0x28) => {
                            let i = menu.step(key == Key::Other(0x28));
                            self.type_menu_dirty = true;
                            self.preview_type(i);
                        }
                        Key::Enter => match menu.hover {
                            Some(i) => self.choose_type(i),
                            None => self.close_type_menu(false),
                        },
                        _ => {}
                    }
                }
                true
            }
            Event::MouseLeave => {
                if menu.hover.take().is_some() || std::mem::take(&mut menu.link_hover) {
                    self.type_menu_dirty = true;
                    self.preview_type(None);
                }
                false
            }
            Event::Focus(false) => {
                self.close_type_menu(false);
                false
            }
            _ => false,
        }
    }

    /// Typ-Liste zeichnen oder ausblenden.
    fn paint_type_menu(&mut self) {
        self.type_menu_dirty = false;
        self.redraw = true;
        match &self.type_menu {
            Some(m) => {
                let (c, x, y) = m.paint(&self.theme, &self.ui.fonts);
                let px = c.to_premul_rgba8();
                self.renderer.set_overlay(
                    OVERLAY_TYPE_MENU,
                    x,
                    y,
                    c.width as u32,
                    c.height as u32,
                    &px,
                );
            }
            None => self
                .renderer
                .set_overlay(OVERLAY_TYPE_MENU, 0, 0, 0, 0, &[]),
        }
    }

    fn commit_wall(&mut self, wall: Option<sk_model::WallChain>) {
        if let Some(wall) = wall {
            // Erstes geschlossenes Gebäude: Hinweis auf F1 (9b), als
            // Hinweiskarte (Darstellung p9 §3.4)
            if self.tool.category == Category::ExteriorWall && wall.closed {
                self.discover_card("help", "Hilfe", ("Hilfe öffnen", delete::Link::Help));
            }
            let set = self.tool_type_of(self.tool.category);
            self.scene
                .add_wall_typed(&wall, self.tool.category, Some(set));
            self.sect.ensure(&self.scene);
            self.upload_model();
            self.refresh_cursor();
        }
    }

    /// Zustand der Knöpfe an Werkzeug und Ansicht angleichen. Ein Gebäude
    /// im Entstehen ohne Außenwand-Werkzeug (anderes Werkzeug, andere
    /// Ansicht) wird verworfen.
    fn sync_ui(&mut self) {
        let drawing = self.tool.enabled && self.tool.category == Category::ExteriorWall;
        if self.scene.building_pending() && !self.ui.dialog && !drawing {
            self.scene.cancel_building();
            self.sync_levels();
            self.upload_model();
        }
        let (b, r, o) = (self.tool.enabled, self.tool.ref_side, self.tool.ortho);
        if (self.ui.building, self.ui.ref_side, self.ui.ortho) != (b, r, o) {
            (self.ui.building, self.ui.ref_side, self.ui.ortho) = (b, r, o);
            self.overlay_dirty = true;
        }
    }

    /// Ein Ereignis; `false` beendet die Schleife. Die Nachfrage „Änderungen
    /// speichern?“ und das offene Dateimenü nehmen Maus und Tasten zuerst.
    fn handle(&mut self, e: Event, surface: &Surface) -> bool {
        self.rolle_angleichen();
        // Ein Klick während der Wände wachsen: sofort Endstand (K3b)
        if matches!(e, Event::MouseDown { .. }) && self.scene.skip_animation() {
            self.upload_model();
        }
        // … ebenso ein Übergang der Sichtbarkeit (Paket 3)
        if matches!(e, Event::MouseDown { .. }) && self.scene.skip_vis_animation() {
            self.upload_model();
        }
        // Hilfekarte (Paket 9): F1, Esc und Klicks in die Karte vor den Fenstern
        if self.handle_help(e) {
            return !self.quit;
        }
        if self.card.is_some() && self.handle_card(e, surface) {
            return !self.quit;
        }
        if self.save_dlg.is_some() && self.handle_save_dialog(e, surface) {
            return !self.quit;
        }
        if self.projektdaten.is_some() && self.handle_projektdaten(e) {
            return !self.quit;
        }
        if self.prefs.is_some() && self.handle_prefs(e, surface) {
            return !self.quit;
        }
        if self.catalog.is_some() && self.handle_catalog(e, surface) {
            return !self.quit;
        }
        if self.materials.is_some() && self.handle_materials(e, surface) {
            return !self.quit;
        }
        if self.verwaltung.is_some() && self.handle_verwaltung(e, surface) {
            return !self.quit;
        }
        if self.menu.is_open() && self.handle_menu(e, surface) {
            return !self.quit;
        }
        if self.confirm.is_some() && self.handle_confirm(e) {
            return !self.quit;
        }
        if self.context.is_some() && self.handle_context(e) {
            return !self.quit;
        }
        if self.hint.is_some() && self.handle_hint(e) {
            if let Some(c) = self.queued_command.take() {
                self.run_command(c, surface);
            }
            return !self.quit;
        }
        if self.pick.is_some() && self.handle_pick(e) {
            return !self.quit;
        }
        // Hinweis in der Statuszeile: erst nach Nachfrage, Fenstern und
        // Dateimenü, die den Klick zuerst bekommen
        if let Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            ..
        } = e
        {
            if self.save_dlg.is_none()
                && !self.menu.is_open()
                && !self.modal()
                && self.click_notice(x, y)
            {
                return true;
            }
        }
        if self.type_menu.is_some() && self.handle_type_menu(e, surface) {
            return !self.quit;
        }
        // Baumpanel (Paket 4) nimmt Maus und Rad über sich
        if self.handle_tree(e, surface) {
            self.sync_props();
            self.sync_caption(surface);
            return !self.quit;
        }
        // Rechtsklick auf ein Bauteil: Kontextmenü (V?-9)
        if let Event::MouseDown {
            button: MouseButton::Right,
            x,
            y,
            ..
        } = e
        {
            if self.open_context(x, y) {
                return true;
            }
        }
        self.handle_inner(e, surface) && !self.quit
    }

    /// Modale Nachfrage: `true`, wenn sie das Ereignis genommen hat.
    fn handle_save_dialog(&mut self, e: Event, surface: &Surface) -> bool {
        let (s, th) = (self.title.scale, self.title.height());
        let Some(d) = self.save_dlg.as_mut() else {
            return false;
        };
        let r = d.rect(s, self.w, self.h, th);
        let answer = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.overlay_dirty |= d.mouse_move(r, s, x, y);
                None
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                d.press(r, s, x, y);
                self.overlay_dirty = true;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.overlay_dirty = true;
                d.release(r, s, x, y)
            }
            Event::Key { key, down, .. } => {
                if !down {
                    return true;
                }
                self.overlay_dirty = true;
                d.key(key)
            }
            Event::MouseDown { .. } | Event::MouseUp { .. } | Event::Wheel { .. } => None,
            // Schließen von außen beendet sofort, vom Nutzer wird schon gefragt
            Event::CloseRequested { ask } => {
                self.quit |= !ask;
                None
            }
            _ => return false,
        };
        if let Some(a) = answer {
            self.answer_save(a, surface);
        }
        true
    }

    /// Maske „Projektdaten“ öffnen (Paket PD-2): bei Datei › Neu mit der
    /// Planung vom letzten Projekt, sonst mit den aktuellen Werten.
    fn open_projektdaten(&mut self, neu: bool) {
        if self.modal() {
            return;
        }
        let planung = if neu { self.settings.planung() } else { None };
        self.projektdaten = Some(
            projektdaten::Maske::new(self.scene.model().project(), neu, planung)
                .mit_ort(self.scene.model().location()),
        );
        self.tip = None;
        self.renderer.set_overlay(OVERLAY_TIP, 0, 0, 0, 0, &[]);
        self.overlay_dirty = true;
    }

    /// Antwort der Maske: bei „Neu“ Anfangswerte ohne Schritt, sonst ein
    /// Schritt „Projektdaten geändert“, nur wenn sich etwas geändert hat.
    fn answer_projektdaten(&mut self, a: projektdaten::Antwort) {
        let Some(m) = self.projektdaten.take() else {
            return;
        };
        self.overlay_dirty = true;
        let projektdaten::Antwort::Uebernehmen(p) = a else {
            return;
        };
        let planung = (p.author.clone(), p.author_addr.clone());
        if m.neu {
            self.scene.projekt_anfang(*p);
            self.scene.lage_anfang(m.ort());
        } else {
            self.scene
                .projektdaten_setzen("Projektdaten geändert", *p, m.ort());
        }
        // Die Planung belegt die Maske beim nächsten „Neu“ vor
        self.settings.set_planung(&planung.0, &planung.1);
        self.sync_tool_chip();
    }

    /// Modale Maske „Projektdaten“: `true`, wenn sie das Ereignis genommen hat.
    fn handle_projektdaten(&mut self, e: Event) -> bool {
        let (s, th) = (self.title.scale, self.title.height());
        let Some(d) = self.projektdaten.as_mut() else {
            return false;
        };
        let r = d.rect(s, self.w, self.h, th);
        let answer = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.overlay_dirty |= d.mouse_move(r, s, x, y);
                None
            }
            // Die Titelleiste bleibt bedienbar (Fensterknöpfe, Ziehen)
            Event::MouseDown { y, .. } | Event::MouseUp { y, .. } if y < th as f64 => {
                return false;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                d.press(r, s, &self.ui.fonts, &self.theme, x, y);
                self.overlay_dirty = true;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.overlay_dirty = true;
                d.release(r, s, x, y)
            }
            Event::Key {
                key, down, mods, ..
            } => {
                if !down {
                    return true;
                }
                self.overlay_dirty = true;
                d.key(key, mods)
            }
            Event::Text(ch) => {
                d.text(ch);
                self.overlay_dirty = true;
                None
            }
            Event::MouseDown { .. } | Event::MouseUp { .. } | Event::Wheel { .. } => None,
            _ => return false,
        };
        if let Some(a) = answer {
            self.answer_projektdaten(a);
        }
        true
    }

    /// Offenes Dateimenü: `true`, wenn es das Ereignis genommen hat. Ein Klick
    /// daneben schließt es und bewirkt sonst nichts.
    fn handle_menu(&mut self, e: Event, surface: &Surface) -> bool {
        let g = menu::Geo::new(&self.theme, self.title.scale, self.title.height() as f32);
        let save = menu::save_enabled(&self.doc, self.scene.model());
        match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.menu_dirty |= self.menu.mouse_move(&g, save, &self.recent, x, y);
                let hover = self.title.button_at(x, y, self.w);
                if hover != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(hover));
                    self.title.hover = hover;
                }
                true
            }
            Event::MouseDown { x, y, .. } => {
                // Der Menüknopf schaltet das Menü wieder zu
                if self.title.button_at(x, y, self.w) == Some(Button::Menu) {
                    self.menu.close();
                } else {
                    self.menu.press(&g, save, &self.recent, x, y);
                }
                self.menu_dirty = true;
                true
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(c) = self.menu.release(&g, save, &self.recent, x, y) {
                    self.run_command(c, surface);
                }
                self.menu_dirty = true;
                true
            }
            Event::MouseUp { .. } | Event::Wheel { .. } => true,
            Event::Key { key, down, .. } => {
                if down {
                    if matches!(key, Key::Alt | Key::Other(0x79)) {
                        self.menu.close();
                    } else if let Some(c) = self.menu.key(key, save, &self.recent) {
                        self.run_command(c, surface);
                    }
                    self.menu_dirty = true;
                }
                true
            }
            Event::Focus(false) => {
                self.menu.close();
                self.menu_dirty = true;
                false
            }
            _ => false,
        }
    }

    fn handle_inner(&mut self, e: Event, surface: &Surface) -> bool {
        if matches!(
            e,
            Event::MouseDown { .. } | Event::Key { down: true, .. } | Event::Wheel { .. }
        ) {
            self.finish_flush();
        }
        self.edit.section = self.plane();
        self.edit.plan_z = self.plan_z();
        let th = self.top() as f64;
        let (vw, vh, sc) = self.view_size();
        // Ereignis in Koordinaten der 3D-Ansicht (unterhalb der Titelleiste)
        let in_view = |e: Event| -> Event {
            match e {
                Event::MouseMove { x, y, mods } => Event::MouseMove { x, y: y - th, mods },
                Event::MouseDown { button, x, y, mods } => Event::MouseDown {
                    button,
                    x,
                    y: y - th,
                    mods,
                },
                Event::MouseUp { button, x, y, mods } => Event::MouseUp {
                    button,
                    x,
                    y: y - th,
                    mods,
                },
                Event::Wheel { delta, x, y, mods } => Event::Wheel {
                    delta,
                    x,
                    y: y - th,
                    mods,
                },
                other => other,
            }
        };
        // Beim Ziehen einer Ebene gehören Maus und Tasten dem Paneel „Geschosse“
        if self.ui.level_dragging().is_some() {
            match e {
                Event::MouseMove { .. } | Event::MouseDown { .. } | Event::MouseUp { .. } => {
                    let out = self.ui.handle(&e, self.w, self.top());
                    self.apply_ui(&out);
                    self.sync_levels();
                    self.sync_props();
                    self.sync_caption(surface);
                    return true;
                }
                Event::Key { key, down, .. } => {
                    if down && key == Key::Escape && self.ui.cancel_level_drag() {
                        self.scene.rollback();
                        self.upload_model();
                        self.overlay_dirty = true;
                        self.sync_levels();
                    }
                    return true;
                }
                _ => {}
            }
        }
        let busy = self.nav.is_dragging() || self.edit.is_dragging() || self.sect.is_dragging();
        let sen = self.sect_enabled();
        let mut camera_moved = false;
        match e {
            Event::CloseRequested { ask } => {
                // Von außen (etwa beim Neustart nach einem neuen Stand) sofort schließen
                if !ask {
                    return false;
                }
                self.run_command(Command::Quit, surface);
            }
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                // Paneele nur neu zeichnen, wenn sich ihre Größe ändert; beim
                // Größeziehen zählt jede Millisekunde
                if self.ui.fit(self.title.scale, width, height) {
                    self.overlay_dirty = true;
                } else {
                    self.layout_dirty = true;
                }
                self.refit_plan();
                self.redraw = true;
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                self.ui.top = self.title.height();
                self.ui.fit(s, self.w, self.h);
                self.refit_plan();
                self.overlay_dirty = true;
                self.redraw = true;
            }
            Event::Maximized(m) => {
                self.overlay_dirty |= self.title.maximized != m;
                self.title.maximized = m;
            }
            Event::Focus(f) => {
                self.overlay_dirty |= self.title.active != f;
                self.title.active = f;
            }
            Event::Redraw => self.redraw = true,
            Event::MouseLeave => {
                self.mouse_at = None;
                if !self.hover_from_list {
                    self.model_hover(None);
                }
                let t = self.now();
                self.redraw |= self.wheel.set_hover(None, t);
                self.dirty_title.extend(self.title.hover.take());
                let out = self.ui.handle(&e, self.w, self.top());
                self.apply_ui(&out);
                self.redraw |= self.tool.handle(&e, &self.cam, vw, vh, sc).redraw;
                let en = self.edit_enabled();
                let out = self
                    .edit
                    .handle(&e, &mut self.scene, &self.cam, vw, vh, sc, en);
                self.redraw |= out.redraw;
                let so = self
                    .sect
                    .handle(&e, &self.scene, &self.cam, vw, vh, sc, sen);
                self.redraw |= so.redraw;
                self.sonne_handle(&e, vw, vh, sc);
            }
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                let hover = if busy {
                    None
                } else {
                    self.title.button_at(x, y, self.w)
                };
                if hover != self.title.hover {
                    self.dirty_title
                        .extend(self.title.hover.into_iter().chain(hover));
                }
                self.title.hover = hover;
                // Geschossbogen (E18) liegt über dem Modell und den Paneelen
                let part = if busy || y < th {
                    None
                } else {
                    self.wheel_hit(x, y)
                };
                let t = self.now();
                // der Knopf „Blickrichtung“ leuchtet unter der Maus
                self.redraw |= self.wheel.set_hover(part, t);
                let over_ui = if busy {
                    false
                } else if part.is_some() {
                    let out = self.ui.handle(&Event::MouseLeave, self.w, self.top());
                    self.apply_ui(&out);
                    true
                } else {
                    let out = self.ui.handle(&e, self.w, self.top());
                    self.apply_ui(&out);
                    out.consumed
                };
                let ev = in_view(e);
                camera_moved |= self.nav.handle(&ev, &mut self.cam, &self.scene, vw, vh, sc);
                let outside = (y < th || over_ui) && !busy;
                // Kettensymbol unter der Maus: Band und Schnittlinie darunter
                // greifen nicht
                let chip = if outside || busy {
                    None
                } else {
                    link_view::hit(&self.chips, x, y, sc)
                };
                if chip != self.chip_hover {
                    self.chip_hover = chip;
                    self.edit.link_hover = chip;
                    self.redraw = true;
                }
                // Sonne und Leiste (S4) vor dem Pfeil: über der Leiste und
                // beim Ziehen der Sonne greift sonst nichts
                let sonne_ev = if outside || chip.is_some() {
                    Event::MouseLeave
                } else {
                    ev
                };
                let outside = self.sonne_handle(&sonne_ev, vw, vh, sc) || outside;
                let nord_ev = if outside || chip.is_some() {
                    Event::MouseLeave
                } else {
                    ev
                };
                self.nord_handle(&nord_ev, vw, vh, sc);
                let sect_ev = if outside || chip.is_some() || self.nord.is_busy() {
                    Event::MouseLeave
                } else {
                    ev
                };
                let so = self
                    .sect
                    .handle(&sect_ev, &self.scene, &self.cam, vw, vh, sc, sen);
                self.redraw |= so.redraw;
                self.cut_changed(&so);
                let edit_ev =
                    if outside || self.sect.is_busy() || chip.is_some() || self.nord.is_busy() {
                        Event::MouseLeave
                    } else {
                        ev
                    };
                let en = self.edit_enabled();
                let out = self
                    .edit
                    .handle(&edit_ev, &mut self.scene, &self.cam, vw, vh, sc, en);
                self.redraw |= out.redraw;
                if out.changed && self.edit.is_dragging() {
                    self.upload_live();
                } else if out.changed {
                    self.upload_model();
                }
                // Über Band oder Schnittlinie zeigt das Wandwerkzeug keinen Fangpunkt
                let tool_ev =
                    if outside || self.edit.is_busy() || self.sect.is_busy() || self.nord.is_busy()
                    {
                        Event::MouseLeave
                    } else {
                        ev
                    };
                self.redraw |= self.tool.handle(&tool_ev, &self.cam, vw, vh, sc).redraw;
                // Bauteil unter der Maus: Mengenliste und Baum zeigen seine
                // Zeile (B7, Paket 4)
                if self.quantity.open || !self.tree.collapsed {
                    let free = !busy
                        && !outside
                        && !self.tool.enabled
                        && !self.edit.is_dragging()
                        && !self.sect.is_dragging()
                        && !self.nav.is_dragging();
                    let hit = match ev {
                        Event::MouseMove { x, y, .. } if free => {
                            let (view, plane) = (self.ui.view, self.plane());
                            selection::pick_at(
                                &mut self.scene,
                                &self.cam,
                                view,
                                plane,
                                x,
                                y,
                                vw,
                                vh,
                            )
                        }
                        _ => None,
                    };
                    self.model_hover(hit);
                }
            }
            // Klick auf den Geschossbogen: Spitze wechselt, Band und Schild
            // nehmen den Klick ohne Wirkung
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } if y >= th && self.wheel_hit(x, y).is_some() => {
                self.shortcuts.cancel_alt();
                self.wheel_press = true;
                let t = self.now();
                let input = self.tool.is_active();
                match self.wheel_hit(x, y) {
                    Some(wheel::Part::Up) => {
                        self.wheel.click_arrow(&mut self.scene, true, input, t)
                    }
                    Some(wheel::Part::Down) => {
                        self.wheel.click_arrow(&mut self.scene, false, input, t)
                    }
                    Some(wheel::Part::Mirror) => self.mirror_cut(),
                    _ => self.wheel.click_band(&mut self.scene, t),
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } if self.wheel_press => self.wheel_press = false,
            Event::MouseDown { button, x, y, .. } => {
                self.shortcuts.cancel_alt();
                if y < th {
                    if button == MouseButton::Left {
                        self.title.pressed = self
                            .title
                            .button_at(x, y, self.w)
                            .filter(|b| !self.title.is_disabled(*b));
                        self.dirty_title.extend(self.title.pressed);
                    }
                } else {
                    let out = self.ui.handle(&e, self.w, self.top());
                    self.apply_ui(&out);
                    // Ein Klick in die Paneele schließt das Feld der
                    // Sonnenstands-Leiste, sonst nähme es weiter jede Taste
                    // (Review 3bx)
                    if out.consumed && self.sonne.eingabe.take().is_some() {
                        self.redraw = true;
                    }
                    if !out.consumed && !self.press_chip(button, x, y, sc) {
                        let ev = in_view(e);
                        camera_moved |=
                            self.nav.handle(&ev, &mut self.cam, &self.scene, vw, vh, sc);
                        if self.ansicht_handle(&ev, sc)
                            || self.sonne_handle(&ev, vw, vh, sc)
                            || self.nord_handle(&ev, vw, vh, sc)
                        {
                            self.sync_ui();
                            self.sync_caption(surface);
                            return true;
                        }
                        let so = self
                            .sect
                            .handle(&ev, &self.scene, &self.cam, vw, vh, sc, sen);
                        self.redraw |= so.redraw;
                        self.cut_changed(&so);
                        let en = self.edit_enabled();
                        let eo = if so.consumed {
                            wall_edit::EditOutcome {
                                consumed: true,
                                ..Default::default()
                            }
                        } else {
                            self.edit
                                .handle(&ev, &mut self.scene, &self.cam, vw, vh, sc, en)
                        };
                        self.redraw |= eo.redraw;
                        if let Some((id, wall)) = eo.locked {
                            self.show_locked(id, Some((wall, delete::Act::Drag)));
                        }
                        if !eo.consumed {
                            // Ohne Wandeingabe wählt ein Klick ein Bauteil (beim Loslassen)
                            if let (Event::MouseDown { x, y, .. }, MouseButton::Left, false) =
                                (ev, button, self.tool.enabled)
                            {
                                self.sel.press(x, y);
                            }
                            let out = self.tool.handle(&ev, &self.cam, vw, vh, sc);
                            self.redraw |= out.redraw;
                            self.commit_wall(out.commit);
                            let en = self.edit_enabled();
                            self.edit.refresh(&self.scene, &self.cam, vw, vh, sc, en);
                        }
                    }
                }
            }
            Event::MouseUp { button, x, y, .. } => {
                if button == MouseButton::Left {
                    if let Some(b) = self.title.pressed.take() {
                        self.dirty_title.push(b);
                        if self.title.button_at(x, y, self.w) == Some(b) {
                            match b {
                                Button::Minimize => surface.command(WindowCommand::Minimize),
                                Button::Maximize => surface.command(WindowCommand::ToggleMaximize),
                                Button::Close => surface.command(WindowCommand::Close),
                                Button::Menu => self.run_command(Command::OpenMenu, surface),
                                Button::Undo => self.run_command(Command::Undo, surface),
                                Button::Redo => self.run_command(Command::Redo, surface),
                                Button::Help => self.run_command(Command::Help, surface),
                            }
                        }
                    }
                }
                let out = self.ui.handle(&e, self.w, self.top());
                self.apply_ui(&out);
                if let Some(id) = out.clicked {
                    self.click(id);
                }
                let ev = in_view(e);
                camera_moved |= self.nav.handle(&ev, &mut self.cam, &self.scene, vw, vh, sc);
                if self.ansicht_handle(&ev, sc)
                    || self.sonne_handle(&ev, vw, vh, sc)
                    || self.nord_handle(&ev, vw, vh, sc)
                {
                    self.sync_ui();
                    self.sync_caption(surface);
                    return true;
                }
                let so = self
                    .sect
                    .handle(&ev, &self.scene, &self.cam, vw, vh, sc, sen);
                self.redraw |= so.redraw;
                self.cut_changed(&so);
                let en = self.edit_enabled();
                let eo = self
                    .edit
                    .handle(&ev, &mut self.scene, &self.cam, vw, vh, sc, en);
                self.redraw |= eo.redraw;
                if let Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    mods,
                } = ev
                {
                    // Band angeklickt (auch ohne Verschieben) oder Klick in die Ansicht
                    let clicked = self.sel.release(x, y, sc);
                    let hit = (eo.clicked.is_none() && clicked).then(|| {
                        let (view, plane) = (self.ui.view, self.plane());
                        selection::pick_at(&mut self.scene, &self.cam, view, plane, x, y, vw, vh)
                    });
                    let change = selection::release_pick(
                        eo.clicked,
                        hit,
                        mods.ctrl,
                        self.tool.enabled,
                        &self.picking.selected,
                    );
                    match change {
                        selection::PickChange::Keep => {}
                        selection::PickChange::Replace(id) => self.select(id),
                        selection::PickChange::Add(id) | selection::PickChange::Remove(id) => {
                            self.picking.click(id, true);
                            self.redraw = true;
                        }
                    }
                }
            }
            // Getippte Zeichen braucht nur das Einstellungsfenster
            Event::Text(_) => {}
            // Mausrad über den Eigenschaften: der Inhalt rollt (Paket 4)
            Event::Wheel { delta, x, y, .. }
                if !self.ui.dialog && self.ui.over_props(x, y, self.w, self.top()) =>
            {
                if self.ui.scroll_props(delta as f32) {
                    self.props_dirty = true;
                }
            }
            // Mausrad über dem Geschossbogen: eine Raste = ein Geschoss
            Event::Wheel { delta, x, y, .. }
                if y >= th && !self.ui.dialog && self.wheel_hit(x, y).is_some() =>
            {
                if self.wheel_acc * delta < 0.0 {
                    self.wheel_acc = 0.0;
                }
                self.wheel_acc += delta;
                let n = self.wheel_acc.trunc();
                self.wheel_acc -= n;
                let t = self.now();
                self.wheel.scroll(&mut self.scene, n as i32, t);
            }
            Event::Wheel { y, .. } => {
                if y >= th && !self.ui.dialog {
                    camera_moved |=
                        self.nav
                            .handle(&in_view(e), &mut self.cam, &self.scene, vw, vh, sc);
                }
            }
            // Esc schließt das Feld „Schatten“ der Ansicht (S7)
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if self.ansicht_schatten.offen => {
                self.ansicht_schatten.offen = false;
                self.redraw = true;
            }
            // Ein Feld der Sonnenstands-Leiste in Eingabe nimmt jede Taste (S4)
            Event::Key {
                key, down, mods, ..
            } if self.sonne.eingabe.is_some() => {
                let mut out = sonne_view::Ausgang::default();
                match self.sonne_an() {
                    Some(sun) if down => {
                        self.sonne.key(key, mods, sun, &mut out);
                    }
                    Some(_) => {}
                    None => self.sonne.reset(),
                }
                if let Some(s) = out.sun {
                    self.scene.set_sun(s);
                }
                self.redraw = true;
            }
            // Ein Zahlenfeld in Eingabe nimmt jede Taste
            Event::Key {
                key, down, mods, ..
            } if self.ui.edit.is_some() => {
                let out = self.ui.key(key, down, mods).unwrap_or_default();
                self.apply_ui(&out);
            }
            // Der Dialog nimmt jede übrige Taste: Enter beginnt, Esc bricht ab,
            // Tab springt ins oberste Feld
            Event::Key { key, down, .. } if self.ui.dialog => match key {
                Key::Enter if down => self.close_building_dialog(true),
                Key::Escape if down => self.close_building_dialog(false),
                Key::Tab if down => {
                    let out = self.ui.focus_field(Field::Draft(Draft::FloorOg));
                    self.apply_ui(&out);
                }
                _ => {}
            },
            // Bild↑/Bild↓ wechseln im Grundriss das Geschoss (E18), im
            // Schnitt den Schnitt
            Event::Key {
                key: key @ (wheel::KEY_PAGE_UP | wheel::KEY_PAGE_DOWN),
                down: true,
                ..
            } if matches!(self.ui.view, ViewKind::Plan | ViewKind::Section) => {
                let t = self.now();
                let (view, input) = (self.ui.view, self.tool.is_active());
                self.wheel.key(&mut self.scene, view, key, input, t);
            }
            Event::Key {
                key, down, mods, ..
            } => {
                let free = !self.tool.is_active() && !self.edit.is_dragging() && !self.nord.zieht();
                let command = self.shortcuts.key(key, down, mods, free);
                let en = self.edit_enabled();
                let eo = self
                    .edit
                    .handle(&e, &mut self.scene, &self.cam, vw, vh, sc, en);
                if eo.changed {
                    self.upload_model();
                }
                self.redraw |= eo.redraw;
                let mut no = nordpfeil::Ausgang::default();
                if eo.consumed {
                    // Esc hat das Ziehen abgebrochen bzw. die Eingabe am Band
                } else if down && self.nord.key(key, mods, &mut no) {
                    // Zahl + Enter beim Aufziehen des Nordpfeils
                    self.redraw = true;
                    self.nord_commit(no.commit);
                } else if down && key == Key::Escape && self.nord.escape() {
                    self.redraw = true;
                } else if self.tool.input().is_some() {
                    // Offene Maßeingabe vor jeder Esc-Kaskade (Paket 8, K2)
                    let out = self.tool.handle(&e, &self.cam, vw, vh, sc);
                    self.redraw |= out.redraw;
                    self.commit_wall(out.commit);
                } else if down && key == Key::Escape && self.scene.isolating().is_some() {
                    // Esc beendet zuerst das Isolieren (Paket 4)
                    let v = sk_model::view::Visibility {
                        isolate: None,
                        ..self.scene.model().visibility().clone()
                    };
                    self.fade_to(v);
                } else if let Some(c) = command {
                    self.run_command(c, surface);
                } else if down && key == Key::Escape && self.scene.building_pending() {
                    // Esc vor dem Schließen des Polygons: das Gebäude entsteht nicht
                    self.tool.set_enabled(false);
                    self.scene.cancel_building();
                    self.sync_levels();
                    self.refresh_cursor();
                } else if down && key == Key::Escape && self.tool.enabled && !self.tool.is_active()
                {
                    // Esc ohne angefangenen Zug beendet die Gebäude-Eingabe
                    self.tool.set_enabled(false);
                    self.refresh_cursor();
                } else if down && key == Key::Escape && !self.tool.enabled && self.sel.id.is_some()
                {
                    self.select(None);
                } else {
                    let out = self.tool.handle(&e, &self.cam, vw, vh, sc);
                    self.redraw |= out.redraw;
                    self.commit_wall(out.commit);
                }
            }
        }
        if camera_moved {
            self.refresh_cursor();
        }
        self.sync_ui();
        self.sync_props();
        self.sync_levels();
        self.sync_caption(surface);
        true
    }

    // --- Hilfe (Paket 9) -------------------------------------------------

    /// Lage der App für die Themenwahl (§1.2).
    fn help_ctx(&self) -> help::HelpCtx {
        use help::{SelKind, Window};
        let window = if self.card.is_some() {
            Some(Window::Backups)
        } else if let Some(p) = &self.prefs {
            Some(if p.pattern_open() {
                Window::Patterns
            } else {
                Window::Settings(p.tab_index())
            })
        } else if self.catalog.is_some() {
            Some(Window::Catalog)
        } else if self.materials.is_some() {
            Some(Window::Materials)
        } else if self.verwaltung.is_some() {
            Some(Window::Verwaltung)
        } else {
            None
        };
        let m = self.scene.model();
        let selection = self.sel.id.and_then(|id| {
            use sk_model::ElementKind as K;
            Some(match m.element(id)?.kind {
                K::Wall(_) if m.stack_offset(id).is_some() => SelKind::UpperWall,
                K::Wall(_) => SelKind::Wall,
                K::GroundSlab(_) | K::StripFooting(_) => SelKind::Foundation,
                K::Floor(_) | K::EdgeStrip { .. } | K::SoffitInsulation { .. } => SelKind::Floor,
                K::RoofTerrace { .. } | K::Coping { .. } => SelKind::Terrace,
            })
        });
        help::HelpCtx {
            window,
            dialog: self.ui.dialog,
            flush_pick: self.pick.is_some(),
            dragging: self.edit.is_dragging() || self.edit.input().is_some(),
            tool: self
                .tool
                .enabled
                .then_some(self.tool.category == Category::InteriorWall),
            isolating: self.scene.isolating().is_some(),
            selection,
            view: self.ui.view,
        }
    }

    /// Etwas Inneres nimmt Esc vor der Karte (Nachtrag H9-1): Ziehen,
    /// Eingabe, Rückfragen, Menüs, Klapplisten in Fenstern.
    fn esc_inner(&self) -> bool {
        self.edit.is_dragging()
            || self.edit.input().is_some()
            || self.tool.input().is_some()
            || self.ui.level_dragging().is_some()
            || self.sect.is_dragging()
            || self.pick.is_some()
            || self.ui.edit.is_some()
            || self.save_dlg.is_some()
            || self.projektdaten.is_some()
            || self.menu.is_open()
            || self.confirm.is_some()
            || self.context.is_some()
            || self.type_menu.is_some()
            || self.prefs.as_ref().is_some_and(|p| p.busy())
            || self.catalog.as_ref().is_some_and(|c| c.busy())
            || self.materials.as_ref().is_some_and(|v| v.busy())
            || self.verwaltung.as_ref().is_some_and(|v| v.busy())
    }

    /// Hilfekarte öffnen bzw. schließen (F1, „?“, Menü „Hilfe“).
    fn show_help(&mut self, open: bool) {
        if open != self.help.is_open() {
            self.help.set_open(open);
            self.help_toggled();
        }
    }

    fn help_toggled(&mut self) {
        let t = self.now();
        self.help_fade = Some((t, self.help.is_open()));
        self.redraw = true;
    }

    /// Stelle der Karte unter `(x, y)` (Fenster-Pixel).
    fn help_hit(&self, x: f64, y: f64) -> Option<help::Hit> {
        let (_, (cx, cy), _) = self.help_img.as_ref()?;
        self.help.hit(x as f32 - cx, y as f32 - cy)
    }

    /// F1, Esc, „?“ und die Maus über der Karte, vor allen Fenstern
    /// (Nachtrag H9-1, H9-2, H9-6). `true`, wenn die Hilfe das Ereignis
    /// genommen hat.
    fn handle_help(&mut self, e: Event) -> bool {
        let dragging = self.edit.is_dragging()
            || self.nav.is_dragging()
            || self.sect.is_dragging()
            || self.ui.level_dragging().is_some();
        // Auch Ziehen in Fenstern (Rollbalken, Farbwähler, Fenster selbst)
        let pressed = dragging || self.help_press_elsewhere;
        match e {
            Event::Key {
                key: help::KEY_F1,
                down,
                ..
            } => {
                // Bei „Änderungen speichern?“ wirkt F1 nicht
                if self.save_dlg.is_some() {
                    return false;
                }
                if down && self.help.key(help::KEY_F1) {
                    self.help_toggled();
                }
                true
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if self.help.is_open() && !self.esc_inner() => {
                self.help.key(Key::Escape);
                self.help_toggled();
                true
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } if self.save_dlg.is_none() && self.title.help_at(x, y) => {
                let open = !self.help.is_open();
                self.show_help(open);
                true
            }
            Event::MouseMove { x, y, .. } => {
                let h = self.help_hit(x, y).filter(|_| !pressed);
                if self.help.set_hover(h) {
                    self.redraw = true;
                }
                if h.is_some() {
                    self.mouse_at = Some((x, y));
                }
                h.is_some()
            }
            Event::MouseDown { button, x, y, .. } => {
                let h = self.help_hit(x, y).filter(|_| !dragging);
                self.help_press_elsewhere = h.is_none();
                let Some(h) = h else {
                    return false;
                };
                if button == MouseButton::Left {
                    let open = self.help.is_open();
                    self.help.click(h);
                    if open != self.help.is_open() {
                        self.help_toggled();
                    }
                }
                true
            }
            Event::MouseUp { x, y, .. } => {
                let elsewhere = std::mem::take(&mut self.help_press_elsewhere);
                !dragging && !elsewhere && self.help_hit(x, y).is_some()
            }
            Event::Wheel { delta, x, y, .. } => {
                if self.help_hit(x, y).is_none() {
                    return false;
                }
                let step = 40.0 * self.ui.scale * (-delta as f32).signum();
                self.help.scroll_by(step);
                true
            }
            _ => false,
        }
    }

    /// Thema dem Tun folgen lassen, Bild der Karte zeichnen bzw. ein-,
    /// aus- und überblenden.
    fn sync_help(&mut self) {
        let t = self.now();
        let ctx = self.help_ctx();
        let nord = self.nord.aktiv
            || self.nord.zieht()
            || self.sonne.is_busy()
            || self.sonne.eingabe.is_some();
        self.help.follow(help::topic_mit_nord(&ctx, nord), t);
        self.help.tick(t);
        // Auch das letzte Bild eines Blendens muss noch gezeichnet werden
        let blending = self.help_fade.is_some() || self.help_swap.is_some();
        let anim = self.theme.size.anim_ms.max(0.0) as u64;
        let progress = |t0: u64| {
            if anim == 0 {
                1.0
            } else {
                (t.saturating_sub(t0) as f32 / anim as f32).min(1.0)
            }
        };
        let open = self.help.is_open();
        let shown = match self.help_fade {
            Some((t0, inn)) => {
                let k = progress(t0);
                if k >= 1.0 {
                    self.help_fade = None;
                }
                if inn {
                    k
                } else {
                    1.0 - k
                }
            }
            None => open as u8 as f32,
        };
        if shown <= 0.0 && !open {
            if self.help_img.take().is_some() {
                self.renderer.set_overlay(OVERLAY_HELP, 0, 0, 0, 0, &[]);
                self.renderer.set_overlay(OVERLAY_HELP_OLD, 0, 0, 0, 0, &[]);
                self.help_px = None;
                self.help_swap = None;
                self.redraw = true;
            }
            return;
        }
        let s = self.ui.scale;
        let key: HelpKey = (
            self.help.key_of_image(),
            s.to_bits(),
            self.theme.rev,
            (self.w, self.h, ctx.window.is_some()),
        );
        if self.help_img.as_ref().map(|i| &i.0) != Some(&key) {
            // Anderes Thema bzw. Liste: das alte Bild blendet aus
            let other = self
                .help_img
                .as_ref()
                .is_some_and(|i| (i.0 .0 .0, i.0 .0 .1) != (key.0 .0, key.0 .1));
            if other && anim > 0 && self.help_fade.is_none() {
                if let Some((px, x, y, w, h)) = &self.help_px {
                    self.renderer
                        .set_overlay(OVERLAY_HELP_OLD, *x, *y, *w, *h, px);
                    self.help_swap = Some(t);
                }
            }
            let (c, margin) = self.help.paint(
                &self.ui.fonts,
                s,
                &self.theme,
                self.h as f32 * 0.6,
                &mut self.help_ground,
            );
            let px = c.to_premul_rgba8();
            let (w, h) = (c.width as u32, c.height as u32);
            let (cw, _) = self.help.size();
            let top = self.top();
            let m = (self.theme.size.panel_margin * s).round();
            // Hauptfenster: links der rechten Spalte, oben bündig mit den
            // Paneelen; über einem Fenster 16 dip vom rechten Rand und 12
            // dip unter der Titelzeile (Darstellung p9 §2, Lage)
            let (x, y) = if ctx.window.is_some() {
                (
                    (self.w as f32 - (16.0 * s).round() - cw).max(m),
                    top as f32 + (12.0 * s).round(),
                )
            } else {
                let views = self.ui.rect(Panel::Views, self.w, top);
                ((views.x - m - cw).max(m), top as f32 + m)
            };
            let (ix, iy) = ((x - margin) as i32, (y - margin) as i32);
            self.renderer.set_overlay(OVERLAY_HELP, ix, iy, w, h, &px);
            self.help_px = Some((px, ix, iy, w, h));
            self.help_img = Some((key, (x, y), margin));
            self.redraw = true;
        }
        let swap = match self.help_swap {
            Some(t0) => {
                let k = progress(t0);
                if k >= 1.0 {
                    self.help_swap = None;
                    self.renderer.set_overlay(OVERLAY_HELP_OLD, 0, 0, 0, 0, &[]);
                }
                k
            }
            None => 1.0,
        };
        if let Some((_, x, y, w, h)) = &self.help_px {
            self.renderer
                .place_overlay(OVERLAY_HELP, *x, *y, *w as i32, *h as i32, shown * swap);
        }
        if self.help_swap.is_some() {
            let (w, h) = self.renderer.overlay_size(OVERLAY_HELP_OLD);
            if let Some((_, x, y, ..)) = &self.help_px {
                self.renderer
                    .place_overlay(OVERLAY_HELP_OLD, *x, *y, w, h, shown * (1.0 - swap));
            }
        }
        if blending {
            self.redraw = true;
        }
    }

    /// Wartezeit für die Karte: Bild für Bild beim Blenden, sonst bis zum
    /// entprellten Themenwechsel; dazu das Ende der Bildzeitmessung.
    fn help_wait(&self) -> Option<std::time::Duration> {
        let blend = (self.help_fade.is_some() || self.help_swap.is_some()).then_some(FRAME);
        let topic = self
            .help
            .wait(self.now())
            .map(std::time::Duration::from_millis);
        let measure = self.frame_measure.as_ref().map(|m| m.wait());
        [blend, topic, measure].into_iter().flatten().min()
    }

    /// „Bildzeit messen (10 s)“: Erfassung wie `--zeiten` beginnen.
    fn start_frame_measure(&mut self) {
        self.frame_measure = Some(frame_time::Measure::new());
        self.status(
            meldung::Meldung::satz("Bildzeit wird 10 s gemessen. Jetzt das Modell langsam drehen."),
            frame_time::SPAN,
        );
    }

    /// Messung vorbei: Zeile in der Statuszeile, Tabelle neben der Datei.
    fn sync_frame_measure(&mut self) {
        if !self.frame_measure.as_ref().is_some_and(|m| m.done()) {
            return;
        }
        let Some(m) = self.frame_measure.take() else {
            return;
        };
        let mut line = frame_time::status_meldung(&m.total, &m.work);
        let (j, mo, t, h, mi) = sk_platform::local_date_time();
        let name = frame_time::file_name(j as u32, mo as u32, t as u32, h as u32, mi as u32);
        let dir = self
            .doc
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(std::path::Path::to_path_buf)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|p| std::path::PathBuf::from(p).join("Documents"))
            })
            .unwrap_or_default();
        let path = dir.join(&name);
        if std::fs::write(&path, &m.table).is_err() {
            line = meldung::Meldung::mit(
                "{} · Tabelle „{}“ ließ sich nicht schreiben.",
                &[&line, &name],
            );
        }
        self.status(line, std::time::Duration::from_secs(30));
    }

    /// Text in der Statuszeile für `time`.
    fn status(&mut self, text: meldung::Meldung, time: std::time::Duration) {
        self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
        self.notice = Some(Notice {
            text,
            since: None,
            rect: (0.0, 0.0, 0.0, 0.0),
            time,
            catalog: false,
            error: false,
        });
        self.redraw = true;
    }

    // --- Löschen (V?-9) ---------------------------------------------------

    /// Ausblenden und Aufleuchten an (`anim_ms` > 0; `fade_ms` bzw.
    /// `flash_ms` 0 schaltet nur das eine aus).
    fn erase_anim(&self) -> bool {
        self.theme.size.anim_ms > 0.0 && self.theme.size.fade_ms > 0.0 && self.w > 0
    }

    /// Entf bzw. „Löschen“: die gewählten Bauteile löschen, soweit sie
    /// löschbar sind, in einem Schritt. Gelöschtes blendet aus, Abgelehntes
    /// leuchtet einmal, der Hinweis am Bauteil sagt, was blieb und warum.
    fn delete_selection(&mut self) {
        self.erase(false);
    }

    /// Löschen aus dem Hauptfenster oder (`list`) aus dem Mengenfenster
    /// (H119): dort steht der Hinweis unter der Zeile, das Modell blendet
    /// aus und leuchtet mit, ohne zweiten Hinweis.
    fn erase(&mut self, list: bool) {
        let ids = self.picking.selected.clone();
        if self.tool.is_active() || self.edit.is_dragging() {
            return;
        }
        if list && !self.quantity.part_selected(&self.picking) {
            let lines = vec![delete::NO_PART.to_string()];
            self.quantity.show_hint(lines, None, Instant::now());
            return;
        }
        if ids.is_empty() {
            return;
        }
        self.close_type_menu(false);
        if self.scene.skip_animation() {
            self.upload_model();
        }
        let m = self.scene.model();
        let fade = self.erase_anim() && ids.iter().any(|id| m.can_delete(*id).is_ok());
        if fade {
            self.renderer.capture_scene();
        }
        let d = self.scene.delete_elements(&ids);
        let m = self.scene.model();
        let (lines, link) = if list {
            (Vec::new(), None)
        } else {
            (delete::hint(m, &d), delete::hint_link(m, &d))
        };
        let anchor = delete::hint_anchor(&d);
        if list {
            let now = Instant::now();
            self.quantity
                .erased(&self.scene, &d, &mut self.picking, now);
        } else if self.quantity.hint.take().is_some() {
            self.quantity.dirty = true;
        }
        if !d.removed.is_empty() {
            if fade {
                self.erase_fade = Some(self.now());
            }
            self.upload_model();
            self.sync_levels();
            self.refresh_cursor();
        } else if fade {
            self.renderer.release_snapshot();
        }
        if self.theme.size.anim_ms > 0.0 && self.theme.size.flash_ms > 0.0 && !anchor.is_empty() {
            self.erase_flash = Some((self.now(), anchor.clone()));
            self.redraw = true;
        }
        self.hint =
            (!lines.is_empty()).then(|| delete::HintCard::new(lines, link, anchor, Instant::now()));
        self.hint_dirty = true;
    }

    /// Bildschirmrechteck (Fenster-Pixel) um die Bauteile in der Ansicht.
    fn screen_bounds(&self, ids: &[sk_model::ElementId]) -> Option<sk_ui::widgets::Rect> {
        let (vw, vh, _) = self.view_size();
        let th = self.top() as f64;
        let plane = self.plane();
        let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
        for &id in ids {
            for h in selection::helpers(&self.scene, id, self.ui.view, plane, 1.0, &self.theme) {
                for p in [h.a, h.b] {
                    let p = vec3(p[0] as f64, p[1] as f64, p[2] as f64);
                    if let Some((x, y)) = self.cam.project(p, vw, vh) {
                        lo = (lo.0.min(x), lo.1.min(y));
                        hi = (hi.0.max(x), hi.1.max(y));
                    }
                }
            }
        }
        // Ganz außerhalb der Ansicht: unten in der Mitte
        let lo = (lo.0.max(0.0), lo.1.max(0.0));
        let hi = (hi.0.min(vw), hi.1.min(vh));
        (lo.0 <= hi.0 && lo.1 <= hi.1).then(|| {
            sk_ui::widgets::Rect::new(
                lo.0 as f32,
                (lo.1 + th) as f32,
                (hi.0 - lo.0) as f32,
                (hi.1 - lo.1) as f32,
            )
        })
    }

    /// Hinweis am Bauteil: Bild zeichnen (beim ersten Mal unter die
    /// Bauteile legen), Deckkraft nachführen, nach der Zeit weg; Ausblenden
    /// und Aufleuchten beenden.
    fn sync_erase(&mut self) {
        let now = self.now();
        // Nach der Ablehnungskarte kommt die Zielkarte wieder
        if self.pick.is_some() && self.hint.is_none() {
            self.pick_card();
        }
        if let Some((t0, _)) = &self.erase_flash {
            if now.saturating_sub(*t0) as f32 >= self.theme.size.flash_ms {
                self.erase_flash = None;
            }
            self.redraw = true;
        }
        if self.w == 0 {
            return;
        }
        let fade = if self.theme.size.anim_ms > 0.0 {
            self.theme.size.fade_ms
        } else {
            0.0
        };
        let s = self.ui.scale;
        if std::mem::take(&mut self.hint_dirty) {
            self.redraw = true;
            let size = self
                .hint
                .as_ref()
                .filter(|h| h.rect.is_none())
                .map(|h| (h.size(&self.theme, &self.ui.fonts, s), h.anchor.clone()));
            if let Some((size, anchor)) = size {
                let bounds = self.screen_bounds(&anchor);
                let win = (self.w as f32, self.h as f32, self.top() as f32);
                let views = self.ui.rect(Panel::Views, self.w, self.top());
                if let Some(h) = self.hint.as_mut() {
                    h.place(size, bounds, win, s);
                    // Entdecken-Karte im Hauptfenster: unten rechts, aber
                    // links der rechten Spalte (nicht über dem Baum)
                    if let (true, Some(r)) = (h.discover, h.rect.as_mut()) {
                        let m = (self.theme.size.panel_margin * s).round();
                        r.x = (views.x - m - r.w).max(8.0 * s).round();
                    }
                }
            }
            match &self.hint {
                Some(h) => {
                    let c = h.paint(&self.theme, &self.ui.fonts, s);
                    let r = h.rect.unwrap_or_default();
                    let m = (self.theme.size.panel_shadow * s).round();
                    self.renderer.set_overlay(
                        OVERLAY_HINT,
                        (r.x - m) as i32,
                        (r.y - m) as i32,
                        c.width as u32,
                        c.height as u32,
                        &c.to_premul_rgba8(),
                    );
                    self.hint_alpha = 1.0;
                }
                None => self.renderer.set_overlay(OVERLAY_HINT, 0, 0, 0, 0, &[]),
            }
        }
        let Some(h) = &self.hint else {
            return;
        };
        match h.alpha(Instant::now(), fade) {
            None => {
                self.hint = None;
                self.renderer.set_overlay(OVERLAY_HINT, 0, 0, 0, 0, &[]);
                self.redraw = true;
            }
            Some(a) if a != self.hint_alpha => {
                let r = h.rect.unwrap_or_default();
                let m = (self.theme.size.panel_shadow * s).round();
                let (w, ht) = self.renderer.overlay_size(OVERLAY_HINT);
                self.renderer.place_overlay(
                    OVERLAY_HINT,
                    (r.x - m) as i32,
                    (r.y - m) as i32,
                    w,
                    ht,
                    a,
                );
                self.hint_alpha = a;
                self.redraw = true;
            }
            Some(_) => {}
        }
    }

    /// Maus am Hinweis: darüber bleibt er stehen; ein Klick auf den Verweis
    /// führt ihn aus, ein Klick auf die Karte bewirkt sonst nichts.
    fn handle_hint(&mut self, e: Event) -> bool {
        let s = self.ui.scale;
        let Some(h) = self.hint.as_mut() else {
            return false;
        };
        match e {
            Event::MouseMove { x, y, .. } => {
                if h.mouse_move(x, y, s, &self.theme, Instant::now()) {
                    self.hint_dirty = true;
                }
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => match h.click(x, y, s, &self.theme) {
                None => false,
                Some(link) => {
                    if let Some(l) = link {
                        self.hint = None;
                        self.hint_dirty = true;
                        self.follow_link(l);
                    }
                    true
                }
            },
            _ => false,
        }
    }

    /// Verweis im Hinweis: Rückgängig, Rückfrage „Gebäude löschen“ oder die
    /// Typ-Liste der Wand.
    fn follow_link(&mut self, l: delete::Link) {
        match l {
            delete::Link::Undo => self.history(false),
            delete::Link::DeleteBuilding(b) => self.open_confirm(b),
            delete::Link::ChangeType(wall) => self.change_type_of(wall),
            delete::Link::Flush(wall) => self.flush(wall),
            delete::Link::Unlock(id) => {
                let s = self.ui.scale;
                let m = self.scene.model();
                self.tree
                    .flash_lock(m, m.lock_source(id), &self.theme, s, Instant::now());
                self.redraw = true;
            }
            delete::Link::Save => {
                let untitled = self.doc.path.is_none();
                self.queued_command = Some(autosave::fail_notice_command(untitled));
            }
            delete::Link::Materials => self.open_materials(None),
            delete::Link::Help => self.show_help(true),
            delete::Link::Dismiss => {}
        }
    }

    /// Typ-Liste im Paneel „Eigenschaften“ für diese Wand öffnen.
    fn change_type_of(&mut self, wall: sk_model::ElementId) {
        if !self.picking.is_selected(wall) {
            self.select(Some(wall));
        }
        self.sync_props();
        // Platz der Eigenschaften schon jetzt, nicht erst im nächsten Bild
        self.sync_tree();
        // Der Chip steht unter Mengen und Feldern; meist ist er hinausgerollt
        if self.ui.reveal_props(Id::PropsType) {
            self.props_dirty = true;
        }
        if self.type_menu.is_none() {
            self.open_type_menu(Id::PropsType);
        }
    }

    /// Rückfrage „Gebäude N löschen?“ zeigen; das Gebäude leuchtet.
    fn open_confirm(&mut self, b: sk_model::BuildingId) {
        self.close_type_menu(false);
        self.context = None;
        self.context_dirty = true;
        self.hint = None;
        self.hint_dirty = true;
        self.confirm = delete::ConfirmCard::new(self.scene.model(), b);
        self.confirm_dirty = true;
        self.tip = None;
        self.redraw = true;
    }

    /// Rückfrage: nimmt Klicks und Tasten; Mausrad und mittlere Taste
    /// bewegen weiter die Kamera.
    fn handle_confirm(&mut self, e: Event) -> bool {
        let (s, top) = (self.ui.scale, self.top());
        let Some(c) = self.confirm.as_mut() else {
            return false;
        };
        let (fonts, t) = (&self.ui.fonts, &self.theme);
        let r = c.rect(fonts, t, s, self.w, self.h, top);
        let answer = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.confirm_dirty |= c.mouse_move(r, fonts, t, s, x, y);
                return !self.nav.is_dragging();
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                c.press(r, fonts, t, s, x, y);
                self.confirm_dirty = true;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.confirm_dirty = true;
                c.release(r, fonts, t, s, x, y)
            }
            Event::MouseDown {
                button: MouseButton::Right,
                ..
            }
            | Event::MouseUp {
                button: MouseButton::Right,
                ..
            } => None,
            Event::Key { key, down, .. } => {
                if !down {
                    return true;
                }
                self.confirm_dirty = true;
                c.key(key)
            }
            _ => return false,
        };
        if let Some(a) = answer {
            self.answer_confirm(a);
        }
        true
    }

    /// Antwort der Rückfrage: „Löschen“ entfernt das Gebäude in einem
    /// Schritt und blendet es aus; sonst bleibt alles.
    fn answer_confirm(&mut self, a: delete::Answer) {
        let Some(c) = self.confirm.take() else {
            return;
        };
        self.confirm_dirty = true;
        self.redraw = true;
        if a != delete::Answer::Delete {
            return;
        }
        let fade = self.erase_anim();
        if fade {
            self.renderer.capture_scene();
        }
        if !self.scene.remove_building(c.building) {
            if fade {
                self.renderer.release_snapshot();
            }
            return;
        }
        if fade {
            self.erase_fade = Some(self.now());
        }
        self.select(None);
        self.upload_model();
        self.sync_levels();
        self.overlay_dirty = true;
        self.refresh_cursor();
    }

    fn paint_confirm(&mut self) {
        self.confirm_dirty = false;
        self.redraw = true;
        let Some(c) = &self.confirm else {
            self.renderer.set_overlay(OVERLAY_CONFIRM, 0, 0, 0, 0, &[]);
            return;
        };
        let s = self.ui.scale;
        let r = c.rect(&self.ui.fonts, &self.theme, s, self.w, self.h, self.top());
        let img = c.paint(&self.theme, &self.ui.fonts, s);
        let m = (self.theme.size.panel_shadow * s).round();
        self.renderer.set_overlay(
            OVERLAY_CONFIRM,
            (r.x - m) as i32,
            (r.y - m) as i32,
            img.width as u32,
            img.height as u32,
            &img.to_premul_rgba8(),
        );
    }

    /// Startkarte nach einem Absturz (F-13) zeigen.
    fn show_start_card(&mut self, f: autosave::Found) {
        let now = std::time::SystemTime::now();
        let card = backup_card::BackupCard::start(f, now, sk_platform::local_date_time());
        self.open_card(card);
    }

    /// „Sicherungen …“ im Dateimenü: die Sicherungen als Liste.
    fn open_backups(&mut self) {
        let Some(dir) = autosave::folder() else {
            return;
        };
        let now = std::time::SystemTime::now();
        let card = backup_card::BackupCard::list(&dir, now, sk_platform::local_date_time());
        self.open_card(card);
    }

    fn open_card(&mut self, card: backup_card::BackupCard) {
        self.close_type_menu(false);
        self.context = None;
        self.context_dirty = true;
        self.tip = None;
        self.renderer.set_overlay(OVERLAY_TIP, 0, 0, 0, 0, &[]);
        self.card = Some(card);
        self.card_fade = None;
        self.card_dirty = true;
        self.redraw = true;
    }

    /// Karte der Sicherungen: nimmt Maus und Tasten; Fensterknöpfe der
    /// Titelleiste und Fensterereignisse gehen durch.
    fn handle_card(&mut self, e: Event, surface: &Surface) -> bool {
        let th = self.top() as f64;
        let (s, top) = (self.ui.scale, self.top());
        let window_button = |a: &App, x: f64, y: f64| {
            y < th
                && matches!(
                    a.title.button_at(x, y, a.w),
                    Some(Button::Minimize | Button::Maximize | Button::Close)
                )
        };
        match e {
            Event::Resized { .. } | Event::ScaleChanged(_) => {
                self.card_dirty = true;
                return false;
            }
            // Schließen: Karte weg, die Nachfrage „Änderungen speichern?“
            // muss erreichbar sein (eine unbeantwortete Startkarte kommt
            // beim nächsten Start wieder)
            Event::CloseRequested { .. } => {
                self.card = None;
                self.card_dirty = true;
                return false;
            }
            Event::MouseMove { x, y, .. } if window_button(self, x, y) => return false,
            Event::MouseDown { x, y, .. } | Event::MouseUp { x, y, .. }
                if window_button(self, x, y) || self.title.pressed.is_some() =>
            {
                return false;
            }
            _ => {}
        }
        let Some(c) = self.card.as_mut() else {
            return false;
        };
        let (fonts, t) = (&self.ui.fonts, &self.theme);
        let r = c.rect(fonts, t, s, self.w, self.h, top);
        let answer = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                if self.title.hover.is_some() {
                    self.dirty_title.extend(self.title.hover.take());
                }
                self.card_dirty |= c.mouse_move(r, fonts, t, s, x, y);
                None
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                c.press(r, fonts, t, s, x, y);
                self.card_dirty = true;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.card_dirty = true;
                c.release(r, fonts, t, s, x, y, Instant::now())
            }
            Event::Key { key, down, .. } => {
                if !down {
                    return true;
                }
                self.card_dirty = true;
                c.key(key)
            }
            Event::MouseDown { .. }
            | Event::MouseUp { .. }
            | Event::Wheel { .. }
            | Event::Text(_)
            | Event::MouseLeave => None,
            _ => return false,
        };
        if let Some(a) = answer {
            self.answer_card(a, surface);
        }
        true
    }

    /// Antwort der Karte; sie blendet aus.
    fn answer_card(&mut self, a: backup_card::Answer, surface: &Surface) {
        let Some(c) = self.card.take() else {
            return;
        };
        // Altes Bild bleibt zum Ausblenden stehen
        self.card_fade = self.erase_anim().then(|| self.now());
        self.card_dirty = true;
        self.redraw = true;
        match a {
            backup_card::Answer::Restore => {
                if let Some(f) = c.found {
                    self.restore_backup(&f.backup, f.original.as_deref(), surface);
                }
            }
            backup_card::Answer::Discard => {
                if let Some(f) = c.found {
                    autosave::answered(&f.backup);
                    // Mit einer Datei gestartet: die bleibt offen
                    let fresh = self.doc.path.is_none() && !self.doc.is_dirty(self.scene.model());
                    if let Some(o) = f.original.filter(|o| fresh && o.is_file()) {
                        self.open_path(surface, o);
                    }
                }
            }
            backup_card::Answer::Open(i) => {
                self.backups = c.entries;
                self.confirm_then(Command::OpenBackup(i), surface);
            }
            backup_card::Answer::Close => {}
        }
        self.sync_caption(surface);
    }

    /// Sicherung öffnen wie „Wiederherstellen“: Stand der Sicherung mit dem
    /// Pfad der gespeicherten Datei, ungespeichert; 5 s ein Hinweis.
    fn restore_backup(
        &mut self,
        backup: &std::path::Path,
        original: Option<&std::path::Path>,
        surface: &Surface,
    ) {
        match autosave::restore(backup, original) {
            Ok((scene, doc)) => {
                autosave::answered(backup);
                self.install_scene(scene);
                self.doc = doc;
                let now = std::time::SystemTime::now();
                let at = std::fs::metadata(backup)
                    .and_then(|m| m.modified())
                    .unwrap_or(now);
                let (.., h, m) = autosave::local_at(at, now, sk_platform::local_date_time());
                let zeit = format!("{h:02}:{m:02}");
                // Gespeicherte Datei nicht lesbar: sie bleibt, wie sie ist
                let name = original
                    .filter(|_| self.doc.path.is_none())
                    .and_then(|o| o.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                let text = match &name {
                    Some(n) => meldung::Meldung::mit(
                        "Sicherung von {} wiederhergestellt. {} ist nicht lesbar und bleibt unverändert; bitte unter neuem Namen speichern.",
                        &[&zeit, n],
                    ),
                    None => meldung::Meldung::mit("Sicherung von {} wiederhergestellt.", &[&zeit]),
                };
                self.notice = Some(Notice {
                    text,
                    since: None,
                    rect: (0.0, 0.0, 0.0, 0.0),
                    time: std::time::Duration::from_secs(if name.is_some() { 15 } else { 5 }),
                    catalog: false,
                    error: false,
                });
                self.sync_levels();
            }
            Err(e) => surface.message(&e, true),
        }
    }

    /// Karte samt Abdunkeln zeichnen bzw. ausblenden.
    fn paint_card(&mut self) {
        self.card_dirty = false;
        self.redraw = true;
        let th = self.top();
        let scrim = menu::scrim_premul(self.theme.env.scrim);
        let h = self.h.saturating_sub(th);
        if let Some(t0) = self.card_fade {
            // Ausblenden über fade_ms; das Bild steht schon
            let ms = self.theme.size.fade_ms.max(1.0);
            let k = 1.0 - (self.now().saturating_sub(t0) as f32 / ms);
            if k <= 0.0 || self.card.is_some() {
                self.card_fade = None;
            } else {
                let (w, hh) = self.renderer.overlay_size(OVERLAY_CARD);
                let (x, y) = self.card_at;
                self.renderer.place_overlay(OVERLAY_CARD, x, y, w, hh, k);
                self.renderer.place_overlay(
                    OVERLAY_CARD_SCRIM,
                    0,
                    th as i32,
                    self.w as i32,
                    h as i32,
                    k,
                );
                return;
            }
        }
        let Some(c) = self.card.as_mut() else {
            self.renderer.set_overlay(OVERLAY_CARD, 0, 0, 0, 0, &[]);
            self.renderer
                .set_overlay(OVERLAY_CARD_SCRIM, 0, 0, 0, 0, &[]);
            return;
        };
        let s = self.ui.scale;
        let r = c.rect(&self.ui.fonts, &self.theme, s, self.w, self.h, th);
        let img = c.paint(&self.theme, &self.ui.fonts, s);
        let px = c.bytes(&img);
        let m = (self.theme.size.panel_shadow * s).round();
        self.card_at = ((r.x - m) as i32, (r.y - m) as i32);
        self.renderer.set_overlay(
            OVERLAY_CARD,
            self.card_at.0,
            self.card_at.1,
            img.width as u32,
            img.height as u32,
            &px,
        );
        c.give_back(img, px);
        self.renderer
            .set_overlay_fill(OVERLAY_CARD_SCRIM, 0, th as i32, self.w, h, scrim);
    }

    /// Rechtsklick in der Ansicht: Bauteil darunter wählen (eine Auswahl,
    /// zu der es gehört, bleibt) und das Kontextmenü öffnen. `false`, wenn
    /// dort kein Bauteil liegt.
    fn open_context(&mut self, x: f64, y: f64) -> bool {
        let top = self.top();
        let th = top as f64;
        if y < th
            || self.ui.dialog
            || self.ui.level_dragging().is_some()
            || self.tool.is_active()
            || self.edit.is_dragging()
            || self.nav.is_dragging()
            || self.ui.over(x, y, self.w, top)
            || self.wheel_hit(x, y).is_some()
        {
            return false;
        }
        let (vw, vh, _) = self.view_size();
        let (view, plane) = (self.ui.view, self.plane());
        let hit = selection::pick_at(&mut self.scene, &self.cam, view, plane, x, y - th, vw, vh);
        let Some(hit) = hit else {
            return false;
        };
        if !self.picking.is_selected(hit) {
            self.select(Some(hit));
        }
        self.close_type_menu(false);
        let s = self.ui.scale;
        self.context = Some(delete::ContextMenu::new(
            self.scene.model(),
            hit,
            &self.picking.selected,
            x,
            y,
            (self.w, self.h, top),
            &self.theme,
            s,
        ));
        self.context_dirty = true;
        self.tip = None;
        self.redraw = true;
        true
    }

    /// Offenes Kontextmenü: nimmt Maus und Tasten; ein Klick daneben
    /// schließt es (ein Rechtsklick öffnet es dort neu).
    fn handle_context(&mut self, e: Event) -> bool {
        let s = self.ui.scale;
        let Some(c) = self.context.as_mut() else {
            return false;
        };
        let t = &self.theme;
        let action = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.context_dirty |= c.mouse_move(t, s, x, y);
                return true;
            }
            Event::MouseDown { button, x, y, .. } => {
                if !c.press(t, s, x, y) {
                    self.context = None;
                    self.context_dirty = true;
                    if button == MouseButton::Right {
                        self.open_context(x, y);
                    }
                }
                return true;
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => c.release(t, s, x, y),
            Event::MouseUp { .. } | Event::Wheel { .. } => return true,
            Event::Key { key, down, .. } => {
                if !down {
                    return true;
                }
                self.context_dirty = true;
                match c.key(key) {
                    Ok(a) => a,
                    Err(()) => {
                        self.context = None;
                        return true;
                    }
                }
            }
            Event::Focus(false) => {
                self.context = None;
                self.context_dirty = true;
                return false;
            }
            _ => return false,
        };
        if let Some(a) = action {
            let target = c.target;
            self.context = None;
            self.context_dirty = true;
            self.context_action(a, target, None);
        }
        true
    }

    /// Befehl aus dem Kontextmenü; `list`: aus dem Mengenfenster, mit den
    /// Bauteilen der Zeile.
    fn context_action(
        &mut self,
        a: delete::Action,
        target: sk_model::ElementId,
        list: Option<Vec<sk_model::ElementId>>,
    ) {
        match a {
            delete::Action::ChangeType => {
                // Randdämmstreifen: der Typ seiner Wand
                let wall = match self.scene.model().can_delete(target) {
                    Err(sk_model::Refusal::Derived { from })
                        if self
                            .scene
                            .model()
                            .element(target)
                            .is_some_and(|e| e.category == Category::EdgeInsulation) =>
                    {
                        from
                    }
                    _ => target,
                };
                self.change_type_of(wall);
            }
            delete::Action::Properties => self.select(Some(target)),
            delete::Action::ShowInModel => {
                if let Some(ids) = list {
                    self.zoom_to(&ids);
                }
            }
            delete::Action::Delete => self.erase(list.is_some()),
            delete::Action::DeleteBuilding => {
                if let Some(b) = self.scene.model().building_of_element(target) {
                    self.open_confirm(b);
                }
            }
        }
    }

    fn paint_context(&mut self) {
        self.context_dirty = false;
        self.redraw = true;
        match &self.context {
            None => self.renderer.set_overlay(OVERLAY_CONTEXT, 0, 0, 0, 0, &[]),
            Some(c) => {
                let (img, x, y) = c.paint(&self.theme, &self.ui.fonts, self.ui.scale);
                self.renderer.set_overlay(
                    OVERLAY_CONTEXT,
                    x,
                    y,
                    img.width as u32,
                    img.height as u32,
                    &img.to_premul_rgba8(),
                );
            }
        }
    }

    fn paint_title(&mut self, surface: &Surface) {
        let c = self
            .title
            .paint(&self.theme, self.ui.fonts.regular.as_ref(), self.w);
        let th = self.title.height();
        self.renderer
            .set_overlay(OVERLAY_TITLE, 0, 0, self.w, th, &c.to_premul_rgba8());
        self.title_img = Some(c);
        self.dirty_title.clear();
        surface.set_caption_area(CaptionArea {
            height: th,
            buttons_width: self.title.buttons_width(),
            left_width: self.title.caption_left(),
        });
    }

    fn paint_overlays(&mut self, surface: &Surface) {
        self.paint_title(surface);
        let th = self.title.height();
        for (slot, p) in [
            (OVERLAY_TOOLS, Panel::Tools),
            (OVERLAY_LEVELS, Panel::Levels),
            (OVERLAY_VIEWS, Panel::Views),
        ] {
            let (c, x, y) = self.ui.paint(&self.theme, p, self.w, th);
            c.premul_rgba8_into(&mut self.panel_px);
            self.renderer
                .set_overlay(slot, x, y, c.width as u32, c.height as u32, &self.panel_px);
        }
        self.dirty_buttons.clear();
        self.paint_props();
        self.paint_dialog();
        self.paint_menu();
        self.paint_save_dialog();
        self.paint_projektdaten();
        self.paint_prefs();
        self.paint_type_menu();
        self.overlay_dirty = false;
        self.layout_dirty = false;
        self.redraw = true;
    }

    /// Dateimenü zeichnen oder ausblenden.
    fn paint_menu(&mut self) {
        self.menu_dirty = false;
        self.redraw = true;
        if !self.menu.is_open() {
            self.renderer.set_overlay(OVERLAY_MENU, 0, 0, 0, 0, &[]);
            return;
        }
        let g = menu::Geo::new(&self.theme, self.title.scale, self.title.height() as f32);
        let save = menu::save_enabled(&self.doc, self.scene.model());
        let (c, x, y) = self
            .menu
            .paint(&self.theme, &self.ui.fonts, &g, save, &self.recent);
        let px = c.to_premul_rgba8();
        self.renderer
            .set_overlay(OVERLAY_MENU, x, y, c.width as u32, c.height as u32, &px);
    }

    /// Nachfrage „Änderungen speichern?“ samt Abdunkeln zeichnen oder ausblenden.
    fn paint_save_dialog(&mut self) {
        let Some(d) = &self.save_dlg else {
            self.renderer.set_overlay(OVERLAY_SAVE, 0, 0, 0, 0, &[]);
            self.renderer
                .set_overlay(OVERLAY_SAVE_SCRIM, 0, 0, 0, 0, &[]);
            return;
        };
        let (s, th) = (self.title.scale, self.title.height());
        let r = d.rect(s, self.w, self.h, th);
        let c = d.paint(&self.theme, &self.ui.fonts, s);
        let m = (self.theme.size.panel_shadow * s).round();
        let px = c.to_premul_rgba8();
        self.renderer.set_overlay(
            OVERLAY_SAVE,
            (r.x - m) as i32,
            (r.y - m) as i32,
            c.width as u32,
            c.height as u32,
            &px,
        );
        let scrim = menu::scrim_premul(self.theme.env.scrim);
        let h = self.h.saturating_sub(th);
        self.renderer
            .set_overlay_fill(OVERLAY_SAVE_SCRIM, 0, th as i32, self.w, h, scrim);
    }

    /// Maske „Projektdaten“ samt Abdunkeln zeichnen oder ausblenden.
    fn paint_projektdaten(&mut self) {
        let Some(d) = &self.projektdaten else {
            self.renderer.set_overlay(OVERLAY_PD, 0, 0, 0, 0, &[]);
            self.renderer.set_overlay(OVERLAY_PD_SCRIM, 0, 0, 0, 0, &[]);
            return;
        };
        let (s, th) = (self.title.scale, self.title.height());
        let r = d.rect(s, self.w, self.h, th);
        let c = d.paint(&self.theme, &self.ui.fonts, s);
        let m = (self.theme.size.panel_shadow * s).round();
        let px = c.to_premul_rgba8();
        self.renderer.set_overlay(
            OVERLAY_PD,
            (r.x - m) as i32,
            (r.y - m) as i32,
            c.width as u32,
            c.height as u32,
            &px,
        );
        let scrim = menu::scrim_premul(self.theme.env.scrim);
        let h = self.h.saturating_sub(th);
        self.renderer
            .set_overlay_fill(OVERLAY_PD_SCRIM, 0, th as i32, self.w, h, scrim);
    }

    /// Knöpfe der Titelleiste an Verlauf und Menü angleichen.
    fn sync_title_state(&mut self) {
        // Bei offenem Einstellungsfenster gesperrt (E5)
        let free = !self.modal();
        let undo = free && self.scene.undo_label().is_some();
        let redo = free && self.scene.redo_label().is_some();
        let open = self.menu.is_open();
        let t = &mut self.title;
        if t.undo_enabled != undo {
            t.undo_enabled = undo;
            self.dirty_title.push(Button::Undo);
        }
        if t.redo_enabled != redo {
            t.redo_enabled = redo;
            self.dirty_title.push(Button::Redo);
        }
        if t.menu_open != open {
            t.menu_open = open;
            self.dirty_title.push(Button::Menu);
        }
    }

    /// Gewünschter Hinweis an der Maus: über dem Fuß einer gekoppelten
    /// OG-Wand (A52).
    fn tip_wanted(&self) -> Option<String> {
        if let Some(p) = &self.prefs {
            return p.tip(&self.scene);
        }
        if let Some(c) = &self.catalog {
            return c.tip();
        }
        if let Some(v) = &self.materials {
            return v.tip();
        }
        if let Some(c) = &self.context {
            return c.tip();
        }
        if self.ui.dialog
            || self.ui.level_dragging().is_some()
            || self.menu.is_open()
            || self.save_dlg.is_some()
            || self.projektdaten.is_some()
            || self.confirm.is_some()
        {
            return None;
        }
        // Rückgängig und Wiederherstellen mit dem Namen des Schritts (E17)
        match self.title.hover {
            Some(Button::Undo) => return menu::history_hint(&self.scene, false),
            Some(Button::Redo) => return menu::history_hint(&self.scene, true),
            Some(Button::Help) => return Some(tip_text(help::tooltip_lines("?"))),
            Some(_) => return None,
            None => {}
        }
        // Beim Ziehen einer gestapelten Wand: ihr Versatz (OG Phase 2)
        if self.edit.pill(&self.scene).is_some() {
            return None;
        }
        if let Some((_, o)) = self.edit.dragged_offset(&self.scene) {
            return Some(offset_label(o));
        }
        // Sonnenstand: ohne Schatten auf diesem Treiber sagt es die Leiste
        if self.renderer.shadow_failed() && (self.sonne.hover.is_some() || self.sonne.ueber_sonne) {
            return Some(SCHATTEN_FEHLT.into());
        }
        // Zahnrad „Schatten“ der Ansicht (S7)
        match self.ansicht_schatten.unter {
            Some(ansicht_schatten::Teil::Zahnrad) => {
                let t = if self.renderer.shadow_failed() {
                    SCHATTEN_FEHLT
                } else {
                    ansicht_schatten::TIP
                };
                return Some(t.into());
            }
            Some(ansicht_schatten::Teil::Licht(sk_model::ShadeLight::Sun))
                if !ansicht_schatten::sonne_waehlbar(self.scene.model().location()) =>
            {
                return Some(ansicht_schatten::OHNE_NORD.into());
            }
            _ => {}
        }
        if let Some(t) = self.button_tip() {
            return Some(t);
        }
        if let Some(t) = self
            .mouse_at
            .and_then(|(x, y)| self.ui.props_tip(x, y, self.w, self.top()))
        {
            return Some(t);
        }
        // Baumpanel: blasses Symbol mit Begründung, sonst die Zeile
        if self.in_tree {
            let m = self.scene.model();
            let faded = self
                .mouse_at
                .and_then(|(x, _)| self.tree.faded_tip(m, x, &self.theme, self.ui.scale));
            return faded.or_else(|| self.tree.tip(m)).filter(|t| !t.is_empty());
        }
        self.chain_tip()
    }

    /// Tooltip an Werkzeug- und Ansichtsknöpfen (9b): Name, Satz aus
    /// `hilfe.txt`, „F1: mehr“; ein gesperrter Knopf nennt statt des Satzes
    /// den Grund.
    fn button_tip(&self) -> Option<String> {
        let id = self.ui.hover?;
        let nord = nordpfeil::tip(self.ui.nord, self.ui.sonne_an);
        let name = match id {
            Id::Building => "Gebäude",
            Id::Interior => "Innenwand",
            Id::Ortho => "90°-Sprung",
            Id::Quantity => cards::KNOPF,
            Id::Projektdaten => "Projektdaten",
            Id::Nord => nord.as_str(),
            Id::View(v) => v.knopf(None),
            _ => return None,
        };
        let reason = match id {
            Id::Building if self.ui.upper_active => Some("Außenwände entstehen aus dem EG."),
            Id::Building | Id::Interior if self.ui.foundation_active => {
                Some("Im Fundament wird nicht gezeichnet.")
            }
            _ => None,
        };
        let mut lines = help::tooltip_lines(name);
        if let (Some(r), true) = (reason, lines.len() == 3) {
            lines[1] = r.into();
        }
        // Mit Nordrichtung heißt die Ansicht nach der Himmelsrichtung, der
        // Satz („Ansicht von vorne.“) bleibt (Sonnenstand S3)
        if let Id::View(v) = id {
            lines[0] = v.titel(self.ui.nord);
        }
        Some(tip_text(lines))
    }

    /// Hinweis am Kettensymbol unter der Maus.
    fn chain_tip(&self) -> Option<String> {
        let wall = self.chip_hover?;
        let (_, linked) = self.scene.model().stack_offset(wall)?;
        Some(link_view::tip(linked).to_string())
    }

    /// Klick auf ein Kettensymbol: löst bzw. koppelt sofort, ohne Rückfrage
    /// (OG Phase 2). Wieder gekoppelt mit Versatz: Hinweis mit „Bündig
    /// setzen“. `true`, wenn der Klick dem Symbol galt.
    fn press_chip(&mut self, button: MouseButton, x: f64, y: f64, sc: f64) -> bool {
        if button != MouseButton::Left || self.tool.is_active() {
            return false;
        }
        let Some(wall) = link_view::hit(&self.chips, x, y, sc) else {
            return false;
        };
        self.toggle_link(wall);
        true
    }

    /// Kette der gestapelten Wand umschalten (Symbol im Plan oder Paneel).
    fn toggle_link(&mut self, wall: sk_model::ElementId) {
        let Some((offset, linked)) = self.scene.model().stack_offset(wall) else {
            return;
        };
        if self.scene.set_linked(wall, !linked) {
            self.upload_model();
            self.sync_props();
            // Neben dem Paar, damit die Karte keine der Wände verdeckt (E20 §6.4)
            let pair = [Some(wall), self.scene.model().wall_below(wall)];
            self.hint = (!linked && offset != 0.0).then(|| {
                let mut h = delete::HintCard::new(
                    relink_lines(offset),
                    Some(("Bündig setzen", delete::Link::Flush(wall))),
                    pair.into_iter().flatten().collect(),
                    Instant::now(),
                );
                h.beside = true;
                h
            });
            self.hint_dirty = true;
            self.redraw = true;
        }
    }

    /// Hinweiskarte, wenn das automatische Sichern scheitert (F-13 §8):
    /// unten mittig, Punkt in `ui.danger`, Verweis „Jetzt speichern“.
    fn fail_notice(&mut self, step: autosave::NoticeStep) {
        let link = ("Jetzt speichern", delete::Link::Save);
        match step {
            autosave::NoticeStep::Show => {
                let [a, b, _] = autosave::fail_notice_lines();
                let mut h =
                    delete::HintCard::new(vec![a, b], Some(link), Vec::new(), Instant::now());
                h.danger = true;
                self.hint = Some(h);
                self.hint_dirty = true;
                self.redraw = true;
            }
            autosave::NoticeStep::Hide => {
                let fade = self.theme.size.fade_ms * (self.theme.size.anim_ms > 0.0) as u8 as f32;
                if let Some(h) = self.hint.as_mut().filter(|h| h.link == Some(link)) {
                    h.dismiss(Instant::now(), fade);
                    self.redraw = true;
                }
            }
            autosave::NoticeStep::None => {}
        }
    }

    /// „Bündig setzen“ (Hinweis oder Paneel): beginnt die Zielwahl (E20);
    /// ohne Versatz wird nur gekoppelt.
    fn flush(&mut self, wall: sk_model::ElementId) {
        self.finish_flush();
        self.cancel_pick(false);
        if let Some(b) = sk_model::edit_blocked(self.scene.model(), &[wall]) {
            self.show_locked(b, Some((wall, delete::Act::Flush)));
            return;
        }
        match flush_pick::FlushPick::start(self.scene.model(), wall) {
            Some(p) => {
                self.pick = Some(p);
                self.pick_card();
                self.refresh_cursor();
            }
            None => {
                if let Some(below) = self.scene.model().wall_below(wall) {
                    self.flush_now(wall, below);
                }
            }
        }
    }

    /// Zielkarte „Zielwand anklicken“ neben dem Paar; sie bleibt, solange
    /// die Zielwahl läuft.
    fn pick_card(&mut self) {
        let Some(p) = &self.pick else {
            return;
        };
        let lines = flush_pick::CARD.map(String::from).to_vec();
        let mut h = delete::HintCard::new(lines, None, p.candidates().to_vec(), Instant::now());
        h.beside = true;
        h.hold();
        self.hint = Some(h);
        self.hint_dirty = true;
        self.redraw = true;
    }

    /// Zielwahl beenden; `notice`: „Bündig setzen abgebrochen.“ zeigen.
    fn cancel_pick(&mut self, notice: bool) {
        if self.pick.take().is_none() {
            return;
        }
        let fade = self.theme.size.fade_ms * (self.theme.size.anim_ms > 0.0) as u8 as f32;
        if let Some(h) = self.hint.as_mut().filter(|h| h.beside) {
            h.dismiss(Instant::now(), fade);
        }
        if notice {
            self.drag_notice = false;
            self.notice = Some(Notice {
                text: meldung::Meldung::satz(flush_pick::CANCELLED),
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: std::time::Duration::from_secs(3),
                catalog: false,
                error: false,
            });
        }
        self.refresh_cursor();
        self.redraw = true;
    }

    /// Kandidat unter dem Bildpunkt (Ansicht): das Bauteil unter der Maus,
    /// im Grundriss auch der Fuß der OG-Wand.
    fn pick_hit(&mut self, x: f64, y: f64) -> Option<sk_model::ElementId> {
        let (vw, vh, _) = self.view_size();
        let (view, plane) = (self.ui.view, self.plane());
        let hit = selection::pick_at(&mut self.scene, &self.cam, view, plane, x, y, vw, vh);
        let p = self.pick.as_ref()?;
        if p.target_of(hit).is_some() || view != ViewKind::Plan {
            return hit;
        }
        p.plan_hit(&self.scene, &self.cam, x, y, vw, vh).or(hit)
    }

    /// Maus und Tasten während der Zielwahl. `true`, wenn sie das Ereignis
    /// genommen hat; Bewegungen und Klicks auf Paneele gehen weiter.
    fn handle_pick(&mut self, e: Event) -> bool {
        let model = self.scene.model();
        if !self.pick.as_mut().is_some_and(|p| p.refresh(model)) {
            self.cancel_pick(false);
            return false;
        }
        let th = self.top() as f64;
        let in_view = |a: &App, x: f64, y: f64| {
            y >= th && !a.ui.over(x, y, a.w, a.top()) && a.wheel_hit(x, y).is_none()
        };
        let act = match e {
            Event::MouseMove { x, y, .. } => {
                let hit = if in_view(self, x, y) {
                    self.pick_hit(x, y - th)
                } else {
                    None
                };
                if self.pick.as_mut().is_some_and(|p| p.hover(hit)) {
                    self.redraw = true;
                }
                return false;
            }
            Event::MouseLeave => {
                if self.pick.as_mut().is_some_and(|p| p.hover(None)) {
                    self.redraw = true;
                }
                return false;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if !in_view(self, x, y) {
                    self.cancel_pick(false);
                    return false;
                }
                let hit = self.pick_hit(x, y - th);
                self.pick
                    .as_ref()
                    .map_or(flush_pick::Act::Cancel, |p| p.click(hit))
            }
            Event::MouseDown {
                button: MouseButton::Right,
                ..
            } => flush_pick::Act::Cancel,
            Event::Key {
                key, down: true, ..
            } => self
                .pick
                .as_ref()
                .map_or(flush_pick::Act::None, |p| p.key(key)),
            _ => return false,
        };
        match act {
            flush_pick::Act::None => return false,
            flush_pick::Act::Cancel => self.cancel_pick(true),
            flush_pick::Act::Flush(w, to) => {
                self.cancel_pick(false);
                self.flush_to(w, to);
            }
            flush_pick::Act::Refused(t, e) => {
                let lines = flush_pick::refused_lines(t, e).to_vec();
                let anchor = self
                    .pick
                    .as_ref()
                    .map_or(Vec::new(), |p| p.candidates().to_vec());
                let mut h = delete::HintCard::new(lines, None, anchor, Instant::now());
                h.danger = true;
                h.beside = true;
                self.hint = Some(h);
                self.hint_dirty = true;
                self.redraw = true;
            }
        }
        true
    }

    /// Bündig setzen: `wall` gleitet in `anim_ms` (ease-out) an die Wand
    /// `target`; ein Schritt.
    fn flush_to(&mut self, wall: sk_model::ElementId, target: sk_model::ElementId) {
        self.finish_flush();
        let m = self.scene.model();
        let upper = if m.wall_below(wall) == Some(target) {
            wall
        } else {
            target
        };
        let (Some((o, _)), Some((run, _))) = (m.stack_offset(upper), m.segment_of(wall)) else {
            return;
        };
        if self.theme.size.anim_ms > 0.0 && o != 0.0 && m.can_flush_to(wall, target).is_ok() {
            self.flush_keep =
                (upper == target && m.stack_offset(target).is_some_and(|s| s.1)).then_some(target);
            self.scene.begin("Bündig gesetzt");
            self.flush_anim = Some((wall, run, o, Instant::now(), target));
            self.redraw = true;
            return;
        }
        self.flush_now(wall, target);
    }

    /// Ein Bild des Gleitens; am Ende der eigentliche Schritt.
    fn step_flush(&mut self) {
        let Some((wall, _, from, start, target)) = self.flush_anim else {
            return;
        };
        let u = start.elapsed().as_secs_f64() * 1000.0 / self.theme.size.anim_ms as f64;
        if u >= 1.0 {
            self.finish_flush();
            return;
        }
        let e = 1.0 - (1.0 - u).powi(3);
        if self.scene.glide_flush(wall, target, from * (1.0 - e)) {
            self.upload_live();
        }
        self.redraw = true;
    }

    /// Gleiten sofort beenden (am Ende oder bei der nächsten Eingabe).
    fn finish_flush(&mut self) {
        if let Some((wall, .., target)) = self.flush_anim.take() {
            self.flush_keep = None;
            self.scene.rollback();
            self.flush_now(wall, target);
        }
    }

    fn flush_now(&mut self, wall: sk_model::ElementId, target: sk_model::ElementId) {
        if self.scene.flush_to(wall, target) == Ok(true) {
            self.upload_model();
            self.sync_props();
            self.drag_notice = false;
            self.notice = Some(Notice {
                text: meldung::Meldung::satz(flush_pick::done_text(
                    self.scene.model(),
                    wall,
                    target,
                )),
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: std::time::Duration::from_secs(5),
                catalog: false,
                error: false,
            });
            // Die Zielwand leuchtet einmal kurz
            if self.theme.size.anim_ms > 0.0 && self.theme.size.flash_ms > 0.0 {
                self.erase_flash = Some((self.now(), vec![target]));
            }
            self.redraw = true;
        }
    }

    // --- Baumpanel (Paket 4) ----------------------------------------------

    /// Abschnitte `[baum]` und `[hinweise]` für `einstellungen.txt`.
    fn panel_settings(&self) -> String {
        self.tree.settings_line() + &hints::line(&self.hints_seen)
    }

    /// Pille am Gummiband bzw. am gezogenen Band (Paket 8): Live-Länge oder
    /// Eingabe, 12 dip neben der Mitte des Gummibands zur Außenseite; beim
    /// Ziehen und danach an der Maus. Nach dem Loslassen blendet sie nach
    /// [`POST_PILL`] in `anim_ms` aus.
    fn paint_input_pill(&mut self, vw: f64, vh: f64, th: u32, scale: f32) {
        let s = scale as f64;
        let rev = self.theme.rev;
        let tool = self.tool.label().map(|(ends, text)| {
            let i = self.tool.input();
            let key = (
                text,
                i.is_some(),
                i.is_some_and(|i| i.error.is_some()),
                i.map_or(0, |i| i.active),
                scale.to_bits(),
                rev,
            );
            (key, Some(ends))
        });
        let edit = || {
            self.edit.pill(&self.scene).map(|(text, typing)| {
                let i = self.edit.input();
                let key = (
                    text,
                    typing,
                    i.is_some_and(|i| i.error.is_some()),
                    0,
                    scale.to_bits(),
                    rev,
                );
                (key, None)
            })
        };
        // Nordpfeil beim Aufziehen und Drehen: Pille hinter der Spitze
        let nord = self.nord.label(&self.cam, (vw, vh), s);
        let nord_key = || {
            nord.as_ref().map(|(_, text)| {
                let i = self.nord.input();
                let key = (
                    text.clone(),
                    i.is_some(),
                    i.is_some_and(|i| i.error.is_some()),
                    i.map_or(0, |i| i.active),
                    scale.to_bits(),
                    rev,
                );
                (key, None)
            })
        };
        let from_nord = tool.is_none() && self.edit.pill(&self.scene).is_none() && nord.is_some();
        let Some((key, ends)) = tool.or_else(edit).or_else(nord_key) else {
            self.post_at = None;
            if self.input_pill.take().is_some() {
                self.renderer.set_overlay(OVERLAY_INPUT, 0, 0, 0, 0, &[]);
                self.input_px = None;
            }
            if self.input_swap.take().is_some() {
                self.renderer
                    .set_overlay(OVERLAY_INPUT_OLD, 0, 0, 0, 0, &[]);
            }
            return;
        };
        if self.input_pill.as_ref() != Some(&key) {
            // Live-Pille → Eingabe: die alte blendet in `anim_ms` aus
            let live_before = self.input_pill.as_ref().is_some_and(|k| !k.1);
            if live_before && key.1 && self.theme.size.anim_ms > 0.0 {
                if let Some((px, w, h)) = &self.input_px {
                    self.renderer
                        .set_overlay(OVERLAY_INPUT_OLD, 0, 0, *w, *h, px);
                    self.input_swap = Some(Instant::now());
                }
            }
            let c = match (ends.is_some(), self.tool.input(), self.edit.input()) {
                _ if from_nord => match self.nord.input() {
                    Some(i) => i.paint(&self.ui.fonts, ["Nord", ""], scale, &self.theme),
                    None => flush_pick::paint_label(&self.ui.fonts, &key.0, scale, &self.theme),
                },
                (true, Some(i), _) => {
                    i.paint(&self.ui.fonts, wall_tool::LABELS, scale, &self.theme)
                }
                (false, _, Some(i)) => i.paint(
                    &self.ui.fonts,
                    [self.edit.input_label(), ""],
                    scale,
                    &self.theme,
                ),
                _ => flush_pick::paint_label(&self.ui.fonts, &key.0, scale, &self.theme),
            };
            let px = c.to_premul_rgba8();
            self.renderer
                .set_overlay(OVERLAY_INPUT, 0, 0, c.width as u32, c.height as u32, &px);
            self.input_px = Some((px, c.width as u32, c.height as u32));
            self.input_pill = Some(key.clone());
        }
        let (w, h) = self.renderer.overlay_size(OVERLAY_INPUT);
        let (pw, ph) = (w as f64, h as f64);
        let at = match ends {
            Some([p, q]) => {
                // Außenseite: links der Zeichenrichtung (Bezugsseite rechts:
                // rechts), im Bild gemessen
                let d = vec3(q.x - p.x, q.y - p.y, 0.0);
                let side = if self.tool.ref_side == sk_model::RefSide::Right {
                    -1.0
                } else {
                    1.0
                };
                let n = vec3(-d.y, d.x, 0.0).normalized() * side;
                let mid = (p + q) * 0.5;
                let a = self.cam.project(mid, vw, vh);
                let b = self.cam.project(mid + n * 100.0, vw, vh);
                a.map(|a| {
                    let dir = b.map_or((0.0, -1.0), |b| {
                        let (x, y) = (b.0 - a.0, b.1 - a.1);
                        let l = (x * x + y * y).sqrt();
                        if l > 1e-6 {
                            (x / l, y / l)
                        } else {
                            (0.0, -1.0)
                        }
                    });
                    // Abstand bis zum Rand der Pille in Richtung `dir`
                    let reach = 12.0 * s + (pw * 0.5 * dir.0.abs()).max(ph * 0.5 * dir.1.abs());
                    (a.0 + dir.0 * reach, a.1 + dir.1 * reach + th as f64)
                })
            }
            None if from_nord => nord
                .and_then(|(p, _)| self.cam.project(p, vw, vh))
                .map(|(x, y)| (x, y + th as f64)),
            None => self
                .mouse_at
                .map(|(x, y)| (x + 14.0 * s + pw * 0.5, y + 20.0 * s + ph * 0.5)),
        };
        let Some((x, y)) = at else {
            self.renderer.place_overlay(OVERLAY_INPUT, 0, 0, 0, 0, 0.0);
            return;
        };
        // Nachkorrektur: stehen lassen, dann ausblenden
        let post = ends.is_none() && !key.1 && !self.edit.is_dragging() && !from_nord;
        let alpha = if post {
            let t0 = *self.post_at.get_or_insert_with(Instant::now);
            let anim = self.theme.size.anim_ms.max(0.0) as f64;
            let over = t0.elapsed().saturating_sub(POST_PILL).as_secs_f64() * 1000.0;
            if over > 0.0 && over >= anim {
                self.edit.end_post();
                self.post_at = None;
                self.input_pill = None;
                self.renderer.set_overlay(OVERLAY_INPUT, 0, 0, 0, 0, &[]);
                self.redraw = true;
                return;
            }
            if anim > 0.0 {
                (1.0 - over / anim) as f32
            } else {
                1.0
            }
        } else {
            self.post_at = None;
            1.0
        };
        // Überblenden: beide Bilder an derselben Mitte, die Lage bleibt
        let swap = self.input_swap.map(|t0| {
            let anim = self.theme.size.anim_ms.max(1.0) as f64;
            (t0.elapsed().as_secs_f64() * 1000.0 / anim).min(1.0) as f32
        });
        let (cx, cy) = (x, y);
        let (x, y) = ((x - pw * 0.5).round() as i32, (y - ph * 0.5).round() as i32);
        self.renderer
            .place_overlay(OVERLAY_INPUT, x, y, w, h, alpha * swap.unwrap_or(1.0));
        match swap {
            Some(k) if k < 1.0 => {
                let (ow, oh) = self.renderer.overlay_size(OVERLAY_INPUT_OLD);
                let ox = (cx - ow as f64 * 0.5).round() as i32;
                let oy = (cy - oh as f64 * 0.5).round() as i32;
                self.renderer
                    .place_overlay(OVERLAY_INPUT_OLD, ox, oy, ow, oh, alpha * (1.0 - k));
                self.redraw = true;
            }
            Some(_) => {
                self.input_swap = None;
                self.renderer
                    .set_overlay(OVERLAY_INPUT_OLD, 0, 0, 0, 0, &[]);
                self.redraw = true;
            }
            None => {}
        }
    }

    /// Wann die Pille nach dem Loslassen wieder gezeichnet werden muss.
    fn pill_wait(&self) -> Option<std::time::Duration> {
        if self.input_swap.is_some() {
            return Some(FRAME);
        }
        let t0 = self.post_at?;
        let e = t0.elapsed();
        Some(if e < POST_PILL { POST_PILL - e } else { FRAME })
    }

    /// Fehler der Maßeingabe einmal in der Statuszeile nennen; dazu der
    /// Entdecken-Hinweis beim ersten gesetzten Punkt (Paket 8).
    fn sync_measure(&mut self) {
        if self.tool.points().len() == 1 {
            self.discover("measure");
        }
        let err = self
            .tool
            .input()
            .or(self.edit.input())
            .and_then(|i| i.error.clone());
        if err != self.input_error {
            if let Some(text) = &err {
                self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                self.notice = Some(Notice {
                    text: text.clone(),
                    since: None,
                    rect: (0.0, 0.0, 0.0, 0.0),
                    time: NOTICE_TIME,
                    catalog: false,
                    error: true,
                });
                self.redraw = true;
            }
            self.input_error = err;
        }
    }

    /// Entdecken-Hinweis `id` beim ersten Mal (A0b), in der Statuszeile.
    fn discover(&mut self, id: &str) {
        if self.hints_seen.contains(id) {
            return;
        }
        let Some(text) = hints::text(id) else {
            return;
        };
        self.hints_seen.insert(id.to_string());
        self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
        self.notice = Some(Notice {
            text: meldung::Meldung::satz(text),
            since: None,
            rect: (0.0, 0.0, 0.0, 0.0),
            time: NOTICE_TIME,
            catalog: false,
            error: false,
        });
        self.redraw = true;
    }

    /// Entdecken-Hinweis `id` beim ersten Mal als Hinweiskarte: Titel, Satz,
    /// Verweis und ×; steht, bis sie geschlossen wird.
    fn discover_card(&mut self, id: &str, title: &str, link: (&'static str, delete::Link)) {
        if self.hints_seen.contains(id) {
            return;
        }
        let Some(text) = hints::text(id) else {
            return;
        };
        self.hints_seen.insert(id.to_string());
        let lines = vec![title.to_string(), text.to_string()];
        let card = delete::HintCard::new(lines, Some(link), Vec::new(), Instant::now());
        self.hint = Some(card.discovering());
        self.hint_dirty = true;
    }

    /// Lage des Baumpanels (Pixel, ohne Schatten): unter „Ansichten“ bis
    /// `panel_margin` über den Fensterrand; mit Auswahl darunter die
    /// Eigenschaften in natürlicher Höhe, getrennt durch die Grenze.
    fn tree_rect(&self) -> sk_ui::widgets::Rect {
        let s = self.ui.scale;
        let z = &self.theme.size;
        let top = self.top();
        let v = self.ui.rect(Panel::Views, self.w, top);
        let m = (z.panel_margin * s).round();
        let y = v.y + v.h + m;
        let avail = (self.h as f32 - m - y).max(0.0);
        let min = self.tree.min_height(z, s);
        let h = if self.tree.collapsed {
            min
        } else if self.ui.has_props() {
            let rh = (z.tree_row_h * s).round();
            // Eigenschaften in natürlicher Höhe, höchstens 55 % der Spalte
            let props = self.ui.props_natural_height().min(0.55 * avail);
            let want = match self.tree.split {
                Some(d) => (d * s).round(),
                None => avail - m - props,
            };
            want.min(avail - m - 3.0 * rh).max(min)
        } else {
            avail.max(min)
        };
        sk_ui::widgets::Rect::new(v.x, y, v.w, h.round())
    }

    /// Baumpanel an Modell, Auswahl und Fenster angleichen und neu zeichnen,
    /// wenn sich etwas geändert hat. Die Eigenschaften rücken mit.
    fn sync_tree(&mut self) {
        if self.w == 0 || self.ui.dialog {
            return;
        }
        let now = Instant::now();
        let s = self.ui.scale;
        let props = self.ui.has_props();
        let animate = props != self.tree_props;
        self.tree_props = props;
        self.tree.divider = props;
        let r = self.tree_rect();
        self.tree.place(r, &self.theme, now, animate);
        if self.tree.growing(now, &self.theme) {
            self.redraw = true;
        }
        let h = self.tree.height(now, &self.theme);
        let m = (self.theme.size.panel_margin * s).round();
        // Platz der Eigenschaften unter dem Baum (nach dem Übergang); mehr
        // Inhalt rollt
        let room = if props {
            (self.h as f32 - m - (r.y + r.h + m)).max(0.0)
        } else {
            0.0
        };
        if room != self.ui.props_room {
            self.ui.props_room = room;
            self.props_dirty = true;
        }
        let slot = h + m;
        if slot != self.ui.tree_slot {
            self.ui.tree_slot = slot;
            if props {
                let (x, y) = self.ui.origin(Panel::Props, self.w, self.top());
                self.renderer.move_overlay(OVERLAY_PROPS, x, y);
            }
            self.redraw = true;
        }
        // Beim Ziehen nicht neu bauen (Leistung), danach einmal
        if !self.edit.is_dragging() {
            self.tree.sync(&self.scene, &self.picking, &self.theme, s);
        }
        // Auswahl im Modell: Ast aufklappen und hinrollen (nach dem Bau
        // der Karten, auch beim ersten Bild)
        let p = self.picking.primary();
        if p != self.tree_primary {
            self.tree_primary = p;
            if let Some(id) = p.filter(|_| !self.tree_picked) {
                self.tree.reveal(self.scene.model(), id, &self.theme, s);
            }
            self.tree_picked = false;
        }
        // Befehlszeile (Bildvergleiche): überfahrene Zeile leuchtet im Modell
        let cli = self.tree.cli_hover.is_some();
        if let Some(a) = self.tree.apply_cli(self.scene.model()) {
            self.tree_action(a);
        }
        if cli {
            let ids = self.tree.hover_ids();
            self.picking.set_hover(None, ids);
            self.hover_from_list = true;
            self.tree.sync(&self.scene, &self.picking, &self.theme, s);
        }
        let out = self.tree.render(
            &self.theme,
            &self.ui.fonts,
            &self.scene,
            &self.picking,
            h,
            s,
        );
        match out {
            Some(tree_panel::TreeOut::Full(c)) => {
                let sh = (self.theme.size.panel_shadow * s).round();
                c.premul_rgba8_into(&mut self.panel_px);
                self.renderer.set_overlay(
                    OVERLAY_TREE,
                    (r.x - sh) as i32,
                    (r.y - sh) as i32,
                    c.width as u32,
                    c.height as u32,
                    &self.panel_px,
                );
                self.redraw = true;
            }
            // Nur Zeilenbänder (Hover, Leuchten)
            Some(tree_panel::TreeOut::Parts(parts)) => {
                for (x, y, c) in parts {
                    c.premul_rgba8_into(&mut self.panel_px);
                    self.renderer.update_overlay(
                        OVERLAY_TREE,
                        x as i32,
                        y as i32,
                        c.width as u32,
                        c.height as u32,
                        &self.panel_px,
                    );
                }
                self.redraw = true;
            }
            None => {}
        }
    }

    /// Maus über dem Baumpanel: `true`, wenn es das Ereignis genommen hat.
    fn handle_tree(&mut self, e: Event, surface: &Surface) -> bool {
        if self.w == 0
            || self.ui.dialog
            || self.ui.level_dragging().is_some()
            || self.ui.edit.is_some() && matches!(e, Event::Key { .. })
            || self.tool.is_active()
            || self.nav.is_dragging()
            || self.edit.is_dragging()
            || self.sect.is_dragging()
        {
            return false;
        }
        let s = self.ui.scale;
        let out = self
            .tree
            .handle(&e, &self.scene, &self.picking, &self.theme, s);
        // Zeile unter der Maus: ihre Bauteile leuchten im Modell
        let ids = self.tree.hover_ids();
        if !ids.is_empty() || self.hover_from_tree {
            if self.picking.set_hover(None, ids.clone()) {
                self.redraw = true;
            }
            self.hover_from_tree = !ids.is_empty();
            self.hover_from_list = self.hover_from_tree;
        }
        if out.relayout {
            self.tree.dirty = true;
        }
        if let Some(a) = out.action {
            self.tree_action(a);
        }
        if let Event::MouseMove { x, y, .. } = e {
            if out.consumed {
                if !self.in_tree {
                    // Modell, Paneele und Titelleiste: die Maus ist weg
                    self.in_tree = true;
                    self.handle_inner(Event::MouseLeave, surface);
                    self.dirty_title.extend(self.title.hover.take());
                }
                self.mouse_at = Some((x, y));
            } else {
                self.in_tree = false;
            }
        }
        out.consumed
    }

    /// Handlung aus dem Baumpanel ausführen.
    fn tree_action(&mut self, a: tree_panel::Action) {
        use tree_panel::Action as A;
        match a {
            A::Select(ids) => self.tree_select(ids),
            A::Storey(sid, ids) => {
                self.tree_select(ids);
                if matches!(self.ui.view, ViewKind::Plan | ViewKind::Section) {
                    let t = self.now();
                    self.wheel.go_to_level(&mut self.scene, sid, t);
                } else {
                    self.scene.set_active_storey(sid);
                }
                if self.ui.view == ViewKind::Plan {
                    self.upload_model();
                }
                self.sync_levels();
            }
            A::Zoom(ids) => self.zoom_to(&ids),
            A::Context { target, x, y, .. } => {
                if !self.picking.is_selected(target) {
                    self.tree_select(vec![target]);
                }
                self.close_type_menu(false);
                let s = self.ui.scale;
                self.context = Some(delete::ContextMenu::new(
                    self.scene.model(),
                    target,
                    &self.picking.selected,
                    x,
                    y,
                    (self.w, self.h, self.top()),
                    &self.theme,
                    s,
                ));
                self.context_dirty = true;
                self.tip = None;
                self.redraw = true;
            }
            A::Visibility(v) => self.fade_to(v),
            A::Lock(ids, on) => {
                let label = if on { "Gesperrt" } else { "Entsperrt" };
                self.scene.edit_model(label, |m| {
                    m.set_locked(&ids, on);
                    true
                });
                self.upload_model();
            }
            A::Delete(ids) => {
                self.picking.selected = ids;
                self.tree_picked = true;
                self.erase(false);
            }
            A::DeleteBuilding(b) => self.open_confirm(b),
            A::Tab(t) => {
                if t == sk_model::tree::Tab::Trade {
                    self.discover("trades");
                }
            }
        }
        self.tree.dirty = true;
    }

    /// Auswahl aus dem Baum: Hauptfenster und Mengenliste folgen.
    fn tree_select(&mut self, ids: Vec<sk_model::ElementId>) {
        if self.picking.selected == ids {
            return;
        }
        self.picking.selected = ids;
        self.tree_picked = true;
        self.quantity.dirty = true;
        if self
            .picking
            .primary()
            .is_some_and(|e| self.scene.follow_selection(e))
        {
            if self.ui.view == ViewKind::Plan {
                self.upload_model();
            }
            self.sync_levels();
        }
        self.sync_props();
        self.redraw = true;
    }

    /// Neue Sichtbarkeit mit Übergang (Baum, Esc). Erstes Ausblenden:
    /// Entdecken-Hinweis.
    fn fade_to(&mut self, v: sk_model::view::Visibility) {
        let old = self.scene.model().visibility().clone();
        let more = v.hidden.len() > old.hidden.len()
            || v.hidden_cat.len() > old.hidden_cat.len()
            || v.hidden_trade.len() > old.hidden_trade.len();
        if self.scene.skip_vis_animation() {
            self.upload_model();
        }
        if self.scene.fade_visibility(v) {
            self.upload_model();
            self.quantity.dirty = true;
            self.redraw = true;
            if more {
                self.discover("hide_counts");
            }
        }
        self.sync_props();
    }

    /// Hinweiskarte „AW-005 ist gesperrt.“ mit dem Verweis „Entsperren im
    /// Baum mit dem Schloss“, Punkt in `ui.danger` (Paket 4 §1.7).
    /// Hinweiskarte „AW-005 ist gesperrt.“; `acted`: das Bauteil, an dem
    /// gehandelt wurde, und wie. Ist es ein anderes, sagt die zweite Zeile,
    /// warum das gesperrte betroffen ist (Prüfung p4-7, aa).
    fn show_locked(
        &mut self,
        id: sk_model::ElementId,
        acted: Option<(sk_model::ElementId, delete::Act)>,
    ) {
        let m = self.scene.model();
        let link = (
            "Entsperren im Baum mit dem Schloss",
            delete::Link::Unlock(id),
        );
        let lines = delete::locked_card(m, id, acted);
        let mut h = delete::HintCard::new(lines, Some(link), vec![id], Instant::now());
        h.danger = true;
        self.hint = Some(h);
        self.hint_dirty = true;
        self.redraw = true;
    }

    /// Hinweise aus dem Laden: leise in die Statuszeile, der Rest als Meldung.
    fn show_hints(&mut self, hints: Vec<meldung::Meldung>, surface: &Surface) {
        let (quiet, loud): (Vec<_>, Vec<_>) = hints.into_iter().partition(|h| catalog::quiet(h));
        if !loud.is_empty() {
            surface.message(&meldungen(&loud), false);
        }
        if let Some(text) = quiet.into_iter().next() {
            self.notice = Some(Notice {
                text,
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: NOTICE_TIME,
                catalog: true,
                error: false,
            });
        }
    }

    /// Statuszeile beim Ziehen einer gestapelten Wand (OG Phase 2): steht,
    /// solange gezogen wird.
    fn sync_drag_notice(&mut self) {
        let want = self
            .edit
            .stack_drag(&self.scene)
            .map(|d| match d {
                wall_edit::StackDrag::Free => meldung::Meldung::satz(
                    "Kette gelöst: nur die OG-Wand bewegt sich, das EG bleibt stehen.",
                ),
                wall_edit::StackDrag::Ctrl => {
                    meldung::Meldung::satz("Strg: nur diese Wand, die Kette bleibt geschlossen.")
                }
            })
            // Zielwahl beim „Bündig setzen“: was über dem Kandidaten geschieht
            .or_else(|| self.pick.as_ref().and_then(|p| p.status()));
        match want {
            Some(t) if self.notice.as_ref().is_none_or(|n| n.text != t) => {
                self.notice = Some(Notice {
                    text: t,
                    since: None,
                    rect: (0.0, 0.0, 0.0, 0.0),
                    time: std::time::Duration::from_secs(3600),
                    catalog: false,
                    error: false,
                });
                self.drag_notice = true;
            }
            None if std::mem::take(&mut self.drag_notice) => {
                self.notice = None;
                self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                self.redraw = true;
            }
            _ => {}
        }
    }

    /// Hinweis in der Statuszeile zeigen, nach [`NOTICE_TIME`] wieder weg.
    fn sync_notice(&mut self) {
        self.sync_drag_notice();
        let Some(n) = self.notice.as_mut() else {
            return;
        };
        match n.since {
            Some(at) if at.elapsed() >= n.time => {
                self.notice = None;
                self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                self.redraw = true;
            }
            Some(_) => {
                // Mittig unten halten, auch wenn das Fenster seine Größe ändert
                let x = ((self.w as f64 - n.rect.2) * 0.5).round();
                let y = (self.h as f64 - n.rect.3 - 16.0 * self.ui.scale as f64).round();
                if (x, y) != (n.rect.0, n.rect.1) {
                    n.rect.0 = x;
                    n.rect.1 = y;
                    self.renderer
                        .move_overlay(OVERLAY_NOTICE, x as i32, y as i32);
                    self.redraw = true;
                }
            }
            None if self.w > 0 => {
                let s = self.ui.scale;
                let dot = if n.error {
                    self.theme.ui.field_invalid
                } else {
                    self.theme.ui.accent
                };
                let c = sk_ui::widgets::notice_dot(&self.ui.fonts, &n.text, s, &self.theme, dot);
                let (cw, ch) = (c.width as f64, c.height as f64);
                let x = ((self.w as f64 - cw) * 0.5).round();
                let y = (self.h as f64 - ch - 16.0 * s as f64).round();
                n.rect = (x, y, cw, ch);
                n.since = Some(Instant::now());
                let px = c.to_premul_rgba8();
                self.renderer.set_overlay(
                    OVERLAY_NOTICE,
                    x as i32,
                    y as i32,
                    c.width as u32,
                    c.height as u32,
                    &px,
                );
                self.redraw = true;
            }
            None => {}
        }
    }

    /// Klick auf den Hinweis: Bauteilkatalog im Reiter Firma.
    fn click_notice(&mut self, x: f64, y: f64) -> bool {
        let Some((rx, ry, rw, rh)) = self
            .notice
            .as_ref()
            .filter(|n| n.since.is_some() && n.catalog)
            .map(|n| n.rect)
        else {
            return false;
        };
        if x < rx || y < ry || x > rx + rw || y > ry + rh {
            return false;
        }
        self.notice = None;
        self.renderer.set_overlay(OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
        self.redraw = true;
        self.open_catalog();
        if let Some(c) = self.catalog.as_mut() {
            c.tab = catalog_view::Tab::Company;
        }
        true
    }

    /// Hinweis an der Maus nachführen: neuer Text beginnt die Wartezeit,
    /// danach erscheint er rechts unter der Maus; ohne Wunsch verschwindet er.
    fn sync_tip(&mut self) {
        let want = self.tip_wanted().zip(self.mouse_at);
        let same = matches!((&self.tip, &want), (Some(t), Some((w, _))) if t.text == *w);
        if !same {
            if self.tip.take().is_some_and(|t| t.shown) {
                self.renderer.set_overlay(OVERLAY_TIP, 0, 0, 0, 0, &[]);
                self.redraw = true;
            }
            self.tip = want.map(|(text, at)| Tip {
                text,
                at,
                since: Instant::now(),
                shown: false,
            });
        }
        let Some(t) = self.tip.as_mut().filter(|t| !t.shown) else {
            return;
        };
        if t.since.elapsed() < TIP_DELAY {
            return;
        }
        t.shown = true;
        let s = self.ui.scale;
        let c = sk_ui::widgets::tooltip(&self.ui.fonts, &t.text, s, &self.theme);
        let (cw, ch) = (c.width as f64, c.height as f64);
        let x = (t.at.0 + 12.0 * s as f64).min(self.w as f64 - cw).max(0.0);
        let mut y = t.at.1 + 20.0 * s as f64;
        if y + ch > self.h as f64 {
            y = t.at.1 - 8.0 * s as f64 - ch;
        }
        let px = c.to_premul_rgba8();
        self.renderer.set_overlay(
            OVERLAY_TIP,
            x.round() as i32,
            y.round() as i32,
            c.width as u32,
            c.height as u32,
            &px,
        );
        self.redraw = true;
    }

    /// Wartezeit bis zum nächsten Hinweis an der Maus (auch dem an einer
    /// Spitze des Geschossbogens).
    fn tip_wait(&self) -> Option<std::time::Duration> {
        let tip = self
            .tip
            .as_ref()
            .filter(|t| !t.shown)
            .map(|t| TIP_DELAY.saturating_sub(t.since.elapsed()));
        let hud = self
            .wheel
            .hint_wait(self.now())
            .map(std::time::Duration::from_millis);
        let notice = self
            .notice
            .as_ref()
            .and_then(|n| Some(n.time.saturating_sub(n.since?.elapsed())));
        let fade = self.theme.size.fade_ms * (self.theme.size.anim_ms > 0.0) as u8 as f32;
        // Gezeigte Deckkraft noch nicht am Ziel (Ende des Einblendens):
        // ein Bild mehr
        let hint = self.hint.as_ref().map(|h| {
            let now = Instant::now();
            match h.alpha(now, fade) {
                Some(a) if a != self.hint_alpha => FRAME,
                _ => h.wait(now, fade),
            }
        });
        let list = if self.quantity.open {
            self.quantity.wait(&self.theme, Instant::now())
        } else {
            None
        };
        let save = self
            .autosave
            .as_ref()
            .filter(|_| !self.edit.is_dragging())
            .and_then(|a| a.wait(self.scene.model(), &self.doc, self.clock.elapsed()));
        [tip, hud, notice, hint, list, save, self.help_wait()]
            .into_iter()
            .flatten()
            .min()
    }

    /// Normal beendet: Einstellungen schreiben, die Sicherung gilt als
    /// sauber beendet (keine Startkarte beim nächsten Start).
    fn closing(&mut self, surface: &Surface) {
        let panel = self.panel_settings();
        save_settings(
            &mut self.settings,
            &self.theme,
            &self.recent,
            (self.quantity.grouping, self.quantity.blatt()),
            panel,
            surface,
        );
        if let Some(a) = self.autosave.as_mut() {
            a.closed(&self.doc);
        }
    }

    /// Uhr des Geschossbogens: Millisekunden seit dem Start.
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    /// Was der Geschossbogen zeigen soll: nur im Grundriss, sanft weg,
    /// solange ein Dialog, das Dateimenü, die Einstellungen oder die Rückfrage
    /// „Gebäude löschen“ offen sind.
    fn wheel_show(&self) -> wheel_view::Show {
        let blocked = self.ui.dialog
            || self.menu.is_open()
            || self.modal()
            || self.save_dlg.is_some()
            || self.confirm.is_some();
        if !self.wheel.visible(self.ui.view, false) {
            wheel_view::Show::Off
        } else if !self.wheel.visible(self.ui.view, blocked) {
            wheel_view::Show::Blocked
        } else {
            wheel_view::Show::On
        }
    }

    /// Teil des Geschossbogens an der Stelle (Fenster-Pixel).
    fn wheel_hit(&self, x: f64, y: f64) -> Option<wheel::Part> {
        if self.wheel_show() != wheel_view::Show::On {
            return None;
        }
        self.wheel_view
            .hit(&self.wheel, &self.ui, self.w, self.h, x, y)
    }

    /// Maus über einer Spitze, die wechseln kann: Zeiger „Hand“.
    fn wheel_hand(&self) -> bool {
        let input = self.tool.is_active();
        match self.wheel.hover {
            Some(wheel::Part::Up) => self.wheel.arrow_enabled(&self.scene, true, input),
            Some(wheel::Part::Down) => self.wheel.arrow_enabled(&self.scene, false, input),
            Some(wheel::Part::Mirror) => true,
            _ => false,
        }
    }

    /// Geschossbogen (E18) je Durchlauf: einen begonnenen Wechsel übernehmen
    /// (altes Bild festhalten, Grundriss des Ziels), Überblendung und Bilder
    /// nachführen.
    fn sync_wheel(&mut self) {
        let t = self.now();
        self.wheel.tick(&mut self.scene, t);
        // Grundriss (Geschosse) oder Schnitt (Schnittrad)
        let plan = self.wheel.visible(self.ui.view, false);
        if self.wheel.take_started().is_some() {
            if plan && self.wheel.animating(t) && self.w > 0 {
                self.renderer.capture_scene();
                // Die Zeit läuft ab dem ersten Bild mit dem neuen Grundriss
                self.wheel.wait_for_first_frame();
            }
            if plan && self.ui.view == ViewKind::Section {
                // Anderer Schnitt: eigene Lage und Blickrichtung
                self.sect.ensure(&self.scene);
                self.fit_camera();
            }
            if plan {
                self.upload_model();
            }
            self.sync_levels();
            self.overlay_dirty = true;
            self.refresh_cursor();
        }
        self.view_shift = 0.0;
        self.view_squeeze = (0.0, 1.0, 0.0);
        let turn = self.turn.filter(|_| self.ui.view == ViewKind::Section);
        match self.wheel.progress(t).filter(|_| plan) {
            Some((e, sw)) => {
                let slide = wheel::SLIDE * self.ui.dpi();
                if let (wheel::Stop::Cut(a), wheel::Stop::Cut(b)) = (sw.from, sw.to) {
                    // Schnitte: seitlich, zu B gleitet das alte Bild nach
                    // links und das neue kommt von rechts
                    let dir = if b > a { -1.0 } else { 1.0 };
                    self.renderer.set_snapshot_xy(1.0 - e, dir * slide * e, 0.0);
                    self.view_squeeze = (-dir * slide * (1.0 - e), 1.0, 0.0);
                } else {
                    // Nach oben: das alte Geschoss sinkt weg, das neue kommt von oben
                    let dir = sw.steps.signum() as f32;
                    self.renderer.set_snapshot(1.0 - e, dir * slide * e);
                    self.view_shift = -dir * slide * (1.0 - e);
                }
                self.redraw = true;
            }
            None if turn.is_some() => {
                // Blatt wenden: das alte Bild staucht sich zur Achse, dann
                // öffnet sich das neue, in der Mitte leicht abgedunkelt
                let (t0, axis) = turn.unwrap_or_default();
                let p = t.saturating_sub(t0) as f32 / self.theme.size.anim_ms.max(1.0);
                let e = wheel::ease_in_out_cubic(p.min(1.0));
                if p >= 1.0 {
                    self.turn = None;
                    self.renderer.release_snapshot();
                } else if e < 0.5 {
                    let shade = 1.0 - 0.15 * (std::f32::consts::PI * e).sin();
                    self.renderer.set_snapshot_turn(1.0 - 2.0 * e, axis, shade);
                } else {
                    self.renderer.set_snapshot(0.0, 0.0);
                    self.view_squeeze = (0.0, 2.0 * e - 1.0, axis);
                }
                self.redraw = true;
            }
            None => match self.erase_fade {
                // Gelöschtes blendet in `fade_ms` aus, Rückgängig blendet ein
                Some(t0) => {
                    let e = t.saturating_sub(t0) as f32 / self.theme.size.fade_ms.max(1.0);
                    if e >= 1.0 {
                        self.erase_fade = None;
                        self.renderer.release_snapshot();
                    } else {
                        self.renderer.set_snapshot(1.0 - e, 0.0);
                    }
                    self.redraw = true;
                }
                None => self.renderer.release_snapshot(),
            },
        }
        let show = self.wheel_show();
        if show != wheel_view::Show::On {
            self.wheel.set_hover(None, t);
        }
        if self.w > 0 {
            let input = self.tool.is_active();
            let changed = self.wheel_view.sync(
                &mut self.renderer,
                &self.wheel,
                &self.scene,
                &self.ui,
                &self.theme,
                (self.w, self.h),
                t,
                show,
                input,
            );
            self.redraw |= changed || self.wheel_view.fading(show);
        }
    }

    /// Dialog „Gebäude erstellen“ samt Abdunkeln des Modellfensters
    /// zeichnen oder ausblenden.
    fn paint_dialog(&mut self) {
        if self.ui.dialog {
            let th = self.title.height();
            let (c, x, y) = self.ui.paint(&self.theme, Panel::Dialog, self.w, th);
            c.premul_rgba8_into(&mut self.panel_px);
            self.renderer.set_overlay(
                OVERLAY_DIALOG,
                x,
                y,
                c.width as u32,
                c.height as u32,
                &self.panel_px,
            );
            let k = self.theme.env.scrim;
            let a = k.3 as u32;
            let pm = |v: u8| ((v as u32 * a + 127) / 255) as u8;
            let scrim = [pm(k.0), pm(k.1), pm(k.2), k.3];
            let h = self.h.saturating_sub(th);
            self.renderer
                .set_overlay_fill(OVERLAY_SCRIM, 0, th as i32, self.w, h, scrim);
        } else {
            self.renderer.set_overlay(OVERLAY_DIALOG, 0, 0, 0, 0, &[]);
            self.renderer.set_overlay(OVERLAY_SCRIM, 0, 0, 0, 0, &[]);
        }
    }

    /// Paneel „Eigenschaften“ zeichnen oder (ohne Auswahl) ausblenden.
    fn paint_props(&mut self) {
        if self.ui.props().is_some() {
            let (c, x, y) = self
                .ui
                .paint(&self.theme, Panel::Props, self.w, self.title.height());
            c.premul_rgba8_into(&mut self.panel_px);
            self.renderer.set_overlay(
                OVERLAY_PROPS,
                x,
                y,
                c.width as u32,
                c.height as u32,
                &self.panel_px,
            );
        } else {
            self.renderer.set_overlay(OVERLAY_PROPS, 0, 0, 0, 0, &[]);
        }
        self.props_dirty = false;
        self.redraw = true;
    }

    /// Zeichnet nur die Knöpfe neu, die sich geändert haben (Hover, Drücken),
    /// und überträgt nur ihren Ausschnitt.
    fn paint_buttons(&mut self, surface: &Surface) {
        for b in std::mem::take(&mut self.dirty_title) {
            let Some(c) = self
                .title_img
                .as_mut()
                .filter(|c| c.width == self.w as usize)
            else {
                self.paint_title(surface);
                break;
            };
            let (x, w) = self.title.repaint_button(&self.theme, c, b, self.w);
            let (x, y, w, h, px) = c.region_premul_rgba8(x, 0, w, c.height);
            self.renderer.update_overlay(
                OVERLAY_TITLE,
                x as i32,
                y as i32,
                w as u32,
                h as u32,
                &px,
            );
            self.redraw = true;
        }
        let mut ids = std::mem::take(&mut self.dirty_buttons);
        let mut i = 0;
        while i < ids.len() {
            if ids[..i].contains(&ids[i]) {
                ids.remove(i);
            } else {
                i += 1;
            }
        }
        for id in ids {
            let Some(p) = self.ui.repaint_button(&self.theme, id) else {
                self.paint_overlays(surface);
                return;
            };
            let slot = match p.panel {
                Panel::Tools => OVERLAY_TOOLS,
                Panel::Views => OVERLAY_VIEWS,
                Panel::Props => OVERLAY_PROPS,
                Panel::Levels => OVERLAY_LEVELS,
                Panel::Dialog => OVERLAY_DIALOG,
            };
            self.renderer
                .update_overlay(slot, p.x as i32, p.y as i32, p.w as u32, p.h as u32, &p.px);
            self.redraw = true;
        }
    }

    /// Neue Fensterbreite bei gleicher Paneelgröße: nur die Titelleiste neu,
    /// die Paneele behalten ihr Bild und rücken an ihren Platz.
    fn relayout_overlays(&mut self, surface: &Surface) {
        if self.ui.dialog {
            // Das Abdunkeln folgt der Fenstergröße
            self.paint_overlays(surface);
            return;
        }
        self.paint_title(surface);
        let th = self.title.height();
        for (slot, p) in [
            (OVERLAY_TOOLS, Panel::Tools),
            (OVERLAY_VIEWS, Panel::Views),
            (OVERLAY_PROPS, Panel::Props),
            (OVERLAY_LEVELS, Panel::Levels),
        ] {
            let (x, y) = self.ui.origin(p, self.w, th);
            self.renderer.move_overlay(slot, x, y);
        }
        self.layout_dirty = false;
        self.redraw = true;
    }
}

/// Fasst direkt aufeinanderfolgende Mausbewegungen zur letzten zusammen.
///
/// Eine Maus mit hoher Abtastrate liefert viele Bewegungen je Bild. Jede würde
/// beim Ziehen das Modell neu berechnen, gezeigt wird aber nur der letzte Stand.
/// Drücken, Loslassen und Tasten trennen die Folgen, damit keine Klickposition
/// verloren geht. Die Navigation rechnet mit der Differenz zur letzten Position
/// und bekommt so dieselbe Gesamtbewegung.
fn coalesce_moves(events: &mut Vec<Event>) {
    events.dedup_by(|next, prev| {
        if let (Event::MouseMove { .. }, Event::MouseMove { .. }) = (&*prev, &*next) {
            *prev = *next;
            true
        } else {
            false
        }
    });
}

/// Zeilen eines Tooltips als Text für [`sk_ui::widgets::tooltip`] (erste
/// Zeile fett), ohne Fettschrift-Marken.
fn tip_text(lines: Vec<String>) -> String {
    lines
        .iter()
        .map(|l| help::plain(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_timing(path: &Option<String>, log: &mut String) {
    use std::io::Write;
    if let Some(p) = path {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(p) {
            let _ = f.write_all(log.as_bytes());
        }
    }
    log.clear();
}

/// Zeilen „14 cm Dämmung (WDVS)“ usw. mit der Schnittfarbe des Baustoffs.
fn layer_rows(model: &sk_model::Model, set: sk_model::LayerSetId) -> Vec<(Rgba, String)> {
    let Some(set) = model.layer_set(set) else {
        return Vec::new();
    };
    set.layers
        .iter()
        .filter_map(|l| {
            let m = model.material(l.material)?;
            let rgb = model.attr().surface(m.surface)?.cut_color;
            let cm = l.thickness / 10.0;
            let cm = if cm.fract().abs() < 1e-9 {
                format!("{cm:.0}")
            } else {
                format!("{cm:.1}").replace('.', ",")
            };
            Some((Rgba::from_rgb8(rgb), format!("{cm} cm {}", m.name)))
        })
        .collect()
}

/// Neues Projekt mit den Typen des Firmenkatalogs (ohne ihn: Startbestand)
/// und den Firmenvorgaben `v` (Sonnenstand S8): der Standardort als
/// Bauort, wenn er von Ganderkesee abweicht; mit Schattenlicht „Sonne“ der
/// Stand heute 12:00 (System aus), damit die Ansichten nicht mit dem Tag
/// wechseln. Beides ohne Schritt.
fn new_model(
    company: Option<&catalog::Company>,
    v: settings::vorgaben::Vorgaben,
) -> sk_model::Model {
    let ort = v.ort;
    let mut m = match company {
        Some(c) => {
            let mut m = sk_model::Model::from_library(c.library());
            // Kopie der Firmen-Kostensätze (Regel 92, E3; KA-0d)
            sk_cost::neues_projekt(&mut m, Some(c.library()));
            m
        }
        None => sk_model::Model::new(),
    };
    if ort != sk_math::sonne::Lage::GANDERKESEE {
        m.init_location(sk_model::Location {
            lat: Some(ort.breite),
            lon: Some(ort.laenge),
            north: None,
        });
    }
    if let Some(s) = ansicht_schatten::festschreiben(None, v.schatten) {
        m.set_sun(s);
    }
    m
}

fn app(surface: Surface, screenshot: Option<String>) -> Result<(), String> {
    // `--musterprobe <ordner>` (B7): ohne GL-Kontext Exit-Code 2
    let probe = std::env::args().skip_while(|a| a != "--musterprobe").nth(1);
    let gl = match Gl::load(|name| surface.gl_proc(name)) {
        Err(e) if probe.is_some() => {
            eprintln!("musterprobe: kein GL-Kontext: {e}");
            std::process::exit(2);
        }
        r => r?,
    };
    // Farbschema aus %APPDATA%\Skizzeo\einstellungen.txt (fehlt sie: dunkel)
    let mut settings = settings::Settings::new(
        std::env::args(),
        std::env::var_os("APPDATA").map(std::path::PathBuf::from),
    );
    let theme = settings.load();
    for h in &settings.hints {
        eprintln!("Einstellungen: {h}");
    }
    // Firmenkatalog (K2): neue Projekte bekommen seine Typen. Der
    // Bildvergleich merkt sich keine gesehene Fassung.
    if screenshot.is_some() {
        catalog::remember_hints(false);
    }
    let (company, hints) = match settings.company_place() {
        Some((p, standard)) => {
            let (c, h) = catalog::Company::laden(&p, standard);
            (Some(c), h)
        }
        None => (None, Vec::new()),
    };
    let (quiet, hints): (Vec<_>, Vec<_>) = hints.into_iter().partition(|h| catalog::quiet(h));
    if !hints.is_empty() && screenshot.is_none() {
        surface.message(&meldungen(&hints), false);
    }
    let mut scene = Scene::with_model(new_model(company.as_ref(), settings.vorgaben));
    scene.set_theme(&theme);
    let mut renderer = match Renderer::new(gl, style(&theme.env)) {
        Err(e) if probe.is_some() => {
            eprintln!("musterprobe: kein GL-Kontext: {e}");
            std::process::exit(2);
        }
        r => r?,
    };
    if let Some(dir) = probe {
        let code = musterprobe::run(std::path::Path::new(&dir), &theme, |l, m, v, s| {
            renderer.pattern_probe(l, m, v, s)
        });
        std::process::exit(code);
    }
    let cam = start_camera();
    let (w, h) = surface.size();
    // Außenwand-Aufbau aus der Bibliothek: Vorschau beim Zeichnen und Anzeige im Paneel
    let exterior = scene.model().defaults().exterior_wall;
    let mut tool = WallTool::new();
    tool.layers = scene.model().wall_layers(exterior);
    let mut ui = Ui::new(surface.scale(), &theme);
    let title = TitleBar::new(surface.scale());
    ui.top = title.height();
    ui.fit(surface.scale(), w, h);
    ui.wall_layers = layer_rows(scene.model(), exterior);
    let doc = Document::new(scene.model().revision());
    let wheel = wheel::Wheel::new(&theme, screenshot.is_some());
    let mut a = App {
        renderer,
        scene,
        doc,
        title,
        ui,
        cam3d: cam.clone(),
        cam3d_empty: true,
        cam,
        fitted: None,
        nav: Navigation::default(),
        tool,
        edit: WallEdit::default(),
        sect: Sections::default(),
        nord: Default::default(),
        sonne: Default::default(),
        sel: Selection::default(),
        props_key: None,
        snaps_key: None,
        mouse_at: None,
        tip: None,
        notice: None,
        menu: menu::FileMenu::default(),
        menu_dirty: false,
        shortcuts: menu::Shortcuts::default(),
        save_dlg: None,
        after_save: None,
        projektdaten: None,
        recent: settings.recent.clone(),
        recent_on: settings.path.is_some(),
        quit: false,
        prefs: None,
        prefs_mem: prefs::Memory::default(),
        prefs_dirty: false,
        pattern_preview: None,
        prefs_popup_dirty: false,
        settings,
        company,
        catalog: None,
        materials: None,
        verwaltung: None,
        admin: false,
        mat_more: false,
        tool_type: [None, None],
        type_menu: None,
        type_menu_dirty: false,
        tool_chip_key: None,
        w,
        h,
        overlay_dirty: true,
        layout_dirty: false,
        props_dirty: false,
        dirty_buttons: Vec::new(),
        dirty_title: Vec::new(),
        title_img: None,
        theme,
        redraw: true,
        mesh_dirty: true,
        live_dirty: true,
        live_runs: Vec::new(),
        preview_shown: true,
        looks_key: None,
        bond_wait: Vec::new(),
        bond_fades: Vec::new(),
        mark_keys: [None; MARKS],
        nord_bild: None,
        sonne_bild: None,
        ecken: Vec::new(),
        ansicht_schatten: Default::default(),
        schatten_bild: None,
        griff_cache: None,
        ecken_stand: 0,
        wuerfel: false,
        licht: None,
        room_keys: Default::default(),
        panel_px: Vec::new(),
        wheel,
        wheel_view: wheel_view::WheelView::new(OVERLAY_WHEEL),
        clock: Instant::now(),
        wheel_acc: 0.0,
        wheel_press: false,
        view_shift: 0.0,
        view_squeeze: (0.0, 1.0, 0.0),
        turn: None,
        picking: picking::Picking::default(),
        quantity: quantity::QuantityWindow::new(),
        hover_from_list: false,
        tree: tree_panel::TreePanel::new(),
        hover_from_tree: false,
        in_tree: false,
        tree_picked: false,
        tree_primary: None,
        tree_props: false,
        hints_seen: Default::default(),
        fly: None,
        quantity_busy: false,
        quantity_wanted: false,
        auto_switch: std::env::args()
            .skip_while(|a| a != "--geschosswechsel")
            .nth(1)
            .and_then(|n| n.parse().ok())
            .map(|n| (n, true)),
        link_view: link_view::LinkView::new(OVERLAY_CHIPS),
        chips: Vec::new(),
        chip_hover: None,
        drag_notice: false,
        flush_anim: None,
        pick: None,
        flush_keep: None,
        pick_label: None,
        input_pill: None,
        post_at: None,
        input_px: None,
        input_swap: None,
        input_error: None,
        save_fail: autosave::FailNotice::default(),
        queued_command: None,
        hint: None,
        hint_dirty: false,
        hint_alpha: 0.0,
        context: None,
        context_dirty: false,
        confirm: None,
        confirm_dirty: false,
        erase_fade: None,
        erase_flash: None,
        autosave: autosave::folder()
            .filter(|_| screenshot.is_none())
            .map(|d| autosave::AutoSave::new(d).in_background()),
        card: None,
        card_dirty: false,
        card_fade: None,
        card_at: (0, 0),
        backups: Vec::new(),
        help: help::HelpCard::default(),
        help_img: None,
        help_ground: None,
        help_fade: None,
        help_swap: None,
        help_px: None,
        help_press_elsewhere: false,
        frame_measure: None,
    };
    // `--hilfe <thema>` bzw. `--hilfe-liste`: Karte offen (Bildvergleich)
    let help_arg = std::env::args().skip_while(|x| x != "--hilfe").nth(1);
    if let Some(t) = help_arg.as_deref().and_then(help::Topic::from_id) {
        a.help.open_with(t);
    }
    if std::env::args().any(|x| x == "--hilfe-liste") {
        a.help.set_open(true);
        a.help.toggle_list();
    }
    if screenshot.is_none() {
        a.notice = quiet.into_iter().next().map(|text| Notice {
            text,
            since: None,
            rect: (0.0, 0.0, 0.0, 0.0),
            time: NOTICE_TIME,
            catalog: true,
            error: false,
        });
        if a.notice.is_none() && a.renderer.pattern_error().is_some() {
            // Review 3q: Flächen ohne Muster, das Programm läuft weiter
            a.notice = Some(Notice {
                text: meldung::Meldung::satz(
                    "Muster in 3D aus: Der Grafiktreiber übersetzt die Muster nicht.",
                ),
                since: None,
                rect: (0.0, 0.0, 0.0, 0.0),
                time: NOTICE_TIME,
                catalog: false,
                error: false,
            });
        }
    }
    a.upload_model();
    a.sync_levels();
    // `skizzeo.exe haus.szo`: Projekt gleich öffnen
    if let Some(path) = document::path_from_args(std::env::args()) {
        a.open_path(&surface, path);
    }
    // `--ausblenden`, `--isolieren`, `--gelaende-aus`: Sichtbarkeit für
    // Bildvergleiche (Paket 3); ersetzt die aus der Datei
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|x| {
        matches!(
            x.as_str(),
            "--ausblenden" | "--isolieren" | "--gelaende-aus"
        )
    }) {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        match cli::visibility_args(&refs, a.scene.model()) {
            Ok(v) => {
                if a.scene.set_visibility(v) {
                    a.upload_model();
                }
            }
            Err(e) => eprintln!("{e}"),
        }
    }
    // Baumpanel für Bildvergleiche (Paket 4): `--karte bauteil`,
    // `--sperren AW-005,…`, `--waehlen AW-005`, `--aufklappen Name,…`,
    // `--ueberfahren Name`, `--baum-isolieren Name` (Namensanfang einer
    // Zeile), `--sperrhinweis AW-005`
    let arg = |k: &str| {
        std::env::args()
            .skip_while(|a| a != k)
            .nth(1)
            .map(|v| v.split(',').map(str::to_string).collect::<Vec<_>>())
    };
    let numbers = |a: &App, v: &[String]| -> Vec<sk_model::ElementId> {
        v.iter()
            .filter_map(|n| {
                a.scene
                    .model()
                    .elements()
                    .iter()
                    .find(|(_, e)| e.number == *n)
                    .map(|(id, _)| id)
            })
            .collect()
    };
    if let Some(t) = arg("--karte") {
        let tab = match t.first().map(String::as_str) {
            Some("bauteil") => sk_model::tree::Tab::Kind,
            Some("gewerk") => sk_model::tree::Tab::Trade,
            _ => sk_model::tree::Tab::Tree,
        };
        a.tree.tab = tab;
    }
    if let Some(v) = arg("--sperren") {
        let ids = numbers(&a, &v);
        a.scene.edit_model("Gesperrt", |m| {
            m.set_locked(&ids, true);
            true
        });
        a.upload_model();
    }
    if let Some(v) = arg("--waehlen") {
        a.picking.selected = numbers(&a, &v);
        a.sync_props();
    }
    a.tree.cli_open = arg("--aufklappen").unwrap_or_default();
    a.tree.cli_hover = arg("--ueberfahren").and_then(|v| v.into_iter().next());
    a.tree.cli_isolate = arg("--baum-isolieren").and_then(|v| v.into_iter().next());
    // `--sperrhinweis AW-005,AW-001`: Sperre beim Ziehen von AW-001
    if let Some(ids) = arg("--sperrhinweis").map(|v| numbers(&a, &v)) {
        if let Some(&id) = ids.first() {
            a.show_locked(id, ids.get(1).map(|&w| (w, delete::Act::Drag)));
        }
        // Bleibt fürs Bildschirmfoto stehen
        if let Some(h) = a.hint.as_mut() {
            h.hold();
        }
    }
    // Fenster „Baustoffe …“ für Bildvergleiche (Paket 5): `--firmenkatalog
    // datei.szk`, `--baustoffe Namensanfang`, `--baustoffe-mehr`,
    // `--baustoffe-firma`
    if let Some(p) = std::env::args()
        .skip_while(|a| a != "--firmenkatalog")
        .nth(1)
    {
        a.company = Some(catalog::Company::laden(std::path::Path::new(&p), false).0);
    }
    if let Some(v) = arg("--baustoffe") {
        let name = v.join(",");
        let g = a
            .scene
            .model()
            .materials()
            .iter()
            .find(|(_, x)| x.name.starts_with(&name))
            .map(|(_, x)| x.guid);
        a.mat_more = std::env::args().any(|x| x == "--baustoffe-mehr");
        a.open_materials(g);
        if std::env::args().any(|x| x == "--baustoffe-firma") {
            if let Some(v) = a.materials.as_mut() {
                v.show_company();
            }
        }
    }
    // `--ansicht schnitt`: mit dieser Ansicht beginnen (Bildvergleiche);
    // `schnitt` zeigt Schnitt A, `schnitt-b` Schnitt B, dazu `--gespiegelt`
    // mit umgekehrtem Blick
    let start = std::env::args().skip_while(|a| a != "--ansicht").nth(1);
    let cut = match start.as_deref() {
        Some("schnitt") => Some(section::CUT_A),
        Some("schnitt-b") => Some(section::CUT_B),
        _ => None,
    };
    let start = if cut.is_some() {
        Some("schnitt".into())
    } else {
        start
    };
    if let Some(v) = start.and_then(|n| ViewKind::from_arg(&n)) {
        if let Some(i) = cut {
            a.scene.set_active_cut(i);
        }
        a.set_view(v);
        if let Some(i) = cut.filter(|_| std::env::args().any(|x| x == "--gespiegelt")) {
            a.sect.lines[i].ensure(&a.scene);
            a.sect.lines[i].mirror();
            a.scene.set_cut(i, a.sect.lines[i].cut());
            a.fit_camera();
            a.upload_model();
        }
    }
    // Nach einem Absturz die Sicherung anbieten (F-13); räumt alte still
    // weg. Nie bei Bildvergleichen und Zeitmessungen.
    if screenshot.is_none() && a.auto_switch.is_none() {
        if let Some(f) =
            autosave::folder().and_then(|d| autosave::start(&d, std::time::SystemTime::now()))
        {
            a.show_start_card(f);
        }
    }
    a.sync_caption(&surface);
    // Mengenfenster (F2, B7): gemerkte Lage, Breite aus dem Schema
    *surface.layout() = windows::read_settings(&a.settings.windows, &surface.monitors());
    a.quantity.grouping = windows::read_grouping(&a.settings.windows);
    a.quantity.blatt_wahl = a.settings.lv_blatt();
    a.quantity.karten.aktiv = windows::read_blatt(&a.settings.windows);
    // Baumpanel: Karte, zugeklappt, Grenze (Paket 4)
    let panel = a.settings.panel.clone();
    a.tree.load_settings(&panel);
    a.hints_seen = hints::seen(&panel);
    surface.layout().set_width_dip(a.theme.size.qto_window_w);
    if surface.layout().quantity_open() || std::env::args().any(|x| x == "--mengenfenster") {
        a.open_quantity(&surface);
    }
    // `--zeiten datei.csv`: Dauer jedes Bildes in Millisekunden mitschreiben
    let timing_path = std::env::args().skip_while(|a| a != "--zeiten").nth(1);
    let mut timing = timing_path.as_ref().map(|p| {
        let _ = std::fs::write(p, frame_time::HEADER);
        String::new()
    });
    let mut last_tick: Option<std::time::Instant> = None;

    loop {
        let mut tagged = Vec::new();
        if let Some((left, up)) = a.auto_switch {
            if !a.wheel.animating(a.now()) && a.ui.view == ViewKind::Plan {
                if left == 0 {
                    if let Some(log) = timing.as_mut() {
                        write_timing(&timing_path, log);
                    }
                    return Ok(());
                }
                // Wie nach einer Pause: Nachbargrundrisse vorbereitet
                a.scene.prepare_neighbor_plans();
                let up = if a.wheel.arrow_enabled(&a.scene, up, false) {
                    up
                } else {
                    !up
                };
                let t = a.now();
                a.wheel.click_arrow(&mut a.scene, up, false, t);
                a.auto_switch = Some((left - 1, up));
            }
        }
        if !a.redraw
            && !a.overlay_dirty
            && !a.nav.is_animating()
            && !a.wheel.animating(a.now())
            && a.auto_switch.is_none()
            && a.fly.is_none()
            && !a.scene.growing()
            && !a.scene.vis_animating()
            && a.erase_fade.is_none()
            && a.bond_fades.is_empty()
            && a.turn.is_none()
            && a.erase_flash.is_none()
            && a.card_fade.is_none()
            && a.flush_anim.is_none()
            && !a.tree.is_growing()
            && !a.tree.needs_paint()
        {
            // Leerlauf: Grundrisse der Nachbargeschosse vorbereiten, damit ein
            // Wechsel am Geschossbogen nichts neu rechnet (E18)
            if a.ui.view == ViewKind::Plan && a.scene.plans_pending() {
                a.scene.prepare_neighbor_plans();
            }
            let dlg_wait = match (&a.catalog, &a.materials) {
                (Some(c), _) => c.wait(&a.theme),
                (None, Some(v)) => v.wait(&a.theme),
                (None, None) => a.prefs.as_ref().and_then(|p| p.wait()),
            };
            let tip_wait = match (a.tip_wait(), a.pill_wait()) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            let wait = match (tip_wait, dlg_wait) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            let wait = match (wait, a.quantity_busy || !a.bond_wait.is_empty()) {
                (w, true) => Some(w.map_or(FRAME, |w| w.min(FRAME))),
                (w, false) => w,
            };
            let next = match wait {
                Some(d) => surface.wait_event_timeout(d),
                None => surface.wait_event().map(Some),
            };
            match next {
                Some(e) => tagged.extend(e),
                None => {
                    if let Some(log) = timing.as_mut() {
                        write_timing(&timing_path, log);
                    }
                    a.closing(&surface);
                    return Ok(());
                }
            }
        }
        while let Some(e) = surface.poll_event() {
            tagged.push(e);
        }
        let mut events = Vec::new();
        for (id, e) in tagged {
            match id {
                windows::WindowId::Main => events.push(e),
                windows::WindowId::Quantity => a.handle_quantity(e, &surface),
            }
        }
        coalesce_moves(&mut events);
        let t_events = Instant::now();
        a.scene.set_now(a.now());
        for e in events {
            if !a.handle(e, &surface) {
                if let Some(log) = timing.as_mut() {
                    write_timing(&timing_path, log);
                }
                a.closing(&surface);
                return Ok(());
            }
        }

        if std::mem::take(&mut a.quantity_wanted) {
            a.open_quantity(&surface);
        }
        // Automatisch sichern (F-13), nicht mitten im Ziehen
        if !a.edit.is_dragging() {
            let now = a.clock.elapsed();
            let outcome = a.autosave.as_mut().and_then(|s| {
                s.tick(a.scene.model(), &a.doc, now);
                s.take_outcome()
            });
            if let Some(ok) = outcome {
                let step = a.save_fail.attempt(ok, now);
                a.fail_notice(step);
            }
        }
        a.sync_wheel();
        if a.prefs.as_mut().is_some_and(|p| p.tick()) {
            a.prefs_dirty = true;
        }
        let theme = &a.theme;
        if a.catalog.as_mut().is_some_and(|c| c.tick(theme)) {
            a.prefs_dirty = true;
        }
        if a.materials.as_mut().is_some_and(|v| v.tick(theme)) {
            a.prefs_dirty = true;
        }
        // Wände wachsen nach einem Typwechsel (K3b)
        if a.scene.growing() {
            let t = a.now();
            a.mesh_dirty |= a.scene.grow_tick(t);
            a.redraw = true;
        }
        // Verbandstabelle fertig: einblenden
        a.bonds_busy();
        // Ausblenden, Einblenden, Isolieren gleiten in `anim_ms`
        if a.scene.vis_animating() {
            let t = a.now();
            a.mesh_dirty |= a.scene.vis_tick(t);
            a.redraw = true;
        }
        a.sync_quantity(&surface);
        a.sync_tool_chip();
        a.sync_tip();
        a.sync_measure();
        a.sync_help();
        a.sync_frame_measure();
        if let Some(id) = a.scene.take_locked() {
            a.show_locked(id, None);
            a.upload_model();
        }
        a.sync_notice();
        a.sync_erase();
        a.sync_title_state();
        a.sync_tree();
        if a.menu_dirty && !a.overlay_dirty && a.w > 0 {
            a.paint_menu();
        }
        if a.overlay_dirty && a.w > 0 {
            a.paint_overlays(&surface);
        } else if a.layout_dirty && a.w > 0 {
            a.relayout_overlays(&surface);
        }
        if a.props_dirty && a.w > 0 {
            a.paint_props();
        }
        if a.prefs_dirty && a.w > 0 {
            a.paint_prefs();
        } else if a.prefs_popup_dirty && a.w > 0 {
            a.paint_prefs_popup();
        }
        if a.type_menu_dirty && a.w > 0 {
            a.paint_type_menu();
        }
        if a.confirm_dirty && a.w > 0 {
            a.paint_confirm();
        }
        if (a.card_dirty || a.card_fade.is_some()) && a.w > 0 {
            a.paint_card();
        }
        if a.context_dirty && a.w > 0 {
            a.paint_context();
        }
        if a.w > 0 {
            a.paint_buttons(&surface);
        }
        let cursor = match (&a.prefs, &a.catalog) {
            _ if a.projektdaten.as_ref().is_some_and(|d| d.ueber_feld()) => {
                sk_platform::Cursor::IBeam
            }
            _ if a.projektdaten.is_some() => sk_platform::Cursor::Arrow,
            (Some(p), _) => p.cursor(),
            (None, Some(c)) => c.cursor(),
            (None, None) if a.materials.is_some() => a
                .materials
                .as_ref()
                .map_or(sk_platform::Cursor::Arrow, |v| v.cursor()),
            (None, None) if a.verwaltung.is_some() => a
                .verwaltung
                .as_ref()
                .map_or(sk_platform::Cursor::Arrow, |v| v.cursor()),
            (None, None) if a.wheel_hand() => sk_platform::Cursor::Hand,
            (None, None) if a.hint.as_ref().is_some_and(|h| h.link_hover) => {
                sk_platform::Cursor::Hand
            }
            (None, None) if a.chip_hover.is_some() => sk_platform::Cursor::Hand,
            (None, None) if a.pick.as_ref().is_some_and(|p| p.hover.is_some()) => {
                sk_platform::Cursor::Hand
            }
            (None, None) if a.sect.over_mark() => sk_platform::Cursor::Hand,
            (None, None) if a.nord.over().is_some() => sk_platform::Cursor::Hand,
            (None, None) if a.sonne.ueber_sonne || a.sonne.ueber_schatten || a.sonne.is_busy() => {
                sk_platform::Cursor::Hand
            }
            (None, None) if a.sonne.hover.is_some() => sk_platform::Cursor::Hand,
            (None, None) if a.ansicht_schatten.hover.is_some() => sk_platform::Cursor::Hand,
            (None, None) => a.ui.cursor(),
        };
        surface.set_cursor(cursor);

        if let Some(f) = &a.fly {
            match f.at(Instant::now()) {
                Some(c) => a.cam = c,
                None => {
                    a.cam = f.to.clone();
                    a.fly = None;
                }
            }
            a.redraw = true;
            a.refresh_cursor();
        }
        a.step_flush();
        if a.nav.is_animating() {
            let now = std::time::Instant::now();
            let dt = last_tick.map_or(1.0 / 60.0, |t| (now - t).as_secs_f64().min(0.1));
            last_tick = Some(now);
            a.nav.tick(&mut a.cam, dt);
            a.refresh_cursor();
        } else {
            last_tick = None;
        }

        let th = a.top();
        if a.redraw && a.w > 0 && a.h > th {
            let t_handled = Instant::now();
            a.build_mesh();
            let t_mesh = Instant::now();
            let drawing = a.ui.view != ViewKind::Persp;
            let preview = match (a.tool.preview(), a.ui.view) {
                (Some(c), ViewKind::Plan) => {
                    Some(scene::mesh_of(&c.solid_cut_at(a.scene.plan_cut())))
                }
                (Some(c), _) => Some(scene::mesh_of(&c.solid())),
                (None, _) => None,
            };
            // Leere Vorschau nur einmal hochladen
            if preview.is_some() || a.preview_shown {
                a.preview_shown = preview.is_some();
                a.renderer
                    .set_mesh(MESH_PREVIEW, &preview.unwrap_or_default());
            }
            let (vw, vh) = (a.w as f64, (a.h - th) as f64);
            let scale = a.title.scale;
            let mut helpers = Vec::new();
            let mut nord_bild = None;
            match a.ui.view {
                ViewKind::Plan => {
                    helpers.extend(a.sect.helpers(&a.scene, &a.cam, vh, scale, &a.theme))
                }
                ViewKind::Persp => {}
                // Gelände ausgeblendet: keine Geländelinie (Paket 3)
                _ if a.scene.terrain_hidden() => {}
                v => helpers.extend(ground_line(
                    a.side_like(v),
                    a.scene.bounds(),
                    scale,
                    a.scene.table(),
                )),
            }
            let plane = a.plane();
            // Hover aus der Mengenliste: leuchtender Umriss mit weichem Schein
            if a.hover_from_list {
                let hovered: Vec<_> = a
                    .picking
                    .hovered()
                    .filter(|h| !a.picking.is_selected(*h))
                    .collect();
                for &h in &hovered {
                    helpers.extend(selection::hover_glow(
                        &a.scene, h, a.ui.view, plane, scale, &a.theme,
                    ));
                }
                for h in hovered {
                    helpers.extend(selection::hover_helpers(
                        &a.scene,
                        Some(h),
                        a.ui.view,
                        plane,
                        scale,
                        &a.theme,
                    ));
                }
            }
            let (k, glowing) = a.scene.grow_glow();
            for &h in glowing {
                helpers.extend(selection::fading_glow(
                    &a.scene, h, a.ui.view, plane, scale, &a.theme, k,
                ));
            }
            // Abgelehnte Bauteile leuchten einmal, das ganze Gebäude, solange
            // die Rückfrage steht (V?-9)
            if let Some((t0, ids)) = &a.erase_flash {
                let ms = a.theme.size.flash_ms;
                let t = a.now().saturating_sub(*t0) as f32;
                if t < ms {
                    let k = scene::GROW_GLOW * (1.0 - t / ms);
                    for &h in ids {
                        helpers.extend(selection::fading_glow(
                            &a.scene, h, a.ui.view, plane, scale, &a.theme, k,
                        ));
                    }
                }
            }
            if let Some(c) = &a.confirm {
                for &h in &c.parts {
                    helpers.extend(selection::fading_glow(
                        &a.scene,
                        h,
                        a.ui.view,
                        plane,
                        scale,
                        &a.theme,
                        scene::GROW_GLOW,
                    ));
                }
            }
            for &id in &a.picking.selected {
                helpers.extend(selection::helpers(
                    &a.scene, id, a.ui.view, plane, scale, &a.theme,
                ));
            }
            // Gewählte OG-Wand mit Partner: Außenkante der Wand darunter auf
            // Höhe ihres Fußes (OK Trenndecke), durchgehend über allem. Beim
            // Ziehen zeichnet sie `edit.helpers`.
            if a.ui.view == ViewKind::Persp && !a.edit.is_dragging() {
                let m = a.scene.model();
                for &id in &a.picking.selected {
                    let own = m.segment_of(id);
                    let below = m.wall_below(id).and_then(|w| m.segment_of(w));
                    let foot = |(run, k): (sk_model::RunId, usize)| {
                        a.scene.foot(run).and_then(|f| f.get(k).copied())
                    };
                    if let (Some((o, _)), Some((p, q))) = (own.and_then(foot), below.and_then(foot))
                    {
                        let up = |v: sk_math::Vec3| sk_math::vec3(v.x, v.y, o.z);
                        helpers.push(wall_edit::partner_line(up(p), up(q), scale, &a.theme));
                    }
                }
            }
            // Feld „Dämmung“ im Fokus oder unter der Maus: die Untersicht-
            // dämmung der Decke leuchtet wie beim Hover aus der Mengenliste
            let soffit_field = Some(ui::Id::Field(ui::Field::Soffit));
            if a.ui.edit.as_ref().map(|e| ui::Id::Field(e.field)) == soffit_field
                || a.ui.hover == soffit_field
            {
                let m = a.scene.model();
                let ud = a
                    .picking
                    .selected
                    .iter()
                    .find_map(|&id| match m.element(id)?.kind {
                        sk_model::ElementKind::Floor(_) => m.soffit_of(id),
                        sk_model::ElementKind::SoffitInsulation { .. } => Some(id),
                        _ => None,
                    });
                if let Some(ud) = ud {
                    helpers.extend(selection::hover_glow(
                        &a.scene, ud, a.ui.view, plane, scale, &a.theme,
                    ));
                    helpers.extend(selection::hover_helpers(
                        &a.scene,
                        Some(ud),
                        a.ui.view,
                        plane,
                        scale,
                        &a.theme,
                    ));
                }
            }
            if let Some(z) = a.ui.level_drag_z() {
                let v = a.side_like(a.ui.view);
                helpers.extend(level_guide(v, a.scene.bounds(), z, scale, &a.theme));
            }
            helpers.extend(a.edit.helpers(&a.scene, &a.cam, scale, !drawing, &a.theme));
            if let Some(p) = &a.pick {
                helpers.extend(p.helpers(&a.scene, a.ui.view, plane, scale, &a.theme));
            }
            helpers.extend(a.tool.helpers(&a.cam, scale, &a.theme));
            if a.tool_allowed() {
                let (width, ink) = a.scene.table().section_line;
                let st = a.nord_stand(vh, scale as f64);
                let hot = a.theme.interact.drag;
                nord_bild = a
                    .nord
                    .bild(st, &a.cam, (vw, vh), scale, ink, hot, width.max(1.5));
            }
            // Sonnenstand (S4): Bahnen und Scheibe am Himmel, in 3D
            let himmel = a.himmel();
            if let Some(h) = &himmel {
                let heiss =
                    a.sonne.ueber_sonne || (a.sonne.is_busy() && a.sonne.am_schatten().is_none());
                helpers.extend(sonne_view::helpers(h, heiss, scale));
                // Griff an der Schattenspitze (S6)
                let griff = a.griff();
                let d = a
                    .sonne_an()
                    .and_then(|s| sonne_view::zur_sonne(a.scene.model().location(), &s));
                helpers.extend(sonne_view::schatten_helpers(
                    &a.sonne,
                    griff.as_ref(),
                    d,
                    scale,
                ));
            }
            a.renderer.set_helpers(&helpers);

            // Kettensymbole an den gestapelten Wänden (Grundriss und 3D)
            let chips_on =
                matches!(a.ui.view, ViewKind::Plan | ViewKind::Persp) && !a.tool.is_active();
            a.chips = if chips_on {
                link_view::chips(&link_view::Want {
                    scene: &a.scene,
                    cam: &a.cam,
                    w: vw,
                    h: vh,
                    top: th as f64,
                    scale: scale as f64,
                    plan_z: a.plan_z(),
                    band: a.edit.active_wall(),
                    hover: a.chip_hover,
                    keep_linked: a.flush_keep,
                })
            } else {
                Vec::new()
            };
            a.link_view.show(
                &mut a.renderer,
                &a.chips,
                &a.theme,
                a.theme.rev,
                scale as f64,
            );

            // Endsymbole der Schnittlinie als kleine Bilder an den Linienenden
            let marks = if a.ui.view == ViewKind::Plan {
                a.sect.marks(&a.scene, &a.cam, vw, vh)
            } else {
                Vec::new()
            };
            for i in 0..MARKS {
                match marks.get(i) {
                    Some((k, m)) => {
                        // Bild nur neu zeichnen, wenn es sich ändert; sonst nur verschieben
                        let line = &a.sect.lines[*k];
                        let revs = (a.theme.rev, a.scene.table().rev);
                        let key = (*k, m.left, line.flip, line.is_busy(), scale.to_bits(), revs);
                        let (ax, ay) = line.mark_anchor(m.left, scale);
                        let x = (m.x - ax as f64).round() as i32;
                        let y = (m.y + th as f64 - ay as f64).round() as i32;
                        if a.mark_keys[i] == Some(key) {
                            a.renderer.move_overlay(OVERLAY_MARKS + i, x, y);
                        } else {
                            let (c, _, _) =
                                line.paint_mark(&a.scene, &a.theme, &a.ui.fonts, m.left, scale);
                            let px = c.to_premul_rgba8();
                            a.renderer.set_overlay(
                                OVERLAY_MARKS + i,
                                x,
                                y,
                                c.width as u32,
                                c.height as u32,
                                &px,
                            );
                            a.mark_keys[i] = Some(key);
                        }
                    }
                    None => {
                        if a.mark_keys[i].take().is_some() {
                            a.renderer.set_overlay(OVERLAY_MARKS + i, 0, 0, 0, 0, &[]);
                        }
                    }
                }
            }

            // Nordpfeil als Bild: nur neu malen, wenn es sich ändert
            if nord_bild != a.nord_bild {
                match nord_bild.as_ref().and_then(|b| b.malen()) {
                    Some((c, x, y)) => {
                        let px = c.to_premul_rgba8();
                        let (w, h) = (c.width as u32, c.height as u32);
                        a.renderer
                            .set_overlay(OVERLAY_NORD, x, y + th as i32, w, h, &px);
                    }
                    None => a.renderer.set_overlay(OVERLAY_NORD, 0, 0, 0, 0, &[]),
                }
                a.nord_bild = nord_bild;
            }

            // Sonnenstand (S4): Leiste, Würfel ohne Gebäude, Licht der Sonne
            let sun = a.sonne_an();
            let leiste = himmel
                .as_ref()
                .zip(sun)
                .map(|(h, sun)| sonne_view::LeistenBild {
                    sun,
                    unter: h.sonne.is_none(),
                    eingabe: a.sonne.eingabe.clone(),
                    hover: a.sonne.hover,
                    vw: vw as u32,
                    scale: scale.to_bits(),
                });
            if leiste != a.sonne_bild {
                match &leiste {
                    Some(b) => {
                        let (c, x, y) = sonne_view::leiste_malen(b, &a.ui.fonts, &a.theme);
                        let px = c.to_premul_rgba8();
                        let (w, h) = (c.width as u32, c.height as u32);
                        a.renderer
                            .set_overlay(OVERLAY_SONNE, x, y + th as i32, w, h, &px);
                    }
                    None => a.renderer.set_overlay(OVERLAY_SONNE, 0, 0, 0, 0, &[]),
                }
                a.sonne_bild = leiste;
            }
            let wuerfel = himmel.is_some() && a.scene.bounds().is_none();
            if wuerfel != a.wuerfel {
                let netz = if wuerfel {
                    sonne_view::wuerfel_netz()
                } else {
                    Default::default()
                };
                a.renderer.set_mesh(MESH_WUERFEL, &netz);
                a.wuerfel = wuerfel;
            }
            let licht = sun.and_then(|s| sonne_view::licht(a.scene.model().location(), &s));
            if licht != a.licht {
                let fest = style(&a.theme.env).light;
                a.renderer.set_light(licht.unwrap_or(fest));
                a.licht = licht;
            }
            // Schatten (S5) nur in 3D mit Himmel, ab 2° Sonnenhöhe
            let loc = a.scene.model().location();
            let sonnenlicht = himmel
                .as_ref()
                .and(sun)
                .and_then(|s| sonne_view::sonnenlicht(loc, &s));
            a.renderer.set_sun(sonnenlicht);
            // Beim Ziehen an Sonne oder Schatten je Bild eine neue Karte,
            // kleiner, wenn die volle zu lange braucht (S6)
            a.renderer.set_shadow_draft(a.sonne.is_busy());
            // Schatten der Ansichten (S7): Licht der Ansicht, Zahnrad und Feld
            let wahl = a.ansicht_wahl();
            let papier = wahl.and_then(|(_, vs)| {
                let d = ansicht_schatten::licht(a.ui.view, vs, loc, a.ansicht_sonne())?;
                let (_, tinte) = a.scene.table().pattern;
                Some(ansicht_schatten::papier(
                    vs,
                    d,
                    [tinte[0], tinte[1], tinte[2]],
                    a.theme.px_per_mm,
                    scale,
                ))
            });
            a.renderer.set_paper_shade(papier);
            let bild = wahl.map(|(i, vs)| ansicht_schatten::Bild {
                vs,
                eigen: a.scene.model().view_shade_own(i).is_some(),
                offen: a.ansicht_schatten.offen,
                hover: a.ansicht_schatten.hover,
                sonne_ok: ansicht_schatten::sonne_waehlbar(loc),
                zu_tief: vs.light == sk_model::ShadeLight::Sun
                    && ansicht_schatten::sonne_waehlbar(loc)
                    && ansicht_schatten::sonne_zu_tief(loc, a.ansicht_sonne()),
                rechts: a.ui.rect(Panel::Views, a.w, th).x.to_bits(),
                scale: scale.to_bits(),
            });
            if bild != a.schatten_bild {
                match &bild {
                    Some(b) => {
                        let (c, x, y) = ansicht_schatten::malen(b, &a.ui.fonts, &a.theme);
                        let px = c.to_premul_rgba8();
                        let (w, h) = (c.width as u32, c.height as u32);
                        a.renderer
                            .set_overlay(OVERLAY_ANSICHT, x, y + th as i32, w, h, &px);
                    }
                    None => a.renderer.set_overlay(OVERLAY_ANSICHT, 0, 0, 0, 0, &[]),
                }
                a.schatten_bild = bild;
            }

            // „Dachterrasse 13,22 m²“ auf der Terrasse, gedimmt wie eine
            // Raumangabe, nach der Platzregel (Review 3f K1): nie über Wand,
            // Attika oder Blech, 6 dip neben dem Kettensymbol
            let rooms: Vec<((f64, f64), String)> = if a.ui.view == ViewKind::Plan {
                let chips: Vec<terrace_label::Box> = a
                    .chips
                    .iter()
                    .filter(|k| !k.linked)
                    .map(|k| terrace_label::chip_box(k, scale as f64, th as f64))
                    .collect();
                // Nicht unter den Paneelen
                let panels: Vec<terrace_label::Box> =
                    a.ui.panel_rects(a.w, th)
                        .iter()
                        .map(|r| terrace_label::Box {
                            x: r.x as f64,
                            y: (r.y - th as f32) as f64,
                            w: r.w as f64,
                            h: r.h as f64,
                        })
                        .collect();
                let pad = terrace_label::PAD * scale as f64;
                terrace_label::labels(
                    &a.scene,
                    &a.cam,
                    vw,
                    vh,
                    scale as f64,
                    &a.ui.fonts,
                    &a.theme,
                    &chips,
                    &panels,
                )
                .into_iter()
                .filter_map(|(p, text)| {
                    let (b, _) = p.shown()?;
                    Some(((b.x + pad, b.y + pad), text))
                })
                .take(ROOMS)
                .collect()
            } else {
                Vec::new()
            };
            for i in 0..ROOMS {
                match rooms.get(i) {
                    Some(((x, y), text)) => {
                        let key = (text.clone(), scale.to_bits(), a.theme.rev);
                        let at =
                            |_: u32, _: u32| (x.round() as i32, (y + th as f64).round() as i32);
                        match &a.room_keys[i] {
                            Some((k, w, h)) if *k == key => {
                                let (px_x, px_y) = at(*w, *h);
                                a.renderer.move_overlay(OVERLAY_ROOMS + i, px_x, px_y);
                            }
                            _ => {
                                let c =
                                    terrace_label::paint(&a.ui.fonts, text, scale as f64, &a.theme);
                                let (w, h) = (c.width as u32, c.height as u32);
                                let (px_x, px_y) = at(w, h);
                                let px = c.to_premul_rgba8();
                                a.renderer
                                    .set_overlay(OVERLAY_ROOMS + i, px_x, px_y, w, h, &px);
                                a.room_keys[i] = Some((key, w, h));
                            }
                        }
                    }
                    None => {
                        if a.room_keys[i].take().is_some() {
                            a.renderer.set_overlay(OVERLAY_ROOMS + i, 0, 0, 0, 0, &[]);
                        }
                    }
                }
            }

            // Maßzahl am Weg der rückenden Wand (Zielwahl, E20)
            let label = a
                .pick
                .as_ref()
                .and_then(|p| p.path(&a.scene))
                .and_then(|(p, q, t)| a.cam.project((p + q) * 0.5, vw, vh).map(|at| (at, t)));
            match label {
                Some(((x, y), t)) => {
                    let key = (t, scale.to_bits(), a.theme.rev);
                    if a.pick_label.as_ref() != Some(&key) {
                        let c = flush_pick::paint_label(&a.ui.fonts, &key.0, scale, &a.theme);
                        let px = c.to_premul_rgba8();
                        a.renderer.set_overlay(
                            OVERLAY_PICK,
                            0,
                            0,
                            c.width as u32,
                            c.height as u32,
                            &px,
                        );
                        a.pick_label = Some(key);
                    }
                    let (w, h) = a.renderer.overlay_size(OVERLAY_PICK);
                    let x = (x - w as f64 * 0.5).round() as i32;
                    let y = (y + th as f64 - h as f64 * 0.5).round() as i32;
                    a.renderer.move_overlay(OVERLAY_PICK, x, y);
                }
                None => {
                    if a.pick_label.take().is_some() {
                        a.renderer.set_overlay(OVERLAY_PICK, 0, 0, 0, 0, &[]);
                    }
                }
            }

            a.paint_input_pill(vw, vh, th, scale);

            let mut view = a.cam.view(a.w, a.h - th);
            if drawing {
                view.paper = Some(a.scene.table().paper);
            }
            view.patterns = draw_table::pattern_mode(a.ui.view, a.theme.env.patterns_3d);
            // Geschosswechsel: der neue Grundriss gleitet an seinen Platz
            if a.view_shift != 0.0 {
                view = view.shifted(a.view_shift, (a.h - th) as f32);
            }
            // Schnittwechsel gleitet seitlich, Spiegeln wendet das Blatt
            let (dx, k, axis) = a.view_squeeze;
            if (dx, k) != (0.0, 1.0) {
                view = view.squeezed(dx, k, axis, a.w as f32);
            }
            let ground = if a.scene.terrain_hidden() {
                0.0
            } else {
                a.theme.env.ground_opacity
            };
            a.renderer.set_ground_opacity(ground);
            let alpha = a.scene.ghost_alpha(drawing);
            a.renderer.set_ghost(Some((MESH_GHOST, alpha)));
            a.renderer.draw(a.w, a.h, th, &view)?;
            if let Some(e) = a.renderer.take_shadow_error() {
                eprintln!("{e}");
            }
            // Tiefen-Durchgang auf der Grafikkarte, aus dem Bild davor
            let schatten_ms = a.renderer.take_shadow_pass_ms().map_or(0.0, |m| m.1);
            let shown_at = a.now();
            a.wheel.shown(shown_at);
            let t_draw = Instant::now();
            if let Some(path) = &screenshot {
                let px = a.renderer.read_pixels(a.w, a.h);
                std::fs::write(path, sk_paint::encode_png(a.w, a.h, &px))
                    .map_err(|e| format!("Bildschirmfoto: {e}"))?;
                return Ok(());
            }
            surface.swap_buffers(a.w, a.h);
            a.redraw = false;
            if let Some(log) = timing.as_mut() {
                let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
                let now = Instant::now();
                let t = frame_time::spalten(
                    ms(t_events, t_handled),
                    ms(t_handled, t_mesh),
                    schatten_ms,
                    ms(t_mesh, t_draw),
                    ms(t_draw, now),
                );
                log.push_str(&format!(
                    "{:.3};{:.3};{:.3};{:.3};{:.3};{:.3}\n",
                    t[0],
                    t[1],
                    t[2],
                    t[3],
                    t[4],
                    t.iter().sum::<f64>(),
                ));
                if log.len() > 4096 {
                    write_timing(&timing_path, log);
                }
            }
            if let Some(m) = a.frame_measure.as_mut() {
                let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
                let now = Instant::now();
                m.push(frame_time::spalten(
                    ms(t_events, t_handled),
                    ms(t_handled, t_mesh),
                    schatten_ms,
                    ms(t_mesh, t_draw),
                    ms(t_draw, now),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_platform::Modifiers;

    fn mv(x: f64) -> Event {
        Event::MouseMove {
            x,
            y: 0.0,
            mods: Modifiers::default(),
        }
    }

    /// Sonnenstand S8: Ein neues Projekt bekommt den Standardort als
    /// Bauort, ab Werk (Ganderkesee) keinen, die Datei bleibt dann ohne
    /// `[location]`. Die Schattenvorgabe gilt lebend für Ansichten ohne
    /// eigene Wahl; eine eigene Wahl bleibt, die Datei ändert sich nicht.
    #[test]
    fn s8_vorgaben_fuer_neue_projekte_und_ansichten() {
        use settings::vorgaben::Vorgaben;
        use sk_math::sonne::Lage;
        use sk_model::{ShadeLight, ViewShade};
        let mit_ort = |ort| Vorgaben {
            ort,
            ..Vorgaben::WERK
        };
        let werk = new_model(None, Vorgaben::WERK);
        assert!(werk.location().is_unset());
        assert!(!sk_model::szo::write(&werk).contains("[location]"));
        let muenchen = Lage {
            breite: 48.137,
            laenge: 11.575,
        };
        let m = new_model(None, mit_ort(muenchen));
        assert_eq!(m.location().lat, Some(48.137));
        assert_eq!(m.location().lon, Some(11.575));
        assert_eq!(m.location().north, None);
        assert!(sk_model::szo::write(&m).contains("[location] lat=48.137 lon=11.575\n"));
        assert_eq!(m.sun(), None);
        assert_eq!(Scene::with_model(m).undo_label(), None, "ohne Schritt");

        // Schatten: Vorgabe ändern, Projekt ohne eigene Wahl folgt
        // Vorgabe „Sonne“: [sun] heute 12:00, System aus (§8 13:00 d)
        let sonne = new_model(
            None,
            Vorgaben {
                schatten: ViewShade {
                    light: ShadeLight::Sun,
                    ..ViewShade::WERK
                },
                ..Vorgaben::WERK
            },
        );
        let s = sonne.sun().expect("[sun]");
        assert!(!s.on && s.minutes == 12 * 60);
        assert_eq!(Scene::with_model(sonne).undo_label(), None);
        let mut p = new_model(None, Vorgaben::WERK);
        let eigen = ViewShade {
            light: ShadeLight::FrontRight,
            ..ViewShade::WERK
        };
        p.set_view_shade(1, eigen, ViewShade::WERK);
        let datei = sk_model::szo::write(&p);
        let rev = p.revision();
        let firma = ViewShade {
            hatch: true,
            ..ViewShade::WERK
        };
        assert_eq!(p.view_shade(0, firma), firma);
        assert_eq!(p.view_shade(1, firma), eigen);
        assert_eq!(sk_model::szo::write(&p), datei);
        assert_eq!(p.revision(), rev);
    }

    #[test]
    fn mausbewegungen_werden_gebuendelt() {
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x: 2.0,
            y: 0.0,
            mods: Modifiers::default(),
        };
        let mut ev = vec![
            mv(1.0),
            mv(2.0),
            down,
            mv(3.0),
            mv(4.0),
            mv(5.0),
            Event::Redraw,
        ];
        coalesce_moves(&mut ev);
        assert_eq!(ev, vec![mv(2.0), down, mv(5.0), Event::Redraw]);
    }
}

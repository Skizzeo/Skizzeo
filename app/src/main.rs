//! Skizzeo – 3D-Gebäudemodellierer.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(test)]
mod abnahme;
mod camera;
mod document;
mod draw_table;
mod menu;
mod nav;
#[cfg(test)]
mod perf;
mod picking;
mod prefs;
mod quantity;
mod scene;
mod schedule_view;
mod section;
mod selection;
mod settings;
mod ui;
mod wall_edit;
mod wall_tool;
mod wheel;
mod wheel_view;
mod windows;

use camera::Camera;
use document::Document;
use draw_table::DrawTable;
use menu::Command;
use nav::Navigation;
use scene::Scene;
use section::SectionLine;
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
    surface: &Surface,
) {
    if settings.path.is_some() {
        settings.recent = recent.clone();
        // Lage des Mengenfensters (F2)
        settings.windows = windows::write_settings(&surface.layout());
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
/// gezogener Wandzug.
const MESH_MODEL: usize = 0;
const MESH_PREVIEW: usize = 1;
const MESH_LIVE: usize = 2;
/// Bildabstand für Animationen im Mengenfenster (das Hauptfenster läuft mit vsync).
const FRAME: std::time::Duration = std::time::Duration::from_millis(16);

// Oberflächenbilder in Zeichenreihenfolge: Endsymbole unter den Paneelen,
// das Abdunkeln hinter dem Dialog über „Ansichten“, „Eigenschaften“ und
// „Werkzeuge“, unter „Geschosse“ (E16), die Titelleiste ganz oben.
/// Endsymbole der Schnittlinie im Grundriss (zwei Plätze).
const OVERLAY_MARKS: usize = 0;
const OVERLAY_VIEWS: usize = 2;
/// Paneel „Eigenschaften“.
const OVERLAY_PROPS: usize = 3;
const OVERLAY_TOOLS: usize = 4;
const OVERLAY_SCRIM: usize = 5;
/// Paneel „Geschosse“.
const OVERLAY_LEVELS: usize = 6;
/// Dialog „Gebäude erstellen“.
const OVERLAY_DIALOG: usize = 7;
const OVERLAY_TITLE: usize = 8;
/// Platz 9 ist frei (früher der Hinweis an der Maus).
/// Dateimenü (E17), darüber die Nachfrage „Änderungen speichern?“ mit
/// Abdunkeln.
const OVERLAY_MENU: usize = 10;
const OVERLAY_SAVE_SCRIM: usize = 11;
const OVERLAY_SAVE: usize = 12;
/// Einstellungsfenster (E5) und sein Aufklapper (Auswahlliste, Farbwähler,
/// Nachfrage).
const OVERLAY_PREFS: usize = 13;
const OVERLAY_PREFS_POPUP: usize = 14;
/// Platz 15 ist frei. Geschossbogen im Grundriss (E18): Bogen, Aufleuchten,
/// Schilder und Hinweis an der Spitze.
const OVERLAY_WHEEL: usize = 16;
/// Hinweis an der Maus, über allem.
const OVERLAY_TIP: usize = OVERLAY_WHEEL + wheel_view::SLOTS;

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
    let (lo, hi) = bounds.unwrap_or((vec3(-2000.0, -2000.0, 0.0), vec3(12000.0, 10000.0, 3500.0)));
    let (yaw, pitch) = view_direction(v);
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
    Camera::parallel(center, yaw, pitch, half)
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
type MarkKey = (bool, bool, u32, (u64, u64));

struct App {
    renderer: Renderer,
    scene: Scene,
    /// Datei des Projekts und gespeicherter Stand.
    doc: Document,
    title: TitleBar,
    ui: Ui,
    cam: Camera,
    /// Letzte 3D-Kamera, um aus den Parallelansichten zurückzukehren.
    cam3d: Camera,
    /// Die gemerkte 3D-Kamera stammt aus einer Zeit ohne Modell.
    cam3d_empty: bool,
    nav: Navigation,
    tool: WallTool,
    edit: WallEdit,
    /// Schnittlinie (im Grundriss verschiebbar) für die Ansicht „Schnitt“.
    sect: SectionLine,
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
    looks_key: Option<(u64, u64, u32)>,
    /// Zuletzt hochgeladene Endsymbole der Schnittlinie (links, hervorgehoben, Skalierung).
    mark_keys: [Option<MarkKey>; 2],
    /// Letzte Mausposition im Fenster (Pixel).
    mouse_at: Option<(f64, f64)>,
    /// Hinweis an der Maus: erscheint nach [`TIP_DELAY`] Ruhe über derselben Stelle.
    tip: Option<Tip>,
    /// Dateimenü am Logo, Tastenkürzel, Nachfrage „Änderungen speichern?“
    /// und der Befehl, der nach der Antwort folgt (E17).
    menu: menu::FileMenu,
    menu_dirty: bool,
    shortcuts: menu::Shortcuts,
    save_dlg: Option<menu::SaveDialog>,
    after_save: Option<Command>,
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
    /// Einstellungsdatei (Ort, Stand beim Laden).
    settings: settings::Settings,
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
    /// Gemeinsamer Hover- und Auswahlzustand beider Fenster (F2).
    picking: picking::Picking,
    /// Mengenfenster (F2, B7).
    quantity: quantity::QuantityWindow,
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
}

/// Hinweis an der Maus (Text, Lage, seit wann gewünscht, schon sichtbar).
struct Tip {
    text: String,
    at: (f64, f64),
    since: Instant,
    shown: bool,
}

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

    /// Erzeugt die vorgemerkten Netze. Beim Ziehen liegt der gezogene Wandzug in
    /// einem eigenen Live-Netz; nur dieses wird dann je Bild neu erzeugt und
    /// hochgeladen, das ruhende Netz bleibt auf der Grafikkarte.
    fn build_mesh(&mut self) {
        // Attribute oder Skalierung geändert: nur die Tabelle neu, die Netze bleiben
        let t = self.scene.table();
        let key = (t.rev, t.theme_rev, self.title.scale.to_bits());
        if self.looks_key != Some(key) {
            self.looks_key = Some(key);
            self.renderer.set_looks(&t.looks(self.title.scale));
        }
        let live = self
            .edit
            .dragging_run()
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
        if self.edit.is_dragging() || self.ui.level_dragging().is_some() {
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
        if self.picking.select_only(id) {
            self.quantity.dirty = true;
        }
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
        !self.tool.is_active() && !self.sect.is_busy()
    }

    /// Schnittlinie greifen nur im Grundriss und ohne angefangenen Wandzug.
    fn sect_enabled(&self) -> bool {
        self.ui.view == ViewKind::Plan && !self.tool.is_active()
    }

    /// Schnittebene der aktuellen Ansicht (nur im Schnitt).
    fn plane(&self) -> Option<(Vec3, Vec3)> {
        match self.ui.view {
            ViewKind::Section => self.sect.plane(),
            _ => None,
        }
    }

    fn refresh_cursor(&mut self) {
        self.edit.section = self.plane();
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
        self.cam = self.camera_for(v);
        if matches!(v, ViewKind::Plan | ViewKind::Section) {
            self.sect.ensure(&self.scene);
        }
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
            _ => fit_parallel(v, self.scene.bounds(), free_w, vh),
        }
    }

    /// Ersetzt das Modell (Neu, Öffnen). Verlauf, Auswahl und angefangene
    /// Eingaben gehen weg; die Kamera zeigt das ganze Modell.
    fn replace_scene(&mut self, model: sk_model::Model) {
        self.scene = Scene::with_model(model);
        self.ui.dialog = false;
        self.snaps_key = None;
        self.scene.set_theme(&self.theme);
        self.tool.set_enabled(self.tool.enabled);
        self.set_wall_kind(self.tool.category);
        self.nav = Navigation::default();
        self.edit = WallEdit::default();
        self.sect = SectionLine::default();
        self.sel = Selection::default();
        self.props_key = None;
        self.ui.set_props(None);
        self.props_dirty = true;
        self.live_runs.clear();
        // Tabelle sicher neu setzen, auch wenn die neue denselben Stand hat
        self.looks_key = None;
        self.cam3d = start_camera();
        self.cam3d_empty = true;
        self.cam = self.camera_for(self.ui.view);
        if matches!(self.ui.view, ViewKind::Plan | ViewKind::Section) {
            self.sect.ensure(&self.scene);
        }
        self.upload_model();
        self.overlay_dirty = true;
        self.refresh_cursor();
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
                self.tip = None;
                self.renderer.set_overlay(OVERLAY_TIP, 0, 0, 0, 0, &[]);
            }
            Command::ClearRecent => self.recent.clear(),
            Command::Settings => self.open_prefs(),
        }
    }

    /// Einstellungsfenster öffnen; ist es offen, bleibt es (es liegt ohnehin
    /// vorn).
    fn open_prefs(&mut self) {
        if self.prefs.is_some() {
            return;
        }
        if self.ui.dialog {
            self.close_building_dialog(false);
        }
        self.ui.hover = None;
        self.title.hover = None;
        let p = prefs::Prefs::open(&mut self.scene, &self.theme).with_memory(&self.prefs_mem);
        self.prefs = Some(p);
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
        self.ui.forget_theme();
        self.ui.use_theme(&self.theme);
        self.ui.fit(self.title.scale, self.w, self.h);
        self.looks_key = None;
        self.mark_keys = [None; 2];
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
        let Some(p) = self.prefs.as_mut() else {
            self.renderer.set_overlay(OVERLAY_PREFS, 0, 0, 0, 0, &[]);
            return;
        };
        let (c, x, y) = p.paint(&self.theme, &self.ui.fonts, &win, &self.scene);
        let px = c.to_premul_rgba8();
        self.renderer
            .set_overlay(OVERLAY_PREFS, x, y, c.width as u32, c.height as u32, &px);
        self.redraw = true;
    }

    fn paint_prefs_popup(&mut self) {
        self.prefs_popup_dirty = false;
        self.redraw = true;
        let win = self.prefs_win();
        let img = self
            .prefs
            .as_mut()
            .and_then(|p| p.paint_popup(&self.theme, &self.ui.fonts, &win, &self.scene));
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
                self.replace_scene(sk_model::Model::new());
                self.doc = Document::new(self.scene.model().revision());
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
            _ => {}
        }
    }

    /// Rückgängig bzw. Wiederherstellen (Knopf und Kürzel).
    fn history(&mut self, redo: bool) {
        if self.tool.is_active() || self.edit.is_dragging() {
            return;
        }
        let changed = if redo {
            self.scene.redo()
        } else {
            self.scene.undo()
        };
        if changed {
            self.upload_model();
            self.sync_levels();
            self.refresh_cursor();
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
        let caption = windows::quantity_caption(&self.doc, self.scene.shown_revision());
        self.quantity.title.caption = caption.clone();
        surface.open_quantity(&caption);
        if !self.ui.quantity_open {
            self.ui.quantity_open = true;
            self.dirty_buttons.push(Id::Quantity);
        }
    }

    /// Mengenfenster schließen (✕ oder Taskleiste).
    fn close_quantity(&mut self, surface: &Surface) {
        surface.close_quantity();
        self.quantity.open = false;
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
        let Some(out) = self
            .quantity
            .handle(&e, &self.theme, &self.ui.fonts, &mut self.picking)
        else {
            return;
        };
        match out {
            quantity::Out::Picking { selection } => {
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
            quantity::Out::Zoom(ids) => self.zoom_to(&ids),
            quantity::Out::SaveCsv => self.save_csv(surface),
            quantity::Out::Command(c) => surface.quantity_command(c),
            quantity::Out::Close => self.close_quantity(surface),
        }
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
        let suggested = format!("{stem} Mengenermittlung.csv");
        let filters = [
            ("Tabelle für Excel (*.csv)", "*.csv"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("Als Tabelle speichern", &filters, "csv", &suggested)
        else {
            return;
        };
        let sched = self.scene.schedule().clone();
        let bytes = schedule_view::csv(self.scene.model(), &sched);
        if let Err(e) = std::fs::write(&path, bytes) {
            surface.message(
                &format!("Die Tabelle konnte nicht gespeichert werden:\n{e}"),
                true,
            );
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
        self.quantity.sync(&mut self.scene, &self.picking, anim);
        let now = Instant::now();
        self.quantity_busy = self.quantity.tick(&self.theme, now);
        let caption = windows::quantity_caption(&self.doc, self.scene.shown_revision());
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
        if !self.quantity.open {
            return;
        }
        if std::mem::take(&mut self.hover_from_list) {
            self.redraw = true;
        }
        if self.picking.set_hover(hit, Vec::new()) {
            self.quantity.dirty = true;
        }
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
                }
                self.tool.set_enabled(on);
            }
            Id::Ref(r) => self.tool.ref_side = r,
            Id::Ortho => self.tool.ortho = !self.tool.ortho,
            Id::View(v) => self.set_view(v),
            Id::Quantity => self.quantity_wanted = true,
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
            if self.scene.set_field(id, field, mm) {
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
        let d = self.scene.model().defaults();
        let set = match cat {
            Category::InteriorWall => d.interior_wall,
            _ => d.exterior_wall,
        };
        self.tool
            .set_category(cat, self.scene.model().wall_layers(set));
        self.ui.wall_layers = layer_rows(self.scene.model(), set);
        self.ui.interior = cat == Category::InteriorWall;
        self.overlay_dirty = true;
    }

    fn commit_wall(&mut self, wall: Option<sk_model::WallChain>) {
        if let Some(wall) = wall {
            self.scene.add_wall_as(&wall, self.tool.category);
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
        if self.save_dlg.is_some() && self.handle_save_dialog(e, surface) {
            return !self.quit;
        }
        if self.prefs.is_some() && self.handle_prefs(e, surface) {
            return !self.quit;
        }
        if self.menu.is_open() && self.handle_menu(e, surface) {
            return !self.quit;
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
        self.edit.section = self.plane();
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
                self.redraw = true;
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                self.ui.top = self.title.height();
                self.ui.fit(s, self.w, self.h);
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
                self.wheel.set_hover(None, t);
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
                self.wheel.set_hover(part, t);
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
                let sect_ev = if outside { Event::MouseLeave } else { ev };
                let so = self
                    .sect
                    .handle(&sect_ev, &self.scene, &self.cam, vw, vh, sc, sen);
                self.redraw |= so.redraw;
                // Der Grundriss hängt nicht von der Schnittlinie ab, nur die Ansicht „Schnitt“
                if so.changed && self.ui.view == ViewKind::Section {
                    self.upload_model();
                }
                let edit_ev = if outside || self.sect.is_busy() {
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
                let tool_ev = if outside || self.edit.is_busy() || self.sect.is_busy() {
                    Event::MouseLeave
                } else {
                    ev
                };
                self.redraw |= self.tool.handle(&tool_ev, &self.cam, vw, vh, sc).redraw;
                // Bauteil unter der Maus: die Mengenliste zeigt seine Zeile (B7)
                if self.quantity.open {
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
                    if !out.consumed {
                        let ev = in_view(e);
                        camera_moved |=
                            self.nav.handle(&ev, &mut self.cam, &self.scene, vw, vh, sc);
                        let so = self
                            .sect
                            .handle(&ev, &self.scene, &self.cam, vw, vh, sc, sen);
                        self.redraw |= so.redraw;
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
                let so = self
                    .sect
                    .handle(&ev, &self.scene, &self.cam, vw, vh, sc, sen);
                self.redraw |= so.redraw;
                let en = self.edit_enabled();
                let eo = self
                    .edit
                    .handle(&ev, &mut self.scene, &self.cam, vw, vh, sc, en);
                self.redraw |= eo.redraw;
                if let Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } = ev
                {
                    if eo.clicked.is_some() {
                        // Band angeklickt, aber nicht verschoben
                        self.sel.release(x, y, sc);
                        self.select(eo.clicked);
                    } else if self.sel.release(x, y, sc) {
                        let (view, plane) = (self.ui.view, self.plane());
                        let hit = selection::pick_at(
                            &mut self.scene,
                            &self.cam,
                            view,
                            plane,
                            x,
                            y,
                            vw,
                            vh,
                        );
                        self.select(hit);
                    }
                }
            }
            // Getippte Zeichen braucht nur das Einstellungsfenster
            Event::Text(_) => {}
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
            // Bild↑/Bild↓ wechseln im Grundriss das Geschoss (E18)
            Event::Key {
                key: key @ (wheel::KEY_PAGE_UP | wheel::KEY_PAGE_DOWN),
                down: true,
                ..
            } if self.ui.view == ViewKind::Plan => {
                let t = self.now();
                let (view, input) = (self.ui.view, self.tool.is_active());
                self.wheel.key(&mut self.scene, view, key, input, t);
            }
            Event::Key {
                key, down, mods, ..
            } => {
                let free = !self.tool.is_active() && !self.edit.is_dragging();
                let command = self.shortcuts.key(key, down, mods, free);
                let en = self.edit_enabled();
                let eo = self
                    .edit
                    .handle(&e, &mut self.scene, &self.cam, vw, vh, sc, en);
                if eo.changed {
                    self.upload_model();
                }
                self.redraw |= eo.redraw;
                if eo.consumed {
                    // Esc hat das Ziehen abgebrochen
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
            left_width: self.title.left_width(),
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
            let px = c.to_premul_rgba8();
            self.renderer
                .set_overlay(slot, x, y, c.width as u32, c.height as u32, &px);
        }
        self.dirty_buttons.clear();
        self.paint_props();
        self.paint_dialog();
        self.paint_menu();
        self.paint_save_dialog();
        self.paint_prefs();
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

    /// Knöpfe der Titelleiste an Verlauf und Menü angleichen.
    fn sync_title_state(&mut self) {
        // Bei offenem Einstellungsfenster gesperrt (E5)
        let free = self.prefs.is_none();
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
        if self.ui.dialog
            || self.ui.level_dragging().is_some()
            || self.menu.is_open()
            || self.save_dlg.is_some()
        {
            return None;
        }
        // Rückgängig und Wiederherstellen mit dem Namen des Schritts (E17)
        match self.title.hover {
            Some(Button::Undo) => return menu::history_hint(&self.scene, false),
            Some(Button::Redo) => return menu::history_hint(&self.scene, true),
            Some(_) => return None,
            None => {}
        }
        self.edit.coupled_hint().map(String::from)
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
        match (tip, hud) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Uhr des Geschossbogens: Millisekunden seit dem Start.
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    /// Was der Geschossbogen zeigen soll: nur im Grundriss, sanft weg,
    /// solange ein Dialog, das Dateimenü oder die Einstellungen offen sind.
    fn wheel_show(&self) -> wheel_view::Show {
        let blocked = self.ui.dialog
            || self.menu.is_open()
            || self.prefs.is_some()
            || self.save_dlg.is_some();
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
            _ => false,
        }
    }

    /// Geschossbogen (E18) je Durchlauf: einen begonnenen Wechsel übernehmen
    /// (altes Bild festhalten, Grundriss des Ziels), Überblendung und Bilder
    /// nachführen.
    fn sync_wheel(&mut self) {
        let t = self.now();
        self.wheel.tick(&mut self.scene, t);
        let plan = self.ui.view == ViewKind::Plan;
        if self.wheel.take_started().is_some() {
            if plan && self.wheel.animating(t) && self.w > 0 {
                self.renderer.capture_scene();
                // Die Zeit läuft ab dem ersten Bild mit dem neuen Grundriss
                self.wheel.wait_for_first_frame();
            }
            if plan {
                self.upload_model();
            }
            self.sync_levels();
            self.overlay_dirty = true;
            self.refresh_cursor();
        }
        self.view_shift = 0.0;
        match self.wheel.progress(t).filter(|_| plan) {
            Some((e, sw)) => {
                // Nach oben: das alte Geschoss sinkt weg, das neue kommt von oben
                let dir = sw.steps.signum() as f32;
                let slide = wheel::SLIDE * self.ui.dpi();
                self.renderer.set_snapshot(1.0 - e, dir * slide * e);
                self.view_shift = -dir * slide * (1.0 - e);
                self.redraw = true;
            }
            None => self.renderer.release_snapshot(),
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
            let px = c.to_premul_rgba8();
            self.renderer
                .set_overlay(OVERLAY_DIALOG, x, y, c.width as u32, c.height as u32, &px);
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
            let px = c.to_premul_rgba8();
            self.renderer
                .set_overlay(OVERLAY_PROPS, x, y, c.width as u32, c.height as u32, &px);
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

fn app(surface: Surface, screenshot: Option<String>) -> Result<(), String> {
    let gl = Gl::load(|name| surface.gl_proc(name))?;
    // Farbschema aus %APPDATA%\Skizzeo\einstellungen.txt (fehlt sie: dunkel)
    let mut settings = settings::Settings::new(
        std::env::args(),
        std::env::var_os("APPDATA").map(std::path::PathBuf::from),
    );
    let theme = settings.load();
    for h in &settings.hints {
        eprintln!("Einstellungen: {h}");
    }
    let mut scene = Scene::new();
    scene.set_theme(&theme);
    let renderer = Renderer::new(gl, style(&theme.env))?;
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
        nav: Navigation::default(),
        tool,
        edit: WallEdit::default(),
        sect: SectionLine::default(),
        sel: Selection::default(),
        props_key: None,
        snaps_key: None,
        mouse_at: None,
        tip: None,
        menu: menu::FileMenu::default(),
        menu_dirty: false,
        shortcuts: menu::Shortcuts::default(),
        save_dlg: None,
        after_save: None,
        recent: settings.recent.clone(),
        recent_on: settings.path.is_some(),
        quit: false,
        prefs: None,
        prefs_mem: prefs::Memory::default(),
        prefs_dirty: false,
        prefs_popup_dirty: false,
        settings,
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
        mark_keys: [None; 2],
        wheel,
        wheel_view: wheel_view::WheelView::new(OVERLAY_WHEEL),
        clock: Instant::now(),
        wheel_acc: 0.0,
        wheel_press: false,
        view_shift: 0.0,
        picking: picking::Picking::default(),
        quantity: quantity::QuantityWindow::new(),
        hover_from_list: false,
        fly: None,
        quantity_busy: false,
        quantity_wanted: false,
        auto_switch: std::env::args()
            .skip_while(|a| a != "--geschosswechsel")
            .nth(1)
            .and_then(|n| n.parse().ok())
            .map(|n| (n, true)),
    };
    a.upload_model();
    a.sync_levels();
    // `skizzeo.exe haus.szo`: Projekt gleich öffnen
    if let Some(path) = document::path_from_args(std::env::args()) {
        a.open_path(&surface, path);
    }
    // `--ansicht schnitt`: mit dieser Ansicht beginnen (Bildvergleiche)
    if let Some(v) = std::env::args()
        .skip_while(|a| a != "--ansicht")
        .nth(1)
        .and_then(|n| ViewKind::from_arg(&n))
    {
        a.set_view(v);
    }
    a.sync_caption(&surface);
    // Mengenfenster (F2, B7): gemerkte Lage, Breite aus dem Schema
    *surface.layout() = windows::read_settings(&a.settings.windows, &surface.monitors());
    surface.layout().set_width_dip(a.theme.size.qto_window_w);
    if surface.layout().quantity_open() || std::env::args().any(|x| x == "--mengenfenster") {
        a.open_quantity(&surface);
    }
    // `--zeiten datei.csv`: Dauer jedes Bildes in Millisekunden mitschreiben
    let timing_path = std::env::args().skip_while(|a| a != "--zeiten").nth(1);
    let mut timing = timing_path.as_ref().map(|p| {
        let _ = std::fs::write(p, "ereignisse;netz;zeichnen;tauschen;gesamt\n");
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
        {
            // Leerlauf: Grundrisse der Nachbargeschosse vorbereiten, damit ein
            // Wechsel am Geschossbogen nichts neu rechnet (E18)
            if a.ui.view == ViewKind::Plan && a.scene.plans_pending() {
                a.scene.prepare_neighbor_plans();
            }
            let wait = match (a.tip_wait(), a.prefs.as_ref().and_then(|p| p.wait())) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            let wait = match (wait, a.quantity_busy) {
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
                    save_settings(&mut a.settings, &a.theme, &a.recent, &surface);
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
        for e in events {
            if !a.handle(e, &surface) {
                if let Some(log) = timing.as_mut() {
                    write_timing(&timing_path, log);
                }
                save_settings(&mut a.settings, &a.theme, &a.recent, &surface);
                return Ok(());
            }
        }

        if std::mem::take(&mut a.quantity_wanted) {
            a.open_quantity(&surface);
        }
        a.sync_wheel();
        if a.prefs.as_mut().is_some_and(|p| p.tick()) {
            a.prefs_dirty = true;
        }
        a.sync_quantity(&surface);
        a.sync_tip();
        a.sync_title_state();
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
        if a.w > 0 {
            a.paint_buttons(&surface);
        }
        let cursor = match &a.prefs {
            Some(p) => p.cursor(),
            None if a.wheel_hand() => sk_platform::Cursor::Hand,
            None => a.ui.cursor(),
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
            match a.ui.view {
                ViewKind::Plan => {
                    helpers.extend(a.sect.helpers(&a.scene, &a.cam, vh, scale, &a.theme))
                }
                ViewKind::Persp => {}
                v => helpers.extend(ground_line(v, a.scene.bounds(), scale, a.scene.table())),
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
            for &id in &a.picking.selected {
                helpers.extend(selection::helpers(
                    &a.scene, id, a.ui.view, plane, scale, &a.theme,
                ));
            }
            if let Some(z) = a.ui.level_drag_z() {
                helpers.extend(level_guide(a.ui.view, a.scene.bounds(), z, scale, &a.theme));
            }
            helpers.extend(a.edit.helpers(&a.scene, &a.cam, scale, !drawing, &a.theme));
            helpers.extend(a.tool.helpers(&a.cam, scale, &a.theme));
            a.renderer.set_helpers(&helpers);

            // Endsymbole der Schnittlinie als kleine Bilder an den Linienenden
            let marks = if a.ui.view == ViewKind::Plan {
                a.sect.marks(&a.scene, &a.cam, vw, vh)
            } else {
                Vec::new()
            };
            for i in 0..2 {
                match marks.get(i) {
                    Some(m) => {
                        // Bild nur neu zeichnen, wenn es sich ändert; sonst nur verschieben
                        let revs = (a.theme.rev, a.scene.table().rev);
                        let key = (m.left, a.sect.is_busy(), scale.to_bits(), revs);
                        let (ax, ay) = a.sect.mark_anchor(m.left, scale);
                        let x = (m.x - ax as f64).round() as i32;
                        let y = (m.y + th as f64 - ay as f64).round() as i32;
                        if a.mark_keys[i] == Some(key) {
                            a.renderer.move_overlay(OVERLAY_MARKS + i, x, y);
                        } else {
                            let (c, _, _) =
                                a.sect
                                    .paint_mark(&a.scene, &a.theme, &a.ui.fonts, m.left, scale);
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

            let mut view = a.cam.view(a.w, a.h - th);
            if drawing {
                view.paper = Some(a.scene.table().paper);
            }
            // Geschosswechsel: der neue Grundriss gleitet an seinen Platz
            if a.view_shift != 0.0 {
                view = view.shifted(a.view_shift, (a.h - th) as f32);
            }
            a.renderer.draw(a.w, a.h, th, &view)?;
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
                log.push_str(&format!(
                    "{:.3};{:.3};{:.3};{:.3};{:.3}\n",
                    ms(t_events, t_handled),
                    ms(t_handled, t_mesh),
                    ms(t_mesh, t_draw),
                    ms(t_draw, now),
                    ms(t_events, now),
                ));
                if log.len() > 4096 {
                    write_timing(&timing_path, log);
                }
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

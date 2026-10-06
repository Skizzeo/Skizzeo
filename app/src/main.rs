//! Skizzeo – 3D-Gebäudemodellierer.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(test)]
mod abnahme;
mod camera;
mod document;
mod draw_table;
mod nav;
#[cfg(test)]
mod perf;
mod scene;
mod section;
mod selection;
mod settings;
mod ui;
mod wall_edit;
mod wall_tool;

use camera::Camera;
use document::Document;
use draw_table::DrawTable;
use nav::Navigation;
use scene::Scene;
use section::SectionLine;
use selection::Selection;
use sk_math::{vec3, Vec3};
use sk_model::Category;
use sk_paint::Rgba;
use sk_platform::{
    CaptionArea, Config, Event, Key, MouseButton, SaveAnswer, Surface, WindowCommand,
};
use sk_render::{gl::Gl, Renderer, Style};
use sk_ui::{
    logo,
    theme::{Environment, Theme},
    titlebar::{Button, TitleBar},
};
use std::f64::consts::{FRAC_PI_2, PI};
use std::time::Instant;
use ui::{Grip, Id, LevelEvent, Panel, Ui, ViewKind};
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

/// Schreibt das Farbschema beim Beenden, falls es sich geändert hat.
fn save_settings(settings: &mut settings::Settings, theme: &Theme) {
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

// Oberflächenbilder in Zeichenreihenfolge: Endsymbole unter den Paneelen,
// die Titelleiste ganz oben.
/// Endsymbole der Schnittlinie im Grundriss (zwei Plätze).
const OVERLAY_MARKS: usize = 0;
const OVERLAY_TOOLS: usize = 2;
const OVERLAY_VIEWS: usize = 3;
/// Paneel „Eigenschaften“.
const OVERLAY_PROPS: usize = 4;
/// Paneel „Geschosse“.
const OVERLAY_LEVELS: usize = 5;
const OVERLAY_TITLE: usize = 6;

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
        occlude: false,
        round: false,
    }]
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
}

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
        if self.sel.validate(&self.scene) {
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
        if self.ui.set_levels(self.scene.levels()) {
            match self.ui.level_dragging() {
                Some(g) => self.dirty_buttons.push(Id::Grip(g)),
                None => self.overlay_dirty = true,
            }
        }
    }

    /// Wählt das Bauteil (oder nichts).
    fn select(&mut self, id: Option<sk_model::ElementId>) {
        if self.sel.set(id) {
            self.redraw = true;
        }
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

    /// Bei ungespeicherten Änderungen nachfragen. `true`: weitermachen.
    fn confirm_discard(&mut self, surface: &Surface) -> bool {
        if !self.doc.is_dirty(self.scene.model()) {
            return true;
        }
        let q = format!("Änderungen an „{}“ speichern?", self.doc.name());
        match surface.ask_save(&q) {
            SaveAnswer::Save => self.save(surface, false),
            SaveAnswer::Discard => true,
            SaveAnswer::Cancel => false,
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
                self.doc.mark_saved(path, self.scene.model().revision());
                true
            }
            Err(e) => {
                surface.message(&e, true);
                false
            }
        }
    }

    fn open(&mut self, surface: &Surface) {
        if !self.confirm_discard(surface) {
            return;
        }
        if let Some(path) = surface.open_dialog("Öffnen", &document::FILTERS) {
            self.open_path(surface, path);
        }
    }

    fn open_path(&mut self, surface: &Surface, path: std::path::PathBuf) {
        match document::load(&path) {
            Ok(loaded) => {
                self.replace_scene(loaded.model);
                self.doc = Document::opened(path, self.scene.model().revision());
                // Im Bildschirmfoto-Modus hielte die Meldung das Bild auf
                let shot = std::env::args().any(|a| a == "--screenshot");
                if !loaded.hints.is_empty() && !shot {
                    surface.message(&document::hints_message(&loaded.hints), false);
                }
            }
            Err(e) => surface.message(&e, true),
        }
    }

    fn new_project(&mut self, surface: &Surface) {
        if self.confirm_discard(surface) {
            self.replace_scene(sk_model::Model::new());
            self.doc = Document::new(self.scene.model().revision());
        }
    }

    /// Dateiname und `•` in Titelleiste und Taskleiste.
    fn sync_caption(&mut self, surface: &Surface) {
        let caption = self.doc.caption(self.scene.model());
        if caption != self.title.caption {
            surface.set_title(&format!("{caption} – Skizzeo"));
            self.title.caption = caption;
            if self.w > 0 {
                self.paint_title(surface);
                self.redraw = true;
            }
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
            // Zahlenfelder melden sich über `UiOut::submit`, Griffe über `UiOut::level`
            Id::Field(_) | Id::Grip(_) => {}
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
        if let Some((field, mm)) = out.submit.filter(|s| s.0.is_level()) {
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

    /// Zustand der Knöpfe an Werkzeug und Ansicht angleichen.
    fn sync_ui(&mut self) {
        let (b, r, o) = (self.tool.enabled, self.tool.ref_side, self.tool.ortho);
        if (self.ui.building, self.ui.ref_side, self.ui.ortho) != (b, r, o) {
            (self.ui.building, self.ui.ref_side, self.ui.ortho) = (b, r, o);
            self.overlay_dirty = true;
        }
    }

    fn handle(&mut self, e: Event, surface: &Surface) -> bool {
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
                return ask && !self.confirm_discard(surface);
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
                let over_ui = if busy {
                    false
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
            }
            Event::MouseDown { button, x, y, .. } => {
                if y < th {
                    if button == MouseButton::Left {
                        self.title.pressed = self.title.button_at(x, y, self.w);
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
                            surface.command(match b {
                                Button::Minimize => WindowCommand::Minimize,
                                Button::Maximize => WindowCommand::ToggleMaximize,
                                Button::Close => WindowCommand::Close,
                            });
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
            Event::Wheel { y, .. } => {
                if y >= th {
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
            Event::Key {
                key, down, mods, ..
            } => {
                let free = !self.tool.is_active() && !self.edit.is_dragging();
                let undo = down && mods.ctrl && key == Key::Char('Z') && free;
                let redo_key = down && mods.ctrl && key == Key::Char('Y') && free;
                let file_key = match key {
                    Key::Char(c @ ('S' | 'O' | 'N')) if down && mods.ctrl && free => Some(c),
                    _ => None,
                };
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
                } else if let Some(c) = file_key {
                    match c {
                        'S' => {
                            self.save(surface, mods.shift);
                        }
                        'O' => self.open(surface),
                        _ => self.new_project(surface),
                    }
                } else if undo || redo_key {
                    let changed = if undo {
                        self.scene.undo()
                    } else {
                        self.scene.redo()
                    };
                    if changed {
                        self.upload_model();
                        self.refresh_cursor();
                    }
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
        self.overlay_dirty = false;
        self.layout_dirty = false;
        self.redraw = true;
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
            };
            self.renderer
                .update_overlay(slot, p.x as i32, p.y as i32, p.w as u32, p.h as u32, &p.px);
            self.redraw = true;
        }
    }

    /// Neue Fensterbreite bei gleicher Paneelgröße: nur die Titelleiste neu,
    /// die Paneele behalten ihr Bild und rücken an ihren Platz.
    fn relayout_overlays(&mut self, surface: &Surface) {
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
    // `--zeiten datei.csv`: Dauer jedes Bildes in Millisekunden mitschreiben
    let timing_path = std::env::args().skip_while(|a| a != "--zeiten").nth(1);
    let mut timing = timing_path.as_ref().map(|p| {
        let _ = std::fs::write(p, "ereignisse;netz;zeichnen;tauschen;gesamt\n");
        String::new()
    });
    let mut last_tick: Option<std::time::Instant> = None;

    loop {
        let mut events = Vec::new();
        if !a.redraw && !a.overlay_dirty && !a.nav.is_animating() {
            match surface.wait_event() {
                Some(e) => events.push(e),
                None => {
                    if let Some(log) = timing.as_mut() {
                        write_timing(&timing_path, log);
                    }
                    save_settings(&mut settings, &a.theme);
                    return Ok(());
                }
            }
        }
        while let Some(e) = surface.poll_event() {
            events.push(e);
        }
        coalesce_moves(&mut events);
        let t_events = Instant::now();
        for e in events {
            if !a.handle(e, &surface) {
                if let Some(log) = timing.as_mut() {
                    write_timing(&timing_path, log);
                }
                save_settings(&mut settings, &a.theme);
                return Ok(());
            }
        }

        if a.overlay_dirty && a.w > 0 {
            a.paint_overlays(&surface);
        } else if a.layout_dirty && a.w > 0 {
            a.relayout_overlays(&surface);
        }
        if a.props_dirty && a.w > 0 {
            a.paint_props();
        }
        if a.w > 0 {
            a.paint_buttons(&surface);
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
                (Some(c), ViewKind::Plan) => Some(scene::mesh_of(&c.solid_cut_at(scene::PLAN_CUT))),
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
            if let Some(id) = a.sel.id {
                let plane = a.plane();
                helpers.extend(selection::helpers(
                    &a.scene, id, a.ui.view, plane, scale, &a.theme,
                ));
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
            a.renderer.draw(a.w, a.h, th, &view)?;
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

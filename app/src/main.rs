//! Skizzeo – 3D-Gebäudemodellierer.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod nav;
mod scene;
mod wall_edit;
mod wall_tool;

use camera::Camera;
use nav::Navigation;
use scene::Scene;
use sk_math::vec3;
use sk_paint::Rgba;
use sk_platform::{CaptionArea, Config, Event, Key, MouseButton, Surface, WindowCommand};
use sk_render::{gl::Gl, Renderer, Style};
use sk_ui::{
    logo, theme,
    titlebar::{Button, TitleBar},
};
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

fn rgb(c: Rgba) -> [f32; 3] {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

fn style(scale: f32) -> Style {
    let l = vec3(0.32, -0.48, 0.82).normalized().to_f32();
    Style {
        sky: theme::SKY.iter().map(|&(d, c)| (d, rgb(c))).collect(),
        ground: rgb(theme::GROUND),
        horizon_softness: theme::HORIZON_SOFTNESS,
        face: rgb(theme::FACE),
        edge: rgb(theme::EDGE),
        edge_width: 1.25 * scale,
        light: l,
        ambient: 0.84,
    }
}

fn app(surface: Surface, screenshot: Option<String>) -> Result<(), String> {
    let gl = Gl::load(|name| surface.gl_proc(name))?;
    let mut renderer = Renderer::new(gl, style(surface.scale()))?;
    let mut scene = Scene::new();
    renderer.set_mesh(0, &scene.mesh());

    let mut title = TitleBar::new(surface.scale());
    let target = scene.center().unwrap_or(vec3(0.0, 0.0, 0.0));
    let mut cam = Camera::looking_at(vec3(-6200.0, -8600.0, 3700.0), target, 45.0);
    let mut nav = Navigation::default();
    let mut tool = WallTool::new();
    let mut edit = WallEdit::default();
    let (mut w, mut h) = surface.size();
    let mut overlay_dirty = true;
    let mut redraw = true;
    let mut last_tick: Option<std::time::Instant> = None;

    loop {
        let mut events = Vec::new();
        if !redraw && !overlay_dirty && !nav.is_animating() {
            match surface.wait_event() {
                Some(e) => events.push(e),
                None => return Ok(()),
            }
        }
        while let Some(e) = surface.poll_event() {
            events.push(e);
        }

        for e in events {
            let th = title.height() as f64;
            let (vw, vh, sc) = (w as f64, h as f64 - th, title.scale as f64);
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
            let mut camera_moved = false;
            match e {
                Event::CloseRequested => return Ok(()),
                Event::Resized { width, height } => {
                    (w, h) = (width, height);
                    overlay_dirty = true;
                }
                Event::ScaleChanged(s) => {
                    title.scale = s;
                    renderer.set_style(style(s));
                    overlay_dirty = true;
                }
                Event::Maximized(m) => {
                    overlay_dirty |= title.maximized != m;
                    title.maximized = m;
                }
                Event::Focus(f) => {
                    overlay_dirty |= title.active != f;
                    title.active = f;
                }
                Event::Redraw => redraw = true,
                Event::MouseLeave => {
                    overlay_dirty |= title.hover.is_some();
                    title.hover = None;
                    redraw |= tool.handle(&Event::MouseLeave, &cam, vw, vh, sc).redraw;
                    let en = !tool.is_active();
                    redraw |= edit
                        .handle(&Event::MouseLeave, &mut scene, &cam, vw, vh, sc, en)
                        .redraw;
                }
                Event::MouseMove { y, .. } => {
                    let hover = if nav.is_dragging() {
                        None
                    } else if let Event::MouseMove { x, y, .. } = e {
                        title.button_at(x, y, w)
                    } else {
                        None
                    };
                    overlay_dirty |= hover != title.hover;
                    title.hover = hover;
                    let ev = in_view(e);
                    camera_moved |= nav.handle(&ev, &mut cam, &scene, vw, vh, sc);
                    let in_title = y < th && !nav.is_dragging() && !edit.is_dragging();
                    let edit_ev = if in_title { Event::MouseLeave } else { ev };
                    let en = !tool.is_active();
                    let out = edit.handle(&edit_ev, &mut scene, &cam, vw, vh, sc, en);
                    redraw |= out.redraw;
                    if out.changed {
                        renderer.set_mesh(0, &scene.mesh());
                    }
                    // Über dem Band zeigt das Wandwerkzeug keinen Fangpunkt
                    let tool_ev = if in_title || edit.is_busy() {
                        Event::MouseLeave
                    } else {
                        ev
                    };
                    redraw |= tool.handle(&tool_ev, &cam, vw, vh, sc).redraw;
                }
                Event::MouseDown { button, x, y, .. } => {
                    if y < th {
                        if button == MouseButton::Left {
                            title.pressed = title.button_at(x, y, w);
                            overlay_dirty = true;
                        }
                    } else {
                        let ev = in_view(e);
                        camera_moved |= nav.handle(&ev, &mut cam, &scene, vw, vh, sc);
                        let en = !tool.is_active();
                        let eo = edit.handle(&ev, &mut scene, &cam, vw, vh, sc, en);
                        redraw |= eo.redraw;
                        if !eo.consumed {
                            let out = tool.handle(&ev, &cam, vw, vh, sc);
                            redraw |= out.redraw;
                            if let Some(wall) = out.commit {
                                scene.add_wall(wall);
                                renderer.set_mesh(0, &scene.mesh());
                                redraw = true;
                            }
                            edit.refresh(&scene, &cam, vw, vh, sc, !tool.is_active());
                        }
                    }
                }
                Event::MouseUp { button, x, y, .. } => {
                    if button == MouseButton::Left {
                        if let Some(b) = title.pressed.take() {
                            overlay_dirty = true;
                            if title.button_at(x, y, w) == Some(b) {
                                surface.command(match b {
                                    Button::Minimize => WindowCommand::Minimize,
                                    Button::Maximize => WindowCommand::ToggleMaximize,
                                    Button::Close => WindowCommand::Close,
                                });
                            }
                        }
                    }
                    let ev = in_view(e);
                    camera_moved |= nav.handle(&ev, &mut cam, &scene, vw, vh, sc);
                    let en = !tool.is_active();
                    redraw |= edit.handle(&ev, &mut scene, &cam, vw, vh, sc, en).redraw;
                }
                Event::Wheel { y, .. } => {
                    if y >= th {
                        camera_moved |= nav.handle(&in_view(e), &mut cam, &scene, vw, vh, sc);
                    }
                }
                Event::Key {
                    key, down, mods, ..
                } => {
                    let free = !tool.is_active() && !edit.is_dragging();
                    let undo = down && mods.ctrl && key == Key::Char('Z') && free;
                    let redo_key = down && mods.ctrl && key == Key::Char('Y') && free;
                    let en = !tool.is_active();
                    let eo = edit.handle(&e, &mut scene, &cam, vw, vh, sc, en);
                    if eo.changed {
                        renderer.set_mesh(0, &scene.mesh());
                    }
                    redraw |= eo.redraw;
                    if eo.consumed {
                        // Esc hat das Ziehen abgebrochen
                    } else if undo || redo_key {
                        let changed = if undo { scene.undo() } else { scene.redo() };
                        if changed {
                            renderer.set_mesh(0, &scene.mesh());
                            edit.refresh(&scene, &cam, vw, vh, sc, true);
                            redraw = true;
                        }
                    } else {
                        let out = tool.handle(&e, &cam, vw, vh, sc);
                        redraw |= out.redraw;
                        if let Some(wall) = out.commit {
                            scene.add_wall(wall);
                            renderer.set_mesh(0, &scene.mesh());
                            redraw = true;
                        }
                    }
                }
            }
            if camera_moved {
                tool.refresh(&cam, vw, vh, sc);
                edit.refresh(&scene, &cam, vw, vh, sc, !tool.is_active());
                redraw = true;
            }
        }

        if overlay_dirty && w > 0 {
            let c = title.paint(w);
            renderer.set_overlay(w, title.height(), &c.to_premul_rgba8());
            surface.set_caption_area(CaptionArea {
                height: title.height(),
                buttons_width: title.buttons_width(),
            });
            overlay_dirty = false;
            redraw = true;
        }

        if nav.is_animating() {
            let now = std::time::Instant::now();
            let dt = last_tick.map_or(1.0 / 60.0, |t| (now - t).as_secs_f64().min(0.1));
            last_tick = Some(now);
            nav.tick(&mut cam, dt);
            let th = title.height() as f64;
            let (vw, vh, sc) = (w as f64, h as f64 - th, title.scale as f64);
            tool.refresh(&cam, vw, vh, sc);
            edit.refresh(&scene, &cam, vw, vh, sc, !tool.is_active());
            redraw = true;
        } else {
            last_tick = None;
        }

        if redraw && w > 0 && h > title.height() {
            let th = title.height();
            let preview = tool.preview().map(|c| c.solid()).unwrap_or_default();
            renderer.set_mesh(1, &scene::mesh_of(&preview));
            let mut helpers = edit.helpers(&scene, title.scale);
            helpers.extend(tool.helpers(&cam, title.scale));
            renderer.set_helpers(&helpers);
            renderer.draw(w, h, th, &cam.view(w, h - th))?;
            if let Some(path) = &screenshot {
                let px = renderer.read_pixels(w, h);
                std::fs::write(path, sk_paint::encode_png(w, h, &px))
                    .map_err(|e| format!("Bildschirmfoto: {e}"))?;
                return Ok(());
            }
            surface.swap_buffers();
            redraw = false;
        }
    }
}

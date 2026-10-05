//! Skizzeo – 3D-Gebäudemodellierer.

#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod camera;
mod nav;
mod scene;

use camera::Camera;
use nav::Navigation;
use scene::Scene;
use sk_math::vec3;
use sk_paint::Rgba;
use sk_platform::{CaptionArea, Config, Event, MouseButton, Surface, WindowCommand};
use sk_render::{gl::Gl, Renderer, Style};
use sk_ui::{logo, theme, titlebar::{Button, TitleBar}};

fn main() {
    let screenshot = std::env::args()
        .skip_while(|a| a != "--screenshot")
        .nth(1);
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
    let scene = Scene::test_body();
    renderer.set_mesh(&scene.mesh());

    let mut title = TitleBar::new(surface.scale());
    let mut cam = Camera::looking_at(vec3(-6200.0, -8600.0, 3700.0), scene.center().unwrap_or(vec3(0.0, 0.0, 0.0)), 45.0);
    let mut nav = Navigation::default();
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
            match e {
                Event::CloseRequested => return Ok(()),
                Event::Resized { width, height } => {
                    (w, h) = (width, height);
                    overlay_dirty = true;
                }
                Event::ScaleChanged(s) => {
                    title.scale = s;
                    renderer = {
                        let mut r = renderer;
                        r.set_style(style(s));
                        r
                    };
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
                }
                Event::MouseMove { x, y, mods } => {
                    let hover = if nav.is_dragging() { None } else { title.button_at(x, y, w) };
                    overlay_dirty |= hover != title.hover;
                    title.hover = hover;
                    let ev = Event::MouseMove { x, y: y - th, mods };
                    redraw |= nav.handle(&ev, &mut cam, &scene, w as f64, h as f64 - th, title.scale as f64);
                }
                Event::MouseDown { button, x, y, mods } => {
                    if y < th {
                        if button == MouseButton::Left {
                            title.pressed = title.button_at(x, y, w);
                            overlay_dirty = true;
                        }
                    } else {
                        let ev = Event::MouseDown { button, x, y: y - th, mods };
                        redraw |= nav.handle(&ev, &mut cam, &scene, w as f64, h as f64 - th, title.scale as f64);
                    }
                }
                Event::MouseUp { button, x, y, mods } => {
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
                    let ev = Event::MouseUp { button, x, y: y - th, mods };
                    redraw |= nav.handle(&ev, &mut cam, &scene, w as f64, h as f64 - th, title.scale as f64);
                }
                Event::Wheel { delta, x, y, mods } => {
                    if y >= th {
                        let ev = Event::Wheel { delta, x, y: y - th, mods };
                        redraw |= nav.handle(&ev, &mut cam, &scene, w as f64, h as f64 - th, title.scale as f64);
                    }
                }
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
            redraw = true;
        } else {
            last_tick = None;
        }

        if redraw && w > 0 && h > title.height() {
            let th = title.height();
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

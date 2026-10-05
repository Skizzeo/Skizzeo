//! Maussteuerung der 3D-Ansicht wie in SketchUp:
//! mittlere Maustaste = Drehen, Umschalt + mittlere Maustaste = Verschieben,
//! Mausrad = Zoomen zum Mauszeiger.

use crate::{camera::Camera, scene::Scene};
use sk_math::Vec3;
use sk_platform::{Event, MouseButton};

enum Drag {
    Orbit { pivot: Vec3 },
    Pan { depth: f64 },
}

#[derive(Default)]
pub struct Navigation {
    drag: Option<Drag>,
    last: (f64, f64),
}

impl Navigation {
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Verarbeitet ein Ereignis in Ansichtskoordinaten. `true`, wenn neu gezeichnet werden muss.
    pub fn handle(&mut self, e: &Event, cam: &mut Camera, scene: &Scene, w: f64, h: f64) -> bool {
        match *e {
            Event::MouseDown { button: MouseButton::Middle, x, y, mods } => {
                let p = pick(cam, scene, x, y, w, h);
                self.drag = Some(if mods.shift {
                    Drag::Pan { depth: (p - cam.eye).dot(cam.forward()) }
                } else {
                    Drag::Orbit { pivot: p }
                });
                self.last = (x, y);
                false
            }
            Event::MouseUp { button: MouseButton::Middle, .. } => {
                self.drag = None;
                false
            }
            Event::MouseMove { x, y, mods } => {
                let (dx, dy) = (x - self.last.0, y - self.last.1);
                self.last = (x, y);
                // Umschalt während des Ziehens wechselt zwischen Drehen und Verschieben.
                if let Some(Drag::Orbit { pivot }) = self.drag {
                    if mods.shift {
                        self.drag = Some(Drag::Pan { depth: (pivot - cam.eye).dot(cam.forward()) });
                    }
                }
                match self.drag {
                    Some(Drag::Orbit { pivot }) => {
                        cam.orbit(pivot, dx, dy);
                        true
                    }
                    Some(Drag::Pan { depth }) => {
                        cam.pan(depth, dx, dy, h);
                        true
                    }
                    None => false,
                }
            }
            Event::Wheel { delta, x, y, .. } => {
                let p = pick(cam, scene, x, y, w, h);
                cam.zoom(p, delta);
                true
            }
            _ => false,
        }
    }
}

/// Punkt unter dem Mauszeiger: Modell, sonst Boden, sonst im Fokusabstand.
fn pick(cam: &mut Camera, scene: &Scene, x: f64, y: f64, w: f64, h: f64) -> Vec3 {
    let dir = cam.ray(x, y, w, h);
    let mut t = scene.raycast(cam.eye, dir);
    if t.is_none() && cam.eye.z * dir.z < 0.0 {
        let tg = -cam.eye.z / dir.z;
        if tg < cam.focus * 50.0 {
            t = Some(tg);
        }
    }
    match t {
        Some(t) => {
            cam.focus = t;
            cam.eye + dir * t
        }
        None => cam.eye + dir * cam.focus,
    }
}

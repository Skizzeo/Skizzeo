//! Maussteuerung der 3D-Ansicht wie in SketchUp:
//! mittlere Maustaste = Drehen, Umschalt + mittlere Maustaste = Verschieben,
//! Mausrad = Zoomen zum Mauszeiger.
//!
//! Drehpunkt ist der Modellpunkt unter dem Mauszeiger. Liegt dort keine
//! Geometrie (Himmel, leerer Boden), wird um die Mitte des Modells gedreht,
//! damit es im Bild bleibt. Das Mausrad zoomt weich in kurzen Schritten.

use crate::{camera::Camera, scene::Scene};
use sk_math::Vec3;
use sk_platform::{Event, MouseButton};

/// Zeitkonstante des weichen Zoomens in Sekunden.
const ZOOM_SMOOTHING: f64 = 0.07;

enum Drag {
    Orbit { pivot: Vec3 },
    Pan { depth: f64 },
}

#[derive(Default)]
pub struct Navigation {
    drag: Option<Drag>,
    last: (f64, f64),
    /// Noch nicht ausgeführte Mausrad-Rasten und ihr Zielpunkt.
    zoom_pending: f64,
    zoom_point: Vec3,
}

impl Navigation {
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// `true`, solange eine Zoom-Bewegung ausläuft und weitere Bilder nötig sind.
    pub fn is_animating(&self) -> bool {
        self.zoom_pending.abs() > 1e-4
    }

    /// Lässt das Zoomen um `dt` Sekunden weiterlaufen.
    pub fn tick(&mut self, cam: &mut Camera, dt: f64) {
        if !self.is_animating() {
            self.zoom_pending = 0.0;
            return;
        }
        let f = 1.0 - (-dt.max(0.0) / ZOOM_SMOOTHING).exp();
        let mut step = self.zoom_pending * f;
        if (self.zoom_pending - step).abs() < 0.002 {
            step = self.zoom_pending;
        }
        cam.zoom(self.zoom_point, step);
        self.zoom_pending -= step;
    }

    /// Verarbeitet ein Ereignis in Ansichtskoordinaten. `true`, wenn neu gezeichnet werden muss.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        cam: &mut Camera,
        scene: &Scene,
        w: f64,
        h: f64,
        scale: f64,
    ) -> bool {
        match *e {
            Event::MouseDown {
                button: MouseButton::Middle,
                x,
                y,
                mods,
            } => {
                self.zoom_pending = 0.0;
                let pivot = drag_point(cam, scene, x, y, w, h);
                // In Parallelansichten (Grundriss, Schnitt, Ansichten) nur verschieben
                self.drag = Some(if mods.shift || cam.ortho.is_some() {
                    Drag::Pan {
                        depth: pan_depth(cam, pivot),
                    }
                } else {
                    Drag::Orbit { pivot }
                });
                self.last = (x, y);
                false
            }
            Event::MouseUp {
                button: MouseButton::Middle,
                ..
            } => {
                self.drag = None;
                false
            }
            Event::MouseMove { x, y, mods } => {
                let (dx, dy) = (x - self.last.0, y - self.last.1);
                self.last = (x, y);
                // Umschalt während des Ziehens wechselt zwischen Drehen und Verschieben.
                if let Some(Drag::Orbit { pivot }) = self.drag {
                    if mods.shift {
                        self.drag = Some(Drag::Pan {
                            depth: pan_depth(cam, pivot),
                        });
                    }
                }
                match self.drag {
                    Some(Drag::Orbit { pivot }) => {
                        // Gleiches Drehgefühl auf Bildschirmen mit hoher Auflösung
                        cam.orbit(pivot, dx / scale, dy / scale);
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
                if self.drag.is_some() {
                    return false;
                }
                // Richtungswechsel verwirft den Rest der alten Bewegung
                if self.zoom_pending * delta < 0.0 {
                    self.zoom_pending = 0.0;
                }
                self.zoom_point = zoom_point(cam, scene, x, y, w, h);
                self.zoom_pending += delta;
                true
            }
            _ => false,
        }
    }
}

/// Drehpunkt bzw. Griffpunkt: Geometrie unter der Maus, sonst Modellmitte.
fn drag_point(cam: &mut Camera, scene: &Scene, x: f64, y: f64, w: f64, h: f64) -> Vec3 {
    let (o, dir) = cam.ray(x, y, w, h);
    if let Some((t, _)) = scene.raycast(o, dir) {
        if cam.ortho.is_none() {
            cam.focus = t;
        }
        return o + dir * t;
    }
    match scene.center() {
        Some(c) => c,
        None => cam.eye + cam.forward() * cam.focus,
    }
}

/// Tiefe für das Verschieben: der Griffpunkt, mindestens vor der Kamera.
fn pan_depth(cam: &Camera, p: Vec3) -> f64 {
    let d = (p - cam.eye).dot(cam.forward());
    if d > cam.near() * 4.0 {
        d
    } else {
        cam.focus
    }
}

/// Zielpunkt des Zoomens: Geometrie, sonst naher Boden, sonst im Fokusabstand
/// auf dem Strahl unter der Maus.
fn zoom_point(cam: &mut Camera, scene: &Scene, x: f64, y: f64, w: f64, h: f64) -> Vec3 {
    let (o, dir) = cam.ray(x, y, w, h);
    if cam.ortho.is_some() {
        // Parallelprojektion: Tiefe spielt keine Rolle
        return o;
    }
    if let Some((t, _)) = scene.raycast(o, dir) {
        cam.focus = t;
        return o + dir * t;
    }
    let mut t = cam.focus;
    if o.z * dir.z < 0.0 {
        t = t.min(-o.z / dir.z);
    }
    o + dir * t
}

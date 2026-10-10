//! Abläufe ziehen (Gefälledämmung G4): Im Grundriss zeigt das gewählte
//! Flachdach mit Gefälle seine Abläufe als Punkte an der Aufkantung. Mit
//! der linken Maustaste lässt sich ein Ablauf an der Innenfläche der
//! Aufkantung entlang ziehen; der Gefälleplan geht live mit. Loslassen ist
//! ein Schritt im Verlauf, Esc bricht ab.

use crate::camera::Camera;
use crate::scene::Scene;
use sk_math::Vec3;
use sk_model::ElementId;
use sk_platform::{Event, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Greifabstand zum Ablauf in Pixeln (bei 96 dpi).
const PICK_PX: f64 = 10.0;
/// Durchmesser des Punkts in Pixeln (bei 96 dpi).
const DOT_PX: f32 = 9.0;

#[derive(Default)]
pub struct DrainEdit {
    /// Ablauf unter der Maus (Index).
    hover: Option<usize>,
    /// Gezogener Ablauf: Dachaufbau, Index, schon bewegt.
    drag: Option<(ElementId, usize, bool)>,
}

/// Ergebnis eines Ereignisses für die App.
#[derive(Default)]
pub struct DrainOutcome {
    pub redraw: bool,
    /// Das Dach hat sich geändert, das Netz muss neu hochgeladen werden.
    pub changed: bool,
    /// Ereignis gehört dem Ablauf, Band und Werkzeug bekommen es nicht.
    pub consumed: bool,
}

impl DrainEdit {
    /// Steht die Maus auf einem Ablauf oder wird einer gezogen?
    pub fn is_busy(&self) -> bool {
        self.hover.is_some() || self.drag.is_some()
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Ereignis im Grundriss; `sel` ist das gewählte Bauteil, `enabled`
    /// sperrt das Greifen (Werkzeug aktiv, andere Ansicht).
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        sel: Option<ElementId>,
        enabled: bool,
    ) -> DrainOutcome {
        let mut out = DrainOutcome::default();
        let (roof, drains) = match sel.filter(|_| enabled) {
            Some(id) => scene.roof_drains(id),
            None => (None, Vec::new()),
        };
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some((r, k, _)) = self.drag {
                    let p = cam.ray(x, y, w, h).0;
                    if scene.drag_drain(r, k, p) {
                        self.drag = Some((r, k, true));
                        out.changed = true;
                        out.redraw = true;
                    }
                    out.consumed = true;
                } else {
                    let hit = hit(cam, &drains, x, y, w, h, scale);
                    out.redraw = hit != self.hover;
                    self.hover = hit;
                }
            }
            Event::MouseLeave => {
                if self.drag.is_none() {
                    out.redraw = self.hover.take().is_some();
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.hover = hit(cam, &drains, x, y, w, h, scale);
                if let (Some(k), Some(r)) = (self.hover, roof) {
                    out.consumed = true;
                    if sk_model::edit_blocked(scene.model(), &[r]).is_none() {
                        scene.begin("Ablauf verschoben");
                        self.drag = Some((r, k, false));
                        out.redraw = true;
                    }
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if let Some((_, _, moved)) = self.drag.take() {
                    if moved {
                        scene.commit();
                    } else {
                        scene.rollback();
                    }
                    out.consumed = true;
                    out.changed = moved;
                    out.redraw = true;
                }
            }
            _ => {}
        }
        out
    }

    /// Esc beim Ziehen: alles wie vorher. `true`, wenn gezogen wurde.
    pub fn escape(&mut self, scene: &mut Scene) -> bool {
        match self.drag.take() {
            Some(_) => {
                scene.rollback();
                true
            }
            None => false,
        }
    }

    /// Punkte an den Abläufen des gewählten Flachdachs (nur Grundriss);
    /// unter der Maus und beim Ziehen in der Farbe des Ziehens.
    pub fn helpers(
        &self,
        scene: &Scene,
        sel: Option<ElementId>,
        scale: f32,
        theme: &Theme,
    ) -> Vec<Helper> {
        let Some(id) = sel else {
            return Vec::new();
        };
        let (_, drains) = scene.roof_drains(id);
        let z = scene.plan_cut() as f32 + 10.0;
        let hot = self.drag.map(|d| d.1).or(self.hover);
        drains
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let a = [p.x as f32, p.y as f32, z];
                let color = if hot == Some(k) {
                    theme.interact.drag
                } else {
                    theme.interact.select
                };
                Helper {
                    a,
                    b: a,
                    color,
                    width: DOT_PX * scale * if hot == Some(k) { 1.4 } else { 1.0 },
                    dash: 0.0,
                    pattern: sk_render::SOLID,
                    occlude: false,
                    round: true,
                }
            })
            .collect()
    }
}

/// Ablauf unter dem Mauspunkt (x, y), im Greifabstand.
fn hit(cam: &Camera, drains: &[Vec3], x: f64, y: f64, w: f64, h: f64, scale: f64) -> Option<usize> {
    let r = PICK_PX * scale;
    drains
        .iter()
        .enumerate()
        .filter_map(|(k, p)| {
            let (px, py) = cam.project(*p, w, h)?;
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            (d <= r).then_some((k, d))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

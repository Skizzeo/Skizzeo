//! Abläufe ziehen (Gefälledämmung G4): Im Grundriss zeigt das gewählte
//! Flachdach mit Gefälle seine Abläufe als Punkte an der Aufkantung. Mit
//! der linken Maustaste lässt sich ein Ablauf an der Innenfläche der
//! Aufkantung entlang ziehen; der Gefälleplan geht live mit. Loslassen ist
//! ein Schritt im Verlauf, Esc bricht ab.
//!
//! Jörn 11.10.: Ein Ablauf geht beim Greifen vor der Schnittlinie. Ein
//! Klick wählt ihn; Kontextmenü und Abschnitt „Gefälle“ fügen einen
//! weiteren hinzu oder löschen den gewählten.

use crate::camera::Camera;
use crate::delete::{Action, ContextMenu};
use crate::scene::Scene;
use crate::ui::Id;
use crate::App;
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
    /// Die Wahl des Ablaufs hat sich geändert (Paneel neu).
    pub active: bool,
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
            Some(id) => scene.plan_drains(id),
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
                let pick = roof.zip(self.hover);
                out.active = scene.set_active_drain(pick);
                out.redraw = out.active;
                if let Some((r, k)) = pick {
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

    /// Punkte an den Abläufen des gewählten Flachdachs (nur im Grundriss
    /// seiner Ebene); unter der Maus und beim Ziehen in der Farbe des
    /// Ziehens.
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
        let (_, drains) = scene.plan_drains(id);
        let z = scene.plan_cut() as f32 + 10.0;
        let hot = self.drag.map(|d| d.1).or(self.hover);
        let active = scene.active_drain(id);
        drains
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let a = [p.x as f32, p.y as f32, z];
                let on = hot == Some(k) || active == Some(k);
                let color = if on {
                    theme.interact.drag
                } else {
                    theme.interact.select
                };
                Helper {
                    a,
                    b: a,
                    color,
                    width: DOT_PX * scale * if on { 1.4 } else { 1.0 },
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

impl App {
    /// Abläufe des gewählten Flachdachs ziehen und wählen (G4, nur im
    /// Grundriss ohne Werkzeug). `true`, wenn das Ereignis dem Ablauf gehört.
    pub(crate) fn drain_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        let en = self.ui.view == crate::ui::ViewKind::Plan && !self.tool.enabled;
        let out = self
            .drains
            .handle(ev, &mut self.scene, &self.cam, vw, vh, sc, self.sel.id, en);
        self.redraw |= out.redraw;
        if out.changed && self.drains.is_dragging() {
            self.upload_live();
        } else if out.changed {
            self.upload_model();
        }
        if out.active {
            self.props_key = None;
        }
        out.consumed
    }

    /// Knöpfe im Abschnitt „Gefälle“: vorschlagen, hinzufügen, löschen.
    pub(crate) fn drain_button(&mut self, id: Id) {
        let Some(sel) = self.sel.id else {
            return;
        };
        if let Some(b) = sk_model::edit_blocked(self.scene.model(), &[sel]) {
            self.show_locked(b, Some((sel, crate::delete::Act::Field)));
            return;
        }
        let ok = match id {
            Id::PropsDrains => {
                self.scene.set_active_drain(None);
                self.scene.propose_roof_drains(sel)
            }
            Id::PropsDrainAdd => self.scene.add_roof_drain(sel),
            Id::PropsDrainDelete => self.scene.remove_roof_drain(sel),
            // Regenfallrohr: Notentwässerung, Kontrollablauf
            Id::PropsAction(a) => self.scene.props_action(sel, a),
            _ => false,
        };
        if ok {
            self.upload_model();
        }
        self.props_key = None;
    }

    /// Befehl aus dem Kontextmenü am Ablauf.
    pub(crate) fn drain_action(&mut self, a: Action) {
        match a {
            Action::AddDrain => self.drain_button(Id::PropsDrainAdd),
            Action::DeleteDrain => self.drain_button(Id::PropsDrainDelete),
            _ => {}
        }
    }

    /// Rechtsklick auf einen Ablauf des gewählten Flachdachs im Grundriss:
    /// ihn wählen und sein Kontextmenü öffnen. `false`, wenn dort keiner
    /// liegt.
    pub(crate) fn open_drain_context(&mut self, x: f64, y: f64) -> bool {
        let top = self.top();
        let th = top as f64;
        let Some(id) = self.sel.id else {
            return false;
        };
        if y < th
            || self.ui.view != crate::ui::ViewKind::Plan
            || self.tool.enabled
            || self.ui.dialog
            || self.ui.over(x, y, self.w, top)
        {
            return false;
        }
        let (vw, vh, sc) = self.view_size();
        let (roof, drains) = self.scene.plan_drains(id);
        let (Some(roof), Some(k)) = (roof, hit(&self.cam, &drains, x, y - th, vw, vh, sc)) else {
            return false;
        };
        self.scene.set_active_drain(Some((roof, k)));
        self.props_key = None;
        self.close_type_menu(false);
        let win = (self.w, self.h, top);
        let can_delete = drains.len() > 1;
        let s = self.ui.scale;
        self.context = Some(ContextMenu::for_drain(
            roof,
            can_delete,
            x,
            y,
            win,
            &self.theme,
            s,
        ));
        self.context_dirty = true;
        self.tip = None;
        self.redraw = true;
        true
    }
}

//! Werkzeug „Wandöffnung“ und „Türöffnung“ in der App (Paket O3): Knopf,
//! Maus, Esc. Die Rechnung (Fangen, Öffnung, Vorschau) steht in
//! [`werkzeug`], das Werkzeug selbst im
//! [`WallTool`](crate::wall_tool::WallTool), der auch die Vorschau zeichnet.
//! Im Grundriss setzt „Tür“ eine Innentür ([`innentuer`]).

#[path = "innentuer.rs"]
pub(crate) mod innentuer;
#[path = "oeffnung_werkzeug.rs"]
pub(crate) mod werkzeug;

use crate::meldung::Meldung;
use crate::ui::ViewKind;
use crate::App;
use sk_math::vec3;
use sk_model::oeffnung::OpeningKind;
use sk_platform::{Event, Key, MouseButton};
use werkzeug::{fangen, Bild, Fang, Klick, Werkzeug};

/// Fangabstand an Höhenlinien und Wandenden in Bildschirmpixeln (96 dpi).
const FANG_PX: f64 = 10.0;

/// Ansichten, in denen das Werkzeug der Art `kind` arbeitet: Außenwand-
/// flächen in 3D und den Ansichten, die Innentür im Grundriss.
fn ansicht_ok(v: ViewKind, kind: OpeningKind) -> bool {
    match v {
        ViewKind::Persp | ViewKind::Front | ViewKind::Back | ViewKind::Left | ViewKind::Right => {
            true
        }
        ViewKind::Plan => kind == OpeningKind::Door,
        ViewKind::Section => false,
    }
}

impl App {
    /// Knopf „Fenster“ (Wandöffnung) bzw. „Tür“ (Türöffnung): schaltet
    /// das Werkzeug der Art ein, derselbe Knopf aus. Im Schnitt geht es nach
    /// 3D, im Grundriss das Fenster auch; die Tür bleibt dort als Innentür.
    pub(crate) fn oeffnung_klick(&mut self, kind: OpeningKind) {
        let an = self.tool.oeffnung.as_ref().is_none_or(|w| w.kind != kind);
        if an && self.ui.foundation_active {
            return;
        }
        self.tool.set_enabled(false);
        self.tool.oeffnung = an.then(|| Werkzeug::new(kind));
        if an {
            self.nord.set_aktiv(false);
            if !ansicht_ok(self.ui.view, kind) {
                self.set_view(ViewKind::Persp);
            }
        }
        self.ext_sync();
        self.refresh_cursor();
        self.redraw = true;
    }

    /// Ecke unter der Maus (Ansichtspixel `x`, `y`): Außenwandfläche
    /// getroffen und gefangen.
    fn oeffnung_fang(&self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) -> Option<Fang> {
        let (o, d) = self.cam.ray(x, y, vw, vh);
        let (face, p) = self.scene.outer_face_at(o, d)?;
        let m = self.scene.model();
        let elevation = m.storey(face.storey)?.elevation;
        // Fangabstand in mm: 100 mm senkrecht an dieser Stelle in Pixeln
        let px = |q| self.cam.project(q, vw, vh);
        let tol = match (px(p), px(p + vec3(0.0, 0.0, 100.0))) {
            (Some(a), Some(b)) => {
                let n = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                FANG_PX * sc * 100.0 / n.max(1e-6)
            }
            _ => 0.0,
        };
        Some(fangen(&face, &m.face_lines(face.storey), elevation, p, tol))
    }

    /// Mausbewegung (Ansichtskoordinaten); `MouseLeave` über Paneelen und
    /// Bändern nimmt die Ecke weg.
    pub(crate) fn oeffnung_move(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) {
        let Some(kind) = self.tool.oeffnung.as_ref().map(|w| w.kind) else {
            return;
        };
        if self.innentuer_an() {
            match *ev {
                Event::MouseMove { x, y, .. } => self.tuer_suchen(x, y, vw, vh, sc),
                Event::MouseLeave => self.tuer_bild(|t, _, _| t.weg()),
                _ => {}
            }
            return;
        }
        let hover = match *ev {
            Event::MouseMove { x, y, .. } if ansicht_ok(self.ui.view, kind) => {
                self.oeffnung_fang(x, y, vw, vh, sc)
            }
            Event::MouseMove { .. } | Event::MouseLeave => None,
            _ => return,
        };
        self.oeffnung_vorschau(hover);
    }

    /// Ecke unter der Maus setzen und die Vorschau neu rechnen.
    fn oeffnung_vorschau(&mut self, hover: Option<Fang>) {
        let m = self.scene.model();
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return;
        };
        let alt = w.vorschau.clone();
        w.hover = hover;
        w.vorschau = w.bild(|f| m.face_lines(f.storey));
        self.redraw |= w.vorschau != alt;
    }

    /// Linksklick über einer Außenwand: erste Ecke, dann die Öffnung als
    /// ein Schritt. `true`, wenn das Werkzeug den Klick nahm.
    pub(crate) fn oeffnung_down(&mut self, ev: &Event) -> bool {
        let Event::MouseDown {
            button: MouseButton::Left,
            ..
        } = *ev
        else {
            return false;
        };
        if self.innentuer_an() {
            return self.tuer_setzen();
        }
        let m = self.scene.model();
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return false;
        };
        if w.hover.is_none() {
            return false;
        }
        // Lehnt das Modell ab, bleibt die erste Ecke und die Statuszeile
        // sagt warum (Review #86, H4)
        match w.setzen(m) {
            Klick::Oeffnung(o) => {
                if self.scene.add_opening(o).is_some() {
                    self.upload_model();
                }
            }
            Klick::Abgelehnt(t) => self.status(Meldung::mit("{}.", &[&t]), crate::NOTICE_TIME),
            Klick::Nichts | Klick::Erste => {}
        }
        let hover = self.tool.oeffnung.as_ref().and_then(|w| w.hover.clone());
        self.oeffnung_vorschau(hover);
        self.redraw = true;
        true
    }

    /// Nach Rückgängig oder Wiederherstellen (Review #86, H5): Die erste Ecke
    /// und die Vorschau zeigten auf Flächen, die es so nicht mehr gibt.
    pub(crate) fn oeffnung_neu(&mut self) {
        if let Some(w) = self.tool.oeffnung.as_mut() {
            w.neu();
            self.redraw = true;
        }
    }

    /// Fokusverlust: wie nach einem Schritt, die Maus ist fort.
    pub(crate) fn oeffnung_fokus(&mut self, aktiv: bool) {
        if !aktiv {
            self.oeffnung_neu();
        }
    }

    /// Taste gedrückt: **Esc** wie [`App::oeffnung_esc`], **Tab** schaltet
    /// den Bezugspunkt der Innentür. `true`, wenn das Werkzeug sie nahm.
    pub(crate) fn oeffnung_taste(&mut self, key: Key) -> bool {
        match key {
            Key::Escape => self.oeffnung_esc(),
            Key::Tab if self.innentuer_an() => {
                self.tuer_bild(|t, m, st| {
                    t.tab();
                    t.nochmal(m, st);
                });
                true
            }
            _ => false,
        }
    }

    /// Mausrad über einer Innenwand mit der Innentür am Cursor: blättert die
    /// Breiten statt zu zoomen. `true`, wenn das Werkzeug es nahm.
    pub(crate) fn tuer_rad(&mut self, ev: &Event) -> bool {
        let Event::Wheel { delta, .. } = *ev else {
            return false;
        };
        let am_cursor = self
            .tool
            .oeffnung
            .as_ref()
            .is_some_and(|w| w.innen.hover.is_some());
        if !self.innentuer_an() || !am_cursor {
            return false;
        }
        self.tuer_bild(|t, m, st| {
            t.rad(delta);
            t.nochmal(m, st);
        });
        true
    }

    /// Innentür im Grundriss: Werkzeug „Tür“ in der Grundrissansicht.
    fn innentuer_an(&self) -> bool {
        self.ui.view == ViewKind::Plan
            && self
                .tool
                .oeffnung
                .as_ref()
                .is_some_and(|w| w.kind == OpeningKind::Door)
    }

    /// Ändert die Innentür mit `f` und zeichnet ihre Vorschau neu.
    fn tuer_bild(
        &mut self,
        f: impl FnOnce(&mut innentuer::Innentuer, &sk_model::Model, sk_model::StoreyId),
    ) {
        let st = self.scene.active_storey();
        let m = self.scene.model();
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return;
        };
        f(&mut w.innen, m, st);
        let bild = w.innen.hover.as_ref().map_or_else(Bild::default, |v| Bild {
            linien: Vec::new(),
            umriss: innentuer::umriss(v),
            punkt: Some((v.punkt, false)),
        });
        self.redraw |= w.vorschau != bild;
        w.vorschau = bild;
    }

    /// Mausbewegung im Grundriss (Ansichtspixel): Punkt in Schnitthöhe,
    /// Fangabstand in mm aus [`FANG_PX`].
    fn tuer_suchen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) {
        let z = self.scene.plan_cut();
        let at = |dx: f64| self.cam.plane_point(x + dx, y, vw, vh, z);
        let hit = match (at(0.0), at(FANG_PX * sc)) {
            (Some(p), Some(q)) => Some((p, (q - p).length())),
            _ => None,
        };
        self.tuer_bild(|t, m, st| match hit {
            Some((p, tol)) => t.suchen(m, st, p, tol, z),
            None => t.weg(),
        });
    }

    /// Klick mit der Innentür am Cursor: setzt sie als ein Schritt; lehnt
    /// das Modell ab, sagt die Statuszeile warum.
    fn tuer_setzen(&mut self) -> bool {
        let o = self.tool.oeffnung.as_ref().and_then(|w| w.innen.oeffnung());
        let Some(o) = o else {
            return false;
        };
        let m = self.scene.model();
        match m.opening_fault(&o, None).map(|f| f.text(m)) {
            Some(t) => self.status(Meldung::mit("{}.", &[&t]), crate::NOTICE_TIME),
            None => {
                if self.scene.add_opening(o).is_some() {
                    self.upload_model();
                }
            }
        }
        self.tuer_bild(|t, m, st| t.nochmal(m, st));
        self.redraw = true;
        true
    }

    /// Esc: nimmt die erste Ecke zurück, ohne sie schließt das Werkzeug.
    /// `true`, wenn es lief.
    pub(crate) fn oeffnung_esc(&mut self) -> bool {
        let Some(w) = self.tool.oeffnung.as_mut() else {
            return false;
        };
        if w.esc() {
            let hover = w.hover.clone();
            self.oeffnung_vorschau(hover);
        } else {
            self.tool.oeffnung = None;
            self.ext_sync();
            self.refresh_cursor();
        }
        self.redraw = true;
        true
    }
}

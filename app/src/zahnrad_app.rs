//! Zahnräder oben rechts in der Ansicht, links neben dem Paneel „Ansichten“:
//! „Schatten“ in den vier Ansichten (S7, [`ansicht_schatten`]) und
//! „Schnitthöhe“ im Grundriss ([`grundriss_schnitt`]). Hier die Verdrahtung
//! in der App: Wahl, Lage, Ereignisse, Tasten, Tooltip und Bilder.

use crate::ui::{Panel, ViewKind};
use crate::{
    ansicht_schatten, grundriss_schnitt, App, OVERLAY_ANSICHT, OVERLAY_SCHNITT, SCHATTEN_FEHLT,
};
use sk_platform::{Event, Key, Modifiers};

impl App {
    /// Wahl des Schattens der aktuellen Ansicht (S7) mit ihrem Platz;
    /// `None` außerhalb der vier Ansichten.
    pub(crate) fn ansicht_wahl(&self) -> Option<(usize, sk_model::ViewShade)> {
        let i = ansicht_schatten::platz(self.ui.view)?;
        Some((
            i,
            self.scene
                .model()
                .view_shade(i, self.settings.vorgaben.schatten),
        ))
    }

    /// Stand der Sonne für die Ansichten (S7).
    pub(crate) fn ansicht_sonne(&self) -> sk_model::Sun {
        ansicht_schatten::sonne_der_ansichten(self.scene.model().sun())
    }

    /// Lage von Zahnrad und Feld in der Ansicht.
    pub(crate) fn ansicht_lage(&self, sc: f64) -> ansicht_schatten::Lage {
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
    pub(crate) fn ansicht_handle(&mut self, ev: &Event, sc: f64) -> bool {
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
            let unter = self.scene.model().view_below(i);
            for k in 0..sk_model::SHADE_VIEWS.len() {
                self.scene
                    .set_view_shade(k, vs, self.settings.vorgaben.schatten);
                self.scene.set_view_below(k, unter);
            }
            self.redraw = true;
        }
        // Unter Gelände (S11): ausgeblendet passt der Ausschnitt bis zum
        // Gelände, solange nicht verschoben oder gezoomt wurde
        if let Some(g) = out.unter {
            self.scene.set_view_below(i, g);
            if self.fitted.as_ref() == Some(&self.cam) {
                self.fit_camera();
            }
            self.redraw = true;
        }
        out.consumed
    }

    /// Lage von Zahnrad und Feld „Schnitthöhe“ im Grundriss.
    fn schnitt_lage(&self, sc: f64) -> grundriss_schnitt::Lage {
        let rechts = self.ui.rect(Panel::Views, self.w, self.top()).x;
        grundriss_schnitt::lage(rechts, sc as f32, &self.theme)
    }

    /// Ereignis an Zahnrad und Feld „Schnitthöhe“ (nur im Grundriss);
    /// `true`, wenn sie es genommen haben.
    fn schnitt_handle(&mut self, ev: &Event, sc: f64) -> bool {
        if self.ui.view != ViewKind::Plan {
            if self.plan_schnitt.offen || self.plan_schnitt.hover.is_some() {
                self.plan_schnitt.reset();
                self.redraw = true;
            }
            return false;
        }
        let lage = self.schnitt_lage(sc);
        let m = self.scene.model();
        let (mm, eigen) = (m.plan_cut_height(), m.plan_cut_own().is_some());
        let out = self.plan_schnitt.handle(ev, &lage, mm, eigen);
        self.schnitt_anwenden(out);
        out.consumed
    }

    /// Neue Schnitthöhe übernehmen: Grundrisse neu schneiden.
    fn schnitt_anwenden(&mut self, out: grundriss_schnitt::Ausgang) {
        self.redraw |= out.redraw;
        if let Some(h) = out.neu {
            if h != self.scene.model().plan_cut_height() {
                self.scene.set_plan_cut_height(h);
                self.upload_model();
            }
        }
    }

    /// Ereignis an den Zahnrädern der Ansicht; `true`, wenn eins es nahm.
    pub(crate) fn zahnrad_handle(&mut self, ev: &Event, sc: f64) -> bool {
        self.ansicht_handle(ev, sc) | self.schnitt_handle(ev, sc)
    }

    /// Ob ein offenes Feld der Zahnräder die Taste `key` nimmt: Esc schließt,
    /// die Eingabe der Schnitthöhe nimmt die Tasten, die sie bearbeitet.
    pub(crate) fn zahnrad_will(&self, key: Key) -> bool {
        let esc = key == Key::Escape;
        (esc && self.ansicht_schatten.offen)
            || self.plan_schnitt.nimmt(key)
            || (esc && self.plan_schnitt.offen)
    }

    /// Ein Klick in die Paneele beendet die Eingabe der Schnitthöhe, sonst
    /// nähme sie dem Paneelfeld die Tasten (wie Review 3bx); `true`, wenn
    /// eine lief.
    pub(crate) fn zahnrad_ab(&mut self) -> bool {
        self.plan_schnitt.falsch = false;
        let lief = self.plan_schnitt.eingabe.take().is_some();
        self.redraw |= lief;
        lief
    }

    /// Taste an die Felder der Zahnräder (nach [`App::zahnrad_will`]).
    pub(crate) fn zahnrad_key(&mut self, key: Key, mods: Modifiers) {
        if key == Key::Escape && self.ansicht_schatten.offen {
            self.ansicht_schatten.offen = false;
            self.redraw = true;
            return;
        }
        let mut out = grundriss_schnitt::Ausgang::default();
        self.plan_schnitt.key(key, mods, &mut out);
        self.schnitt_anwenden(out);
    }

    /// Zeiger über den Zahnrädern: Textcursor über dem Wertfeld, sonst die
    /// Hand über einem Knopf; `None` daneben.
    pub(crate) fn zahnrad_cursor(&self) -> Option<sk_platform::Cursor> {
        match self.plan_schnitt.hover {
            Some(grundriss_schnitt::Teil::Wert) => Some(sk_platform::Cursor::IBeam),
            Some(_) => Some(sk_platform::Cursor::Hand),
            None => self
                .ansicht_schatten
                .hover
                .map(|_| sk_platform::Cursor::Hand),
        }
    }

    /// Tooltip an den Zahnrädern.
    pub(crate) fn zahnrad_tip(&self) -> Option<String> {
        if self.plan_schnitt.hover == Some(grundriss_schnitt::Teil::Zahnrad) {
            return Some(grundriss_schnitt::TIP.into());
        }
        match self.ansicht_schatten.unter {
            Some(ansicht_schatten::Teil::Zahnrad) => {
                let t = if self.renderer.shadow_failed() {
                    SCHATTEN_FEHLT
                } else {
                    ansicht_schatten::TIP
                };
                Some(t.into())
            }
            Some(ansicht_schatten::Teil::Licht(sk_model::ShadeLight::Sun))
                if !ansicht_schatten::sonne_waehlbar(self.scene.model().location()) =>
            {
                Some(ansicht_schatten::OHNE_NORD.into())
            }
            _ => None,
        }
    }

    /// Bilder der Zahnräder hochladen, wenn sie sich geändert haben (`th`:
    /// Höhe der Titelleiste, `scale`: Skalierung).
    pub(crate) fn zahnrad_bilder(&mut self, th: u32, scale: f32) {
        let loc = self.scene.model().location();
        let wahl = self.ansicht_wahl();
        let bild = wahl.map(|(i, vs)| ansicht_schatten::Bild {
            vs,
            gestrichelt: self.scene.model().view_below(i),
            eigen: self.scene.model().view_shade_own(i).is_some(),
            offen: self.ansicht_schatten.offen,
            hover: self.ansicht_schatten.hover,
            sonne_ok: ansicht_schatten::sonne_waehlbar(loc),
            zu_tief: vs.light == sk_model::ShadeLight::Sun
                && ansicht_schatten::sonne_waehlbar(loc)
                && ansicht_schatten::sonne_zu_tief(loc, self.ansicht_sonne()),
            rechts: self.ui.rect(Panel::Views, self.w, th).x.to_bits(),
            scale: scale.to_bits(),
        });
        if bild != self.schatten_bild {
            match &bild {
                Some(b) => {
                    let (c, x, y) = ansicht_schatten::malen(b, &self.ui.fonts, &self.theme);
                    let px = c.to_premul_rgba8();
                    let (w, h) = (c.width as u32, c.height as u32);
                    self.renderer
                        .set_overlay(OVERLAY_ANSICHT, x, y + th as i32, w, h, &px);
                }
                None => self.renderer.set_overlay(OVERLAY_ANSICHT, 0, 0, 0, 0, &[]),
            }
            self.schatten_bild = bild;
        }
        let m = self.scene.model();
        let bild = (self.ui.view == ViewKind::Plan).then(|| grundriss_schnitt::Bild {
            mm: m.plan_cut_height().to_bits(),
            eigen: m.plan_cut_own().is_some(),
            offen: self.plan_schnitt.offen,
            hover: self.plan_schnitt.hover,
            eingabe: self.plan_schnitt.eingabe.clone(),
            falsch: self.plan_schnitt.falsch,
            rechts: self.ui.rect(Panel::Views, self.w, th).x.to_bits(),
            scale: scale.to_bits(),
        });
        if bild != self.plan_schnitt_bild {
            match &bild {
                Some(b) => {
                    let (c, x, y) = grundriss_schnitt::malen(b, &self.ui.fonts, &self.theme);
                    let px = c.to_premul_rgba8();
                    let (w, h) = (c.width as u32, c.height as u32);
                    self.renderer
                        .set_overlay(OVERLAY_SCHNITT, x, y + th as i32, w, h, &px);
                }
                None => self.renderer.set_overlay(OVERLAY_SCHNITT, 0, 0, 0, 0, &[]),
            }
            self.plan_schnitt_bild = bild;
        }
    }
}

//! Zahnräder oben rechts in der Ansicht, links neben dem Paneel „Ansichten“:
//! „Schatten“ in den vier Ansichten (S7, [`ansicht_schatten`]),
//! „Schnitthöhe“ im Grundriss ([`grundriss_schnitt`]) und die Sichtbarkeit
//! in 3D und Schnitt ([`sicht_zahnrad`]); jedes Feld trägt das Häkchen
//! „Dachentwässerung ausblenden“ seiner Ansicht (Jörn 12.10.), die von
//! Grundriss, Schnitt und 3D dazu den Schalter „Ausbau innen“ (Raumbuch
//! RB4), Grundriss und 3D die Zeile „Bemaßung“ ([`crate::bemassung`]).
//! Hier die Verdrahtung in der App: Wahl, Lage, Ereignisse, Tasten,
//! Tooltip und Bilder.

use crate::scene::scene_ausbau::platz as ausbau_platz;
use crate::ui::{Panel, ViewKind};
use crate::{
    ansicht_schatten, bemassung, grundriss_schnitt, sicht_zahnrad, App, OVERLAY_ANSICHT,
    OVERLAY_SCHNITT, SCHATTEN_FEHLT,
};
use sk_platform::{Event, Key, Modifiers};

/// Bedienzustand der Zahnräder und ihre zuletzt hochgeladenen Bilder.
#[derive(Default)]
pub(crate) struct Zahnraeder {
    /// Schatten der vier Ansichten (S7).
    pub schatten: ansicht_schatten::Schalter,
    pub schatten_bild: Option<ansicht_schatten::Bild>,
    /// Schnitthöhe des Grundrisses.
    pub schnitt: grundriss_schnitt::Schalter,
    pub schnitt_bild: Option<grundriss_schnitt::Bild>,
    /// Sichtbarkeit in 3D und Schnitt; ihr Bild teilt den Platz mit dem
    /// der Ansichten.
    pub sicht: sicht_zahnrad::Schalter,
    pub sicht_bild: Option<sicht_zahnrad::Bild>,
}

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
            if self.zahnrad.schatten.offen || self.zahnrad.schatten.hover.is_some() {
                self.zahnrad.schatten.reset();
                self.redraw = true;
            }
            return false;
        };
        let lage = self.ansicht_lage(sc);
        let ok = ansicht_schatten::sonne_waehlbar(self.scene.model().location());
        let eigen = self.scene.model().view_shade_own(i).is_some();
        let out = self.zahnrad.schatten.handle(ev, &lage, vs, ok, eigen);
        self.redraw |= out.redraw;
        self.entw_anwenden(out.haken, out.alle);
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
            if self.zahnrad.schnitt.offen || self.zahnrad.schnitt.hover.is_some() {
                self.zahnrad.schnitt.reset();
                self.redraw = true;
            }
            return false;
        }
        let lage = self.schnitt_lage(sc);
        let m = self.scene.model();
        let (mm, eigen) = (m.plan_cut_height(), m.plan_cut_own().is_some());
        let out = self.zahnrad.schnitt.handle(ev, &lage, mm, eigen);
        self.schnitt_anwenden(out);
        self.entw_anwenden(out.haken, out.alle);
        self.ausbau_anwenden(out.ausbau, out.alle);
        self.mass_anwenden(out.ketten, out.punkte);
        out.consumed
    }

    /// Ereignis an Zahnrad und Feld „Sichtbarkeit“ (nur in 3D und Schnitt);
    /// `true`, wenn sie es genommen haben.
    fn sicht_handle(&mut self, ev: &Event, sc: f64) -> bool {
        if !sicht_zahnrad::zeigt(self.ui.view) {
            if self.zahnrad.sicht.offen || self.zahnrad.sicht.hover.is_some() {
                self.zahnrad.sicht.reset();
                self.redraw = true;
            }
            return false;
        }
        let rechts = self.ui.rect(Panel::Views, self.w, self.top()).x;
        let mass = bemassung::ansicht(self.ui.view);
        let lage = sicht_zahnrad::lage(rechts, sc as f32, &self.theme, mass);
        let out = self.zahnrad.sicht.handle(ev, &lage);
        self.redraw |= out.redraw;
        self.entw_anwenden(out.haken, out.alle);
        self.ausbau_anwenden(out.ausbau, out.alle);
        self.mass_anwenden(out.ketten, out.punkte);
        out.consumed
    }

    /// Häkchen „Maßketten zeigen“ bzw. „Maßbezugspunkte zeigen“ der
    /// gezeigten Ansicht umschalten (Ansichtszustand ohne Schritt).
    fn mass_anwenden(&mut self, ketten: bool, punkte: bool) {
        let v = self.ui.view;
        if (ketten || punkte) && bemassung::ansicht(v) {
            let (k, p) = self.scene.mass_sicht(v);
            self.scene.set_mass_sicht(v, k ^ ketten, p ^ punkte);
            self.redraw = true;
        }
    }

    /// Häkchen „Dachentwässerung ausblenden“ der gezeigten Ansicht
    /// umschalten (gleitet wie im Baum) bzw. ihre Wahl in alle sieben
    /// Ansichten übertragen (Jörn 12.10.).
    fn entw_anwenden(&mut self, haken: bool, alle: bool) {
        let v = self.ui.view;
        if haken && self.scene.set_drain_hidden(v, !self.scene.drain_hidden(v)) {
            self.upload_model();
            self.sync_props();
        }
        if alle {
            self.scene.drain_to_all(v);
        }
    }

    /// „Ausbau innen“ der gezeigten Ansicht schalten (`ausbau`) bzw. ihre
    /// Wahl in Grundriss, Schnitt und 3D übertragen (`alle`).
    fn ausbau_anwenden(&mut self, ausbau: Option<bool>, alle: bool) {
        let Some(i) = ausbau_platz(self.ui.view) else {
            return;
        };
        let an = ausbau.unwrap_or(self.scene.model().view_finish(i));
        let n = sk_model::FINISH_VIEWS.len();
        let welche = match (alle, ausbau) {
            (true, _) => 0..n,
            (false, Some(_)) => i..i + 1,
            (false, None) => return,
        };
        let mut neu = false;
        for k in welche {
            if self.scene.model().view_finish(k) != an {
                self.scene.set_view_finish(k, an);
                neu = true;
            }
        }
        if neu {
            self.upload_model();
        }
    }

    /// Beim Wechsel der Ansicht: Felder zu, die Schnelloption der neuen
    /// Ansicht gilt sofort; `true`, wenn sie das Netz ändert (der Aufrufer
    /// lädt es dann einmal hoch, Review H1).
    pub(crate) fn zahnrad_reset(&mut self) -> bool {
        self.zahnrad.schatten.reset();
        self.zahnrad.schnitt.reset();
        self.zahnrad.sicht.reset();
        self.scene.sicht_fuer(self.ui.view)
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
        self.ansicht_handle(ev, sc) | self.schnitt_handle(ev, sc) | self.sicht_handle(ev, sc)
    }

    /// Ob ein offenes Feld der Zahnräder die Taste `key` nimmt: Esc schließt,
    /// die Eingabe der Schnitthöhe nimmt die Tasten, die sie bearbeitet.
    pub(crate) fn zahnrad_will(&self, key: Key) -> bool {
        let esc = key == Key::Escape;
        (esc && (self.zahnrad.schatten.offen || self.zahnrad.sicht.offen))
            || self.zahnrad.schnitt.nimmt(key)
            || (esc && self.zahnrad.schnitt.offen)
    }

    /// Ein Klick in die Paneele beendet die Eingabe der Schnitthöhe, sonst
    /// nähme sie dem Paneelfeld die Tasten (wie Review 3bx); `true`, wenn
    /// eine lief.
    pub(crate) fn zahnrad_ab(&mut self) -> bool {
        self.zahnrad.schnitt.falsch = false;
        let lief = self.zahnrad.schnitt.eingabe.take().is_some();
        self.redraw |= lief;
        lief
    }

    /// Taste an die Felder der Zahnräder (nach [`App::zahnrad_will`]).
    pub(crate) fn zahnrad_key(&mut self, key: Key, mods: Modifiers) {
        if key == Key::Escape && (self.zahnrad.schatten.offen || self.zahnrad.sicht.offen) {
            self.zahnrad.schatten.offen = false;
            self.zahnrad.sicht.offen = false;
            self.redraw = true;
            return;
        }
        let mut out = grundriss_schnitt::Ausgang::default();
        self.zahnrad.schnitt.key(key, mods, &mut out);
        self.schnitt_anwenden(out);
    }

    /// Zeiger über den Zahnrädern: Textcursor über dem Wertfeld, sonst die
    /// Hand über einem Knopf; `None` daneben.
    /// Dazu die Hand über Maßzahlen und Maßlinien ([`crate::wall_edit::mass_edit`]).
    pub(crate) fn zahnrad_cursor(&self) -> Option<sk_platform::Cursor> {
        match self.zahnrad.schnitt.hover {
            Some(grundriss_schnitt::Teil::Wert) => Some(sk_platform::Cursor::IBeam),
            Some(_) => Some(sk_platform::Cursor::Hand),
            None => (self.zahnrad.schatten.hover.is_some() || self.zahnrad.sicht.hover.is_some())
                .then_some(sk_platform::Cursor::Hand)
                .or_else(|| self.edit.masse.cursor()),
        }
    }

    /// Tooltip an den Zahnrädern und an den Maßketten.
    pub(crate) fn zahnrad_tip(&self) -> Option<String> {
        match (self.zahnrad.schnitt.hover, self.zahnrad.sicht.hover) {
            (Some(grundriss_schnitt::Teil::Zahnrad), _) => {
                return Some(grundriss_schnitt::TIP.into())
            }
            (_, Some(sicht_zahnrad::Teil::Zahnrad)) => return Some(ansicht_schatten::TIP.into()),
            (Some(grundriss_schnitt::Teil::Ausbau(_)), _)
            | (_, Some(sicht_zahnrad::Teil::Ausbau(_))) => {
                return Some(sicht_zahnrad::AUSBAU_TIP.into())
            }
            _ => {}
        }
        match self.zahnrad.schatten.unter {
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
            // Maßzahl oder Maßlinie unter der Maus
            _ => self.edit.masse.tip().map(String::from),
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
            entw: self.scene.drain_hidden(self.ui.view),
            eigen: self.scene.model().view_shade_own(i).is_some(),
            offen: self.zahnrad.schatten.offen,
            hover: self.zahnrad.schatten.hover,
            sonne_ok: ansicht_schatten::sonne_waehlbar(loc),
            zu_tief: vs.light == sk_model::ShadeLight::Sun
                && ansicht_schatten::sonne_waehlbar(loc)
                && ansicht_schatten::sonne_zu_tief(loc, self.ansicht_sonne()),
            rechts: self.ui.rect(Panel::Views, self.w, th).x.to_bits(),
            scale: scale.to_bits(),
        });
        let ausbau = |v| ausbau_platz(v).is_some_and(|i| self.scene.model().view_finish(i));
        let sicht = sicht_zahnrad::zeigt(self.ui.view).then(|| sicht_zahnrad::Bild {
            an: self.scene.drain_hidden(self.ui.view),
            ausbau: ausbau(self.ui.view),
            offen: self.zahnrad.sicht.offen,
            hover: self.zahnrad.sicht.hover,
            mass: bemassung::ansicht(self.ui.view).then(|| self.scene.mass_sicht(self.ui.view)),
            rechts: self.ui.rect(Panel::Views, self.w, th).x.to_bits(),
            scale: scale.to_bits(),
        });
        // Ansichten und 3D/Schnitt schließen einander aus: ein Platz
        if bild != self.zahnrad.schatten_bild || sicht != self.zahnrad.sicht_bild {
            let (f, t) = (&self.ui.fonts, &self.theme);
            let neu = match (&bild, &sicht) {
                (Some(b), _) => Some(ansicht_schatten::malen(b, f, t)),
                (None, Some(b)) => Some(sicht_zahnrad::malen(b, f, t)),
                (None, None) => None,
            };
            match neu {
                Some((c, x, y)) => {
                    let px = c.to_premul_rgba8();
                    let (w, h) = (c.width as u32, c.height as u32);
                    self.renderer
                        .set_overlay(OVERLAY_ANSICHT, x, y + th as i32, w, h, &px);
                }
                None => self.renderer.set_overlay(OVERLAY_ANSICHT, 0, 0, 0, 0, &[]),
            }
            self.zahnrad.schatten_bild = bild;
            self.zahnrad.sicht_bild = sicht;
        }
        let m = self.scene.model();
        let bild = (self.ui.view == ViewKind::Plan).then(|| grundriss_schnitt::Bild {
            mm: m.plan_cut_height().to_bits(),
            eigen: m.plan_cut_own().is_some(),
            offen: self.zahnrad.schnitt.offen,
            hover: self.zahnrad.schnitt.hover,
            eingabe: self.zahnrad.schnitt.eingabe.clone(),
            falsch: self.zahnrad.schnitt.falsch,
            entw: self.scene.drain_hidden(ViewKind::Plan),
            ausbau: ausbau(ViewKind::Plan),
            mass: self.scene.mass_sicht(ViewKind::Plan),
            rechts: self.ui.rect(Panel::Views, self.w, th).x.to_bits(),
            scale: scale.to_bits(),
        });
        if bild != self.zahnrad.schnitt_bild {
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
            self.zahnrad.schnitt_bild = bild;
        }
    }
}

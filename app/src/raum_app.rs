//! Angaben auf dem Papier des Grundrisses in der App: Terrassenangaben
//! (Review 3f K1, aus `main.rs` hierher) und Raumstempel (Grundriss G4,
//! [`stempel`]) mit ihren Ereignissen, Tasten und Bildern.

#[path = "grundriss/stempel.rs"]
pub(crate) mod stempel;

use crate::terrace_label::{self, Box};
use crate::ui::ViewKind;
use crate::{App, OVERLAY_ROOMS, OVERLAY_STEMPEL, ROOMS, STEMPEL};
use sk_platform::{Event, Key, Modifiers};

/// Raumstempel mit den hochgeladenen Bildern je Platz.
#[derive(Default)]
pub(crate) struct Raumstempel {
    pub(crate) s: stempel::Stempel,
    bilder: Vec<Option<stempel::Schluessel>>,
    liste: Option<stempel::ListenSchluessel>,
}

impl App {
    /// Stempel und Abläufe im Grundriss (Stempel zuerst: sie liegen im Raum,
    /// die Abläufe am Rand). `true`, wenn eins das Ereignis nahm.
    pub(crate) fn griffe_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        let aktiv = self.ui.view == ViewKind::Plan && !self.tool.enabled;
        let out = self.stempel.s.handle(
            ev,
            &mut self.scene,
            &self.cam,
            vw,
            vh,
            sc,
            &self.ui.fonts,
            &self.theme,
            aktiv,
        );
        self.stempel_folgen(out);
        if out.consumed {
            self.drain_handle(&Event::MouseLeave, vw, vh, sc);
            return true;
        }
        self.drain_handle(ev, vw, vh, sc)
    }

    fn stempel_folgen(&mut self, out: stempel::Ausgang) {
        self.redraw |= out.redraw;
        if out.changed {
            self.props_key = None;
        }
        if let Some(id) = out.gewaehlt {
            self.select(Some(id));
            self.props_key = None;
        }
    }

    /// Ruht das Paneel „Eigenschaften“? Beim Ziehen einer Wand, einer Ebene
    /// oder eines Raumstempels (Review #112 B1) und beim Gleiten („Bündig
    /// setzen“) bleibt es auf dem Stand davor.
    pub(crate) fn paneel_ruht(&self) -> bool {
        self.edit.is_dragging()
            || self.level_dragging().is_some()
            || self.flush_anim.is_some()
            || self.stempel.s.zieht()
    }

    /// Steht die Maus auf einem Stempel oder Ablauf, oder wird einer gezogen?
    pub(crate) fn griffe_busy(&self) -> bool {
        self.drains.is_busy() || self.stempel.s.is_busy()
    }

    /// Nimmt ein offenes Feld die Taste (Zahnräder, Namensfeld)?
    pub(crate) fn feld_will(&self, key: Key) -> bool {
        self.zahnrad_will(key) || self.stempel.s.will(key)
    }

    /// Taste an das Feld, das sie nimmt (nach [`App::feld_will`]).
    pub(crate) fn feld_key(&mut self, key: Key, mods: Modifiers) {
        if self.stempel.s.will(key) {
            let out = self.stempel.s.key(key, mods, &mut self.scene);
            self.stempel_folgen(out);
        } else {
            self.zahnrad_key(key, mods);
        }
    }

    /// Ein Klick in die Paneele beendet die Eingaben der Zahnräder und das
    /// Namensfeld (übernimmt den Namen); `true`, wenn eins lief.
    pub(crate) fn feld_ab(&mut self) -> bool {
        let lief = self.stempel.s.namen.is_some();
        if self.stempel.s.ab(&mut self.scene) {
            self.props_key = None;
        }
        self.redraw |= lief;
        self.zahnrad_ab() | lief
    }

    /// Getipptes Zeichen ins Namensfeld des Raumstempels.
    pub(crate) fn stempel_text(&mut self, ch: char) {
        self.redraw |= self.stempel.s.text(ch);
    }

    /// Zeiger über Stempeln und Zahnrädern; `None` daneben.
    pub(crate) fn feld_cursor(&self) -> Option<sk_platform::Cursor> {
        let maus = self.mouse_at.map(|(x, y)| (x, y - self.top() as f64));
        let plan = self.ui.view == ViewKind::Plan;
        self.stempel
            .s
            .cursor(maus)
            .filter(|_| plan)
            .or_else(|| self.zahnrad_cursor())
    }

    /// Angaben auf dem Papier des Grundrisses hochladen, wenn sie sich
    /// geändert haben (`th`: Höhe der Titelleiste).
    pub(crate) fn raumangaben(&mut self, th: u32, scale: f32, vw: f64, vh: f64) {
        let plan = self.ui.view == ViewKind::Plan;
        let chips: Vec<Box> = if plan {
            self.chips
                .iter()
                .filter(|k| !k.linked)
                .map(|k| terrace_label::chip_box(k, scale as f64, th as f64))
                .collect()
        } else {
            Vec::new()
        };
        // Nicht unter den Paneelen
        let panels: Vec<Box> = self
            .ui
            .panel_rects(self.w, th)
            .iter()
            .map(|r| Box {
                x: r.x as f64,
                y: (r.y - th as f32) as f64,
                w: r.w as f64,
                h: r.h as f64,
            })
            .collect();
        self.terrassenangaben(th, scale, vw, vh, &chips, &panels);
        let mut gesperrt = panels;
        gesperrt.extend(chips);
        self.stempelbilder(th, scale, vw, vh, &gesperrt);
    }

    /// „Dachterrasse 13,22 m²“ auf der Terrasse, gedimmt wie eine
    /// Raumangabe, nach der Platzregel (Review 3f K1): nie über Wand,
    /// Attika oder Blech, 6 dip neben dem Kettensymbol.
    fn terrassenangaben(
        &mut self,
        th: u32,
        scale: f32,
        vw: f64,
        vh: f64,
        chips: &[Box],
        panels: &[Box],
    ) {
        let rooms: Vec<((f64, f64), String)> = if self.ui.view == ViewKind::Plan {
            let pad = terrace_label::PAD * scale as f64;
            terrace_label::labels(
                &self.scene,
                &self.cam,
                vw,
                vh,
                scale as f64,
                &self.ui.fonts,
                &self.theme,
                chips,
                panels,
            )
            .into_iter()
            .filter_map(|(p, text)| {
                let (b, _) = p.shown()?;
                Some(((b.x + pad, b.y + pad), text))
            })
            .take(ROOMS)
            .collect()
        } else {
            Vec::new()
        };
        for i in 0..ROOMS {
            match rooms.get(i) {
                Some(((x, y), text)) => {
                    let key = (text.clone(), scale.to_bits(), self.theme.rev);
                    let (px_x, px_y) = (x.round() as i32, (y + th as f64).round() as i32);
                    match &self.room_keys[i] {
                        Some((k, _, _)) if *k == key => {
                            self.renderer.move_overlay(OVERLAY_ROOMS + i, px_x, px_y);
                        }
                        _ => {
                            let c = terrace_label::paint(
                                &self.ui.fonts,
                                text,
                                scale as f64,
                                &self.theme,
                            );
                            let (w, h) = (c.width as u32, c.height as u32);
                            let px = c.to_premul_rgba8();
                            self.renderer
                                .set_overlay(OVERLAY_ROOMS + i, px_x, px_y, w, h, &px);
                            self.room_keys[i] = Some((key, w, h));
                        }
                    }
                }
                None => {
                    if self.room_keys[i].take().is_some() {
                        self.renderer
                            .set_overlay(OVERLAY_ROOMS + i, 0, 0, 0, 0, &[]);
                    }
                }
            }
        }
    }

    /// Raumstempel des aktiven Geschosses (G4) und die Vorschlagsliste des
    /// offenen Namensfelds.
    fn stempelbilder(&mut self, th: u32, scale: f32, vw: f64, vh: f64, gesperrt: &[Box]) {
        let plan = self.ui.view == ViewKind::Plan;
        // Neu erkannter Raum: das Namensfeld gleich offen (J10), nicht
        // mitten in einem Wandzug und nicht neben dem Nordpfeil, der nach
        // dem ersten Umriss Zahlen nimmt
        if let Some(id) = self.scene.neuer_raum() {
            if plan && !self.tool.is_active() && !self.nord.aktiv {
                self.stempel.s.oeffnen(&self.scene, id);
            }
        }
        let st = &mut self.stempel.s;
        if plan {
            st.auslegen(
                &self.scene,
                &self.cam,
                vw,
                vh,
                scale as f64,
                &self.ui.fonts,
                &self.theme,
                gesperrt,
            );
        } else {
            // Andere Ansicht: der Name gilt, das Feld nimmt keine Tasten mehr
            if st.ab(&mut self.scene) {
                self.props_key = None;
            }
            st.bilder.clear();
            st.liste = None;
            st.hover = None;
        }
        let (sel, rev) = (self.sel.id, self.theme.rev);
        self.stempel.bilder.resize(STEMPEL, None);
        for i in 0..STEMPEL {
            let neu = self.stempel.s.bild(i, sel, scale as f64, rev);
            let platz = OVERLAY_STEMPEL + i;
            match neu {
                Some((k, [x, y])) => {
                    let (px, py) = (x.round() as i32, (y + th as f64).round() as i32);
                    if self.stempel.bilder[i].as_ref() == Some(&k) {
                        self.renderer.move_overlay(platz, px, py);
                    } else {
                        let c = stempel::malen(&k, &self.ui.fonts, &self.theme);
                        let b = c.to_premul_rgba8();
                        let (w, h) = (c.width as u32, c.height as u32);
                        self.renderer.set_overlay(platz, px, py, w, h, &b);
                        self.stempel.bilder[i] = Some(k);
                    }
                }
                None => {
                    if self.stempel.bilder[i].take().is_some() {
                        self.renderer.set_overlay(platz, 0, 0, 0, 0, &[]);
                    }
                }
            }
        }
        let platz = OVERLAY_STEMPEL + STEMPEL;
        match self.stempel.s.liste_bild(scale as f64, rev) {
            Some((k, [x, y])) => {
                let (px, py) = (x.round() as i32, (y + th as f64).round() as i32);
                if self.stempel.liste.as_ref() == Some(&k) {
                    self.renderer.move_overlay(platz, px, py);
                } else {
                    let c = stempel::liste_malen(&k, &self.ui.fonts, &self.theme);
                    let b = c.to_premul_rgba8();
                    let (w, h) = (c.width as u32, c.height as u32);
                    self.renderer.set_overlay(platz, px, py, w, h, &b);
                    self.stempel.liste = Some(k);
                }
            }
            None => {
                if self.stempel.liste.take().is_some() {
                    self.renderer.set_overlay(platz, 0, 0, 0, 0, &[]);
                }
            }
        }
    }
}

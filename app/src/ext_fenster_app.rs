//! Fenster „Erweiterungen“ und „Bauteil einlesen …“ in der App (E5): Datei
//! wählen und prüfen, Antworten des Fensters ausführen (Ordner schreiben,
//! Ein/Aus, Entfernen, das Projekt aktualisieren), danach Werkzeug, Katalog
//! und Liste angleichen. Das Fenster selbst in [`crate::ext_verwaltung`],
//! Ordner und Prüfung in [`crate::ext_ablage`].

use crate::ext_ablage::{self, Ablage};
use crate::ext_verwaltung::{self as ev, Antwort, Fenster, Frage, Tat};
use crate::{meldung, menu, App, NOTICE_TIME, OVERLAY_EXT, OVERLAY_EXT_SCRIM};
use sk_model::erweiterung::{anzeige, ExtDef};
use sk_platform::{Event, MouseButton, Surface};

/// Dateifilter beim Einlesen.
const FILTER: [(&str, &str); 2] = [("Bauteil (*.szb)", "*.szb"), ("Alle Dateien (*.*)", "*.*")];

impl App {
    /// Fenster „Erweiterungen“ öffnen (Datei › Erweiterungen …).
    pub(crate) fn ext_fenster_auf(&mut self) {
        if self.ext_fenster.is_some() {
            return;
        }
        self.ext_liste_zu();
        let z = ev::zeilen(&self.ext_ablage, self.scene.model());
        self.ext_fenster = Some(Fenster::new(z));
        self.tip = None;
        self.overlay_dirty = true;
    }

    /// Thema der Hilfe bei offenem Fenster (E9): beim Einlesen eigenes.
    pub(crate) fn ext_thema(&self) -> crate::help::Topic {
        match self
            .ext_fenster
            .as_ref()
            .is_some_and(Fenster::beim_einlesen)
        {
            true => crate::help::Topic::Einlesen,
            false => crate::help::Topic::Ext,
        }
    }

    /// „Bauteil einlesen …“: Datei wählen, prüfen, Rückfrage zeigen.
    pub(crate) fn ext_einlesen(&mut self, surface: &Surface) {
        let Some(p) = surface.open_dialog("Bauteil einlesen", &FILTER) else {
            return;
        };
        let datei = p
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let f = self.ext_frage_zur_datei(&datei, ext_ablage::datei_text(&p));
        self.ext_frage_zeigen(f);
    }

    /// Rückfrage zum Text einer Datei (`Err`: nicht lesbar).
    pub(crate) fn ext_frage_zur_datei(&self, datei: &str, text: Result<String, String>) -> Frage {
        let v = text
            .map_err(|e| vec![e])
            .and_then(|t| ext_ablage::pruefen(&t, &self.ext_ablage, self.scene.model()))
            .map(|mut v| {
                // Folgen der genutzten Bauleistungen im wirksamen Katalog
                let firma = self.company.as_ref().map(|c| c.library());
                let k = sk_cost::lesen::katalog(self.scene.model(), firma);
                v.hinweise
                    .extend(sk_cost::erweiterung::fehlende_folgen(&k, &v.def));
                ext_ablage::gewerke_ohne_titel(&mut v, &self.ext_ablage, self.scene.model(), firma);
                // Neue Sätze für den Firmenkatalog (E8c)
                if let Some(f) = firma {
                    v.saetze = sk_cost::neue_saetze::neue_saetze(self.scene.model(), f, &v.def);
                }
                v
            });
        match v {
            Ok(v) => Frage::einlesen(v),
            Err(e) => Frage::fehler(datei, e),
        }
    }

    pub(crate) fn ext_frage_zeigen(&mut self, f: Frage) {
        match self.ext_fenster.as_mut() {
            Some(w) => w.frage(f),
            None => {
                self.ext_liste_zu();
                let z = ev::zeilen(&self.ext_ablage, self.scene.model());
                self.ext_fenster = Some(Fenster::mit_frage(z, f));
            }
        }
        self.tip = None;
        self.overlay_dirty = true;
    }

    /// Ordner neu gelesen oder geändert: Werkzeug, Katalog und Liste folgen;
    /// `key` wird in der Liste gewählt, `satz` steht im Fuß.
    fn ext_nach_tat(&mut self, key: Option<&str>, satz: String) {
        self.ext_bib = self.ext_ablage.bibliothek();
        if let Some(c) = self.catalog.as_mut() {
            c.set_ext(&self.ext_bib.defs);
            self.prefs_dirty = true;
        }
        self.ext_sync();
        let z = ev::zeilen(&self.ext_ablage, self.scene.model());
        if let Some(w) = self.ext_fenster.as_mut() {
            w.setze_zeilen(z, key);
            w.meldung = Some(satz);
        }
        self.sync_ui();
        self.sync_props();
        self.overlay_dirty = true;
        self.redraw = true;
    }

    /// Das Projekt auf `d` bringen: ein Rückgängig-Schritt.
    fn ext_ins_projekt(&mut self, d: ExtDef) -> Result<(), String> {
        let mut fehler = None;
        self.scene
            .edit_model("Erweiterung aktualisiert", |m| match m.put_ext_def(d) {
                Ok(()) => true,
                Err(e) => {
                    fehler = Some(e.to_string());
                    false
                }
            });
        fehler.map_or(Ok(()), Err)
    }

    fn ext_fehler(&mut self, text: String) {
        match self.ext_fenster.as_mut() {
            Some(w) => {
                w.meldung = Some(text);
                self.overlay_dirty = true;
            }
            None => self.status(meldung::Meldung::mit("{}", &[&text]), NOTICE_TIME),
        }
    }

    pub(crate) fn ext_antwort(&mut self, a: Antwort, surface: &Surface) {
        match a {
            Antwort::Zu => {
                self.ext_fenster = None;
                self.overlay_dirty = true;
            }
            Antwort::Einlesen => self.ext_einlesen(surface),
            Antwort::Schalten(key, an) => match self.ext_ablage.schalten(&key, an) {
                Ok(()) => {
                    let wort = if an { "eingeschaltet" } else { "ausgeschaltet" };
                    let name = self.ext_name(&key);
                    self.ext_nach_tat(Some(&key), format!("„{name}“ {wort}."));
                }
                Err(e) => self.ext_fehler(e),
            },
            Antwort::Aktualisieren(key) => {
                let Some(e) = self.ext_ablage.eintrag(&key) else {
                    return;
                };
                let d = e.def.clone();
                let alt = self.scene.model().ext_def(&key).map_or(0, |o| o.version);
                match ext_ablage::aenderungen(self.scene.model(), &d) {
                    Err(e) => self.ext_fehler(e),
                    Ok(z) if z.is_empty() => self.ext_tat(Tat::Aktualisieren(Box::new(d))),
                    Ok(z) => self.ext_frage_zeigen(Frage::aktualisieren(d, alt, z)),
                }
            }
            Antwort::Uebernehmen(key) => {
                let Some(d) = self.scene.model().ext_def(&key).cloned() else {
                    return;
                };
                let f = self.ext_frage_zur_datei(&key, Ok(d.text));
                self.ext_frage_zeigen(f);
            }
            Antwort::Baustoffe(key) => {
                let mut n = 0;
                self.scene.edit_model("Baustoffe angelegt", |m| {
                    n = m.ext_baustoffe_nachlegen(&key);
                    n > 0
                });
                let satz = match n {
                    0 => "Keine Baustoffe angelegt.".to_string(),
                    1 => "1 Baustoff angelegt.".to_string(),
                    n => format!("{n} Baustoffe angelegt."),
                };
                self.ext_nach_tat(Some(&key), satz);
            }
            Antwort::Tat(t) => self.ext_tat(t),
        }
    }

    /// Die angehakten neuen Sätze an die Firma (E8c, verwaltung.md §8a):
    /// mit Verwaltungskennwort als Vorschlag in den Entwurf, am Einzelplatz
    /// direkt mit Pille „unbestätigt“. Der Satz für die Meldung.
    fn ext_saetze_fuer_firma(&mut self, v: &crate::ext_ablage::Vorschlag) -> Option<String> {
        use sk_cost::neue_saetze::{quelle_text, Stand};
        let saetze: Vec<sk_cost::SatzNeu> = v
            .saetze
            .iter()
            .filter(|n| n.stand == Stand::Neu)
            .map(|n| sk_cost::SatzNeu {
                rec: n.rec,
                of: n.guid,
                kurz: n.name.clone(),
                zeilen: n.zeilen.clone(),
            })
            .collect();
        if saetze.is_empty() {
            return None;
        }
        let n = saetze.len();
        let c = self.company.as_mut()?;
        let mut h = sk_cost::Herkunft::jetzt(sk_cost::HerkunftArt::Import);
        h.quelle = quelle_text(&v.def);
        let mit_kennwort = sk_cost::verwaltung::hat_kennwort(c.library());
        let projekt = mit_kennwort.then(|| (self.scene.model().project().guid, self.doc.name()));
        let op = sk_cost::Op::SaetzeAusErweiterung {
            projekt,
            quelle: quelle_text(&v.def),
            saetze,
        };
        let anzahl = if n == 1 {
            "Ein neuer Satz".to_string()
        } else {
            format!("{n} neue Sätze")
        };
        Some(if mit_kennwort {
            match c.vorschlagen(&h, &op) {
                Ok(()) => format!("{anzahl} der Verwaltung vorgeschlagen."),
                Err(e) => format!("Nichts vorgeschlagen: {e}"),
            }
        } else {
            match c.fuer_firma(&h, std::slice::from_ref(&op)) {
                Ok(_) => format!("{anzahl} im Firmenkatalog, unbestätigt."),
                Err(e) => format!("Firmenkatalog nicht geändert: {e}"),
            }
        })
    }

    fn ext_name(&self, key: &str) -> String {
        let d = self
            .ext_ablage
            .eintrag(key)
            .map(|e| &e.def)
            .or_else(|| self.scene.model().ext_def(key));
        d.map_or(key.to_string(), |d| anzeige(d.name(), 60))
    }

    fn ext_tat(&mut self, t: Tat) {
        match t {
            Tat::Einlesen(v) => {
                let v = *v;
                let key = v.def.key.clone();
                let name = anzeige(v.def.name(), 60);
                if let Err(e) = self.ext_ablage.schreiben(&v.def, v.hinweise.clone()) {
                    self.ext_fehler(format!("„{name}“ nicht eingelesen: {e}"));
                    return;
                }
                let mut satz = format!("„{name}“ Version {} eingelesen.", v.def.version);
                if let Some(s) = self.ext_saetze_fuer_firma(&v) {
                    satz.push(' ');
                    satz.push_str(&s);
                }
                if v.projekt.is_some() {
                    match self.ext_ins_projekt(v.def) {
                        Ok(()) => satz.push_str(" Das Projekt folgt."),
                        Err(e) => satz.push_str(&format!(" Projekt unverändert: {e}")),
                    }
                }
                if self.ext_fenster.is_none() {
                    self.ext_fenster = Some(Fenster::new(Vec::new()));
                }
                self.ext_nach_tat(Some(&key), satz);
            }
            Tat::Aktualisieren(d) => {
                let key = d.key.clone();
                let satz = format!(
                    "Projekt auf Version {} von „{}“ gebracht.",
                    d.version,
                    anzeige(d.name(), 60)
                );
                match self.ext_ins_projekt(*d) {
                    Ok(()) => self.ext_nach_tat(Some(&key), satz),
                    Err(e) => self.ext_fehler(e),
                }
            }
            Tat::Entfernen(key) => {
                let name = self.ext_name(&key);
                match self.ext_ablage.entfernen(&key) {
                    Ok(()) => self.ext_nach_tat(None, format!("„{name}“ entfernt.")),
                    Err(e) => self.ext_fehler(e),
                }
            }
        }
    }

    /// Offenes Fenster: `true`, wenn es das Ereignis genommen hat.
    pub(crate) fn handle_ext_fenster(&mut self, e: Event, surface: &Surface) -> bool {
        let (s, th) = (self.title.scale, self.title.height());
        let Some(w) = self.ext_fenster.as_mut() else {
            return false;
        };
        let r = w.rect(s, self.w, self.h, th);
        let antwort = match e {
            Event::MouseMove { x, y, .. } => {
                self.mouse_at = Some((x, y));
                self.overlay_dirty |= w.mouse_move(r, s, x, y);
                None
            }
            Event::MouseDown { y, .. } | Event::MouseUp { y, .. } if y < th as f64 => {
                return false;
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                w.press(r, s, x, y);
                self.overlay_dirty = true;
                None
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.overlay_dirty = true;
                w.release(r, s, x, y)
            }
            Event::Wheel { delta, .. } => {
                self.overlay_dirty |= w.wheel(r, s, delta);
                None
            }
            Event::Key { key, down, .. } => {
                if !down {
                    return true;
                }
                self.overlay_dirty = true;
                w.key(key)
            }
            Event::MouseDown { .. } | Event::MouseUp { .. } | Event::Text(_) => None,
            _ => return false,
        };
        if let Some(a) = antwort {
            self.ext_antwort(a, surface);
        }
        true
    }

    /// Fenster samt Abdunkeln zeichnen oder ausblenden.
    pub(crate) fn paint_ext_fenster(&mut self) {
        let Some(w) = &self.ext_fenster else {
            self.renderer.set_overlay(OVERLAY_EXT, 0, 0, 0, 0, &[]);
            self.renderer
                .set_overlay(OVERLAY_EXT_SCRIM, 0, 0, 0, 0, &[]);
            return;
        };
        let (s, th) = (self.title.scale, self.title.height());
        let r = w.rect(s, self.w, self.h, th);
        let c = w.paint(&self.theme, &self.ui.fonts, s, r);
        let m = (self.theme.size.panel_shadow * s).round();
        let px = c.to_premul_rgba8();
        self.renderer.set_overlay(
            OVERLAY_EXT,
            (r.x - m) as i32,
            (r.y - m) as i32,
            c.width as u32,
            c.height as u32,
            &px,
        );
        let scrim = menu::scrim_premul(self.theme.env.scrim);
        let h = self.h.saturating_sub(th);
        self.renderer
            .set_overlay_fill(OVERLAY_EXT_SCRIM, 0, th as i32, self.w, h, scrim);
    }
}

/// Ablage aus dem Ordner neben den Einstellungen; ohne Einstellungen
/// (Tests, Bildschirmfotos) leer.
pub fn ablage(einstellungen: Option<&std::path::Path>) -> Ablage {
    match einstellungen.and_then(|p| p.parent()) {
        Some(dir) => Ablage::lesen(&dir.join(crate::ext_app::ORDNER)),
        None => Ablage::default(),
    }
}

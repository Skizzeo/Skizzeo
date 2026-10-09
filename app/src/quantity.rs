//! Mengenfenster (F2, B7): eigenes Programmfenster mit derselben Titelleiste
//! wie das Hauptfenster; darunter das Blatt der Mengenermittlung
//! ([`ListView`]). Hover und Auswahl laufen über den gemeinsamen Zustand.
//! Entf, Kontextmenü und Hinweis nach dem Löschen wie im Hauptfenster (H119).
//! Über den Blättern die Kartenleiste (KA-2a): Mengen und Kosten
//! ([`KostenView`]) mit ihrer lebenden Zahl; ein Klick wechselt das Blatt.

use crate::ava_view::AvaView;
use crate::cards::{self, Blatt, Karten};
use crate::delete::{Action, ContextMenu, HintCard, Link};
use crate::kosten_view::{self, KostenView};
use crate::picking::Picking;
use crate::scene::Scene;
use crate::schedule_view::{Grouping, ListOut, ListView, RowBand};
use sk_model::qto::Schedule;
use sk_model::{Deleted, ElementId};
use sk_paint::Canvas;
use sk_platform::{CaptionArea, Event, Key, MouseButton, WindowCommand};
use sk_ui::theme::Theme;
use sk_ui::titlebar::{Button, TitleBar};
use sk_ui::widgets::Fonts;
use std::time::{Duration, Instant};

/// Fuge zum Hauptfenster (dip) und Dauer ihres Aufblinkens beim Einrasten.
const SEAM: f32 = 2.0;
const SEAM_FLASH: Duration = Duration::from_millis(150);

/// Fensterbild zum Zeigen: Bytes und, falls nur ein Teil neu ist, dessen
/// Zeilen (von, bis).
pub type Frame<'a> = (&'a [u8], Option<(u32, u32)>);

/// Was ein Ereignis im Mengenfenster für die App bedeutet.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Hover oder Auswahl im gemeinsamen Zustand geändert (`selection`:
    /// die Auswahl, sonst nur der Hover).
    Picking {
        selection: bool,
    },
    /// Doppelklick: die aktive Ansicht holt die Bauteile ins Bild.
    Zoom(Vec<ElementId>),
    SaveCsv,
    /// AVA-Druckvorschau: „Als PDF speichern“.
    SavePdf,
    /// AVA-Druckvorschau: Titelblatt und Inhaltsverzeichnis geändert.
    BlattWahl((bool, bool)),
    Command(WindowCommand),
    Close,
    /// Entf: die gemeinsame Auswahl löschen.
    Delete,
    /// Rechtsklick (Fensterpixel): Menü an der Zeile öffnen.
    OpenContext {
        x: f64,
        y: f64,
    },
    /// Befehl aus dem Menü: Bauteil des Menüs, Bauteile der Zeile.
    Action(Action, ElementId, Vec<ElementId>),
    /// Verweis im Hinweis.
    Link(Link),
    /// Preisblatt: Preis oder Firmenpreis schreiben (KA-2c).
    Kosten(kosten_view::Schreiben),
    /// Verwaltung mit dieser Bauleistung öffnen (KA-3a2).
    Verwaltung(sk_model::Guid),
    /// Nur die Abfrage des Verwaltungskennworts (Bedienbarkeit 16.1).
    Kennwort,
    /// Ablauf `kind=user` für dieses Haus (paket-ka3b §3).
    Ablauf(sk_model::Guid),
    /// Maske „Projektdaten“ im Hauptfenster, Cursor in diesem Feld.
    Projektdaten(usize),
}

pub struct QuantityWindow {
    /// Fenster offen (aus Sicht der App).
    pub open: bool,
    pub w: u32,
    pub h: u32,
    pub title: TitleBar,
    pub list: Option<ListView>,
    /// Kartenleiste und das Blatt Kosten (KA-2).
    pub karten: Karten,
    pub kosten: Option<KostenView>,
    pub ava: Option<AvaView>,
    /// Dateiname für das Bauvorhaben im AVA-Kopf („haus.szo“).
    pub datei: String,
    /// Gliederung der Liste (Paket 1b), aus den Einstellungen.
    pub grouping: Grouping,
    /// Druckvorschau: Titelblatt und Inhaltsverzeichnis, aus den
    /// Einstellungen.
    pub blatt_wahl: (bool, bool),
    /// Muss neu gezeichnet und gezeigt werden.
    pub dirty: bool,
    /// Angedockt (für die Fuge) und seit wann die Fuge aufblinkt.
    pub docked: bool,
    seam_flash: Option<Instant>,
    /// Im letzten Bild leuchtete etwas auf (dann noch ein Schlussbild).
    was_busy: bool,
    /// Zuletzt gezeigtes Fensterbild (vormultipliziert) und was die Pille
    /// „wird aktualisiert“ darin zeigt: Beim Ziehen im Modell wird nur die
    /// Pille neu gezeichnet, nicht das ganze Fenster (Review 1g).
    shown: Vec<u8>,
    /// Leinwand der Bilder, behält ihren Speicher ([`Canvas::reuse`]).
    canvas: Canvas,
    pill_shown: Option<(u8, u8)>,
    /// Hover oder Auswahl haben sich geändert: nur die Zeilen neu zeichnen,
    /// deren Band anders aussieht (Review 1h, U5). Dazu Bänder und Knopf,
    /// wie sie im gezeigten Bild stehen.
    bands_dirty: bool,
    bands_shown: Vec<RowBand>,
    button_shown: (bool, bool),
    /// Rollstand des gezeigten Bildes (px): Rollen verschiebt das Bild und
    /// zeichnet nur die frei werdenden Zeilen (U6b).
    scroll_shown: i32,
    /// Hinweis nach Entf unter der Zeile (H119).
    pub hint: Option<HintCard>,
    /// Kontextmenü an einer Zeile und die Bauteile der Zeile.
    pub context: Option<(ContextMenu, Vec<ElementId>)>,
    /// Voller Name einer gekürzten Zeile an der Maus (Fensterpixel), seit
    /// wann er gewünscht ist und ob er schon steht.
    tip: Option<(String, (f64, f64), Instant, bool)>,
}

impl QuantityWindow {
    pub fn new() -> QuantityWindow {
        let mut title = TitleBar::new(1.0);
        title.side = true;
        QuantityWindow {
            open: false,
            w: 0,
            h: 0,
            title,
            list: None,
            karten: Karten::new(Blatt::Mengen),
            kosten: None,
            ava: None,
            datei: String::new(),
            grouping: Grouping::Storey,
            blatt_wahl: (false, false),
            dirty: false,
            docked: true,
            seam_flash: None,
            was_busy: false,
            shown: Vec::new(),
            canvas: Canvas::new(0, 0),
            pill_shown: None,
            bands_dirty: false,
            bands_shown: Vec::new(),
            button_shown: (false, false),
            scroll_shown: 0,
            hint: None,
            context: None,
            tip: None,
        }
    }

    /// Ein- und Ausblenden des Hinweises (ms).
    fn fade_ms(t: &Theme) -> f32 {
        if t.size.anim_ms > 0.0 {
            t.size.fade_ms
        } else {
            0.0
        }
    }

    /// Hinweis unter der Zeile zeigen (leer: keiner).
    pub fn show_hint(
        &mut self,
        lines: Vec<String>,
        link: Option<(&'static str, Link)>,
        now: Instant,
    ) {
        self.hint = (!lines.is_empty()).then(|| HintCard::new(lines, link, Vec::new(), now));
        self.dirty = true;
    }

    /// Entdecken-Karte (Paket 5 §1.1) unten rechts im Blatt.
    pub fn discover(&mut self, lines: Vec<String>, link: (&'static str, Link), now: Instant) {
        self.hint = Some(HintCard::new(lines, Some(link), Vec::new(), now).discovering());
        self.dirty = true;
    }

    /// Nach dem Löschen aus der Liste (Entf oder Menü): Hinweis, Aufleuchten
    /// der abgelehnten Zeilen, bereinigte Auswahl.
    pub fn erased(&mut self, s: &Scene, d: &Deleted, p: &mut Picking, now: Instant) {
        let Some(l) = self.list.as_mut() else { return };
        let (lines, link) = l.erased(s, d, p, now);
        self.show_hint(lines, link, now);
    }

    /// Entf löscht hier etwas (sonst „Hier ist kein Bauteil gewählt.“).
    pub fn part_selected(&self, p: &Picking) -> bool {
        match self.blatt() {
            Blatt::Mengen => self.list.as_ref().is_some_and(|l| l.part_selected(p)),
            Blatt::Kosten | Blatt::Ava => !p.selected.is_empty(),
        }
    }

    /// Karte unter der Maus (Fensterpixel).
    fn karte_at(&self, t: &Theme, x: f64, y: f64) -> Option<Blatt> {
        let s = self.title.scale;
        Karten::hit(self.cards_x0(t), 32.0 * s, s, self.cards_breit(t), x, y)
    }

    /// Rechtsklick auf eine Zeile: wählt sie und öffnet das Menü. `false`
    /// auf Geschoss- und Summenzeilen.
    #[allow(clippy::too_many_arguments)]
    pub fn open_context(
        &mut self,
        s: &Scene,
        p: &mut Picking,
        x: f64,
        y: f64,
        t: &Theme,
        fonts: &Fonts,
    ) -> bool {
        let Some(l) = self.list.as_mut() else {
            return false;
        };
        let Some((target, ids)) = l.context_at(t, fonts, p, x, y) else {
            return false;
        };
        let menu = ContextMenu::for_list(
            s.model(),
            target,
            &p.selected,
            x,
            y,
            (self.w, self.h, self.title.height()),
            t,
            self.title.scale,
        );
        self.context = Some((menu, ids));
        self.hint = None;
        self.bands_dirty = true;
        self.dirty = true;
        true
    }

    /// Wann sich der Hinweis wieder ändert (für die Ereignisschleife).
    pub fn wait(&self, t: &Theme, now: Instant) -> Option<Duration> {
        let hint = self.hint.as_ref().map(|h| h.wait(now, Self::fade_ms(t)));
        let tip = self
            .tip
            .as_ref()
            .filter(|x| !x.3)
            .map(|x| crate::TIP_DELAY.saturating_sub(now.duration_since(x.2)));
        hint.into_iter().chain(tip).min()
    }

    /// Hinweis mit dem vollen Namen nachführen: neuer Name beginnt die
    /// Wartezeit, ohne Namen verschwindet er.
    fn set_tip(&mut self, want: Option<(String, (f64, f64))>) {
        let same = matches!((&self.tip, &want), (Some(a), Some(b)) if a.0 == b.0);
        if same {
            return;
        }
        if self.tip.take().is_some_and(|x| x.3) {
            self.dirty = true;
        }
        self.tip = want.map(|(text, at)| (text, at, Instant::now(), false));
    }

    /// Menü und Hinweis schließen.
    pub fn close_popups(&mut self) {
        if self.context.take().is_some() || self.hint.take().is_some() {
            self.dirty = true;
        }
        if self
            .kosten
            .as_mut()
            .is_some_and(|k| k.blaetter_schliessen())
        {
            self.dirty = true;
        }
    }

    pub fn caption_area(&self) -> CaptionArea {
        CaptionArea {
            height: self.title.height(),
            buttons_width: self.title.buttons_width(),
            left_width: 0,
        }
    }

    /// Andockzustand aus der Fensterschicht; beim Einrasten blinkt die Fuge.
    pub fn set_docked(&mut self, docked: bool, animate: bool) {
        if docked != self.docked {
            if docked && animate {
                self.seam_flash = Some(Instant::now());
            }
            self.docked = docked;
            self.dirty = true;
        }
    }

    /// Das gezeigte Blatt.
    pub fn blatt(&self) -> Blatt {
        self.karten.aktiv
    }

    /// Oberkante der Blätter unter Titelleiste und Karten (dip).
    fn sheet_top() -> f32 {
        32.0 + cards::HEIGHT
    }

    /// Linker Rand der Karten (px), wie der Inhalt der Blätter.
    fn cards_x0(&self, t: &Theme) -> f32 {
        t.size.sheet_pad * self.title.scale
    }

    /// Breite für die Karten (px): das Fenster ohne die Ränder.
    fn cards_breit(&self, t: &Theme) -> f32 {
        (self.w as f32 - 2.0 * self.cards_x0(t)).max(0.0)
    }

    /// An Modell und gemeinsamen Zustand angleichen (ohne Firmenkatalog).
    #[cfg(test)]
    pub fn sync(&mut self, s: &mut Scene, p: &Picking, animate: bool) {
        self.sync_mit(s, p, None, animate);
    }

    /// An Modell, Firmenkatalog und gemeinsamen Zustand angleichen. Beide
    /// Blätter laufen mit, denn beide Karten zeigen ihre Zahl; den Umfang
    /// gibt das gezeigte Blatt vor.
    pub fn sync_mit(
        &mut self,
        s: &mut Scene,
        p: &Picking,
        firma: Option<(&sk_model::Library, u64)>,
        animate: bool,
    ) {
        let g = self.grouping;
        let aktiv = self.karten.aktiv;
        let list = self.list.get_or_insert_with(|| ListView::grouped(s, g));
        let kosten = self.kosten.get_or_insert_with(KostenView::new);
        let ava = self.ava.get_or_insert_with(AvaView::new);
        ava.blatt_wahl = self.blatt_wahl;
        // Der Umfang gilt für alle Blätter; das gezeigte gibt ihn vor
        let umfang = match aktiv {
            Blatt::Mengen => list.leiste.umfang.clone(),
            Blatt::Kosten => kosten.leiste.umfang.clone(),
            Blatt::Ava => ava.leiste.umfang.clone(),
        };
        list.leiste.umfang = umfang.clone();
        kosten.leiste.umfang = umfang.clone();
        ava.leiste.umfang = umfang;
        list.top = Self::sheet_top();
        kosten.top = Self::sheet_top();
        ava.top = Self::sheet_top();
        if ava.datei != self.datei {
            ava.datei.clone_from(&self.datei);
        }
        list.scale = self.title.scale;
        kosten.scale = self.title.scale;
        ava.scale = self.title.scale;
        (list.w, list.h) = (self.w, self.h);
        (kosten.w, kosten.h) = (self.w, self.h);
        (ava.w, ava.h) = (self.w, self.h);
        if list.sync(s, animate && aktiv == Blatt::Mengen) && aktiv == Blatt::Mengen {
            self.dirty = true;
        }
        if kosten.sync(s, firma) && aktiv == Blatt::Kosten {
            self.dirty = true;
            // Die Netto-Vorschau an „übernehmen“ entsteht erst beim
            // Überfahren: den wartenden Tooltip nachziehen
            if kosten.auf_uebernehmen() {
                if let (Some(tip), Some(neu)) = (self.tip.as_mut(), kosten.tip_uebernehmen()) {
                    tip.0 = neu;
                }
            }
        }
        // Liegen die Zeilen danach anders (aufgeklappt, gerollt), zeichnet
        // `frame` doch das ganze Bild
        if list.follow(s, p) && aktiv == Blatt::Mengen {
            self.bands_dirty = true;
        }
        if kosten.follow(p) && aktiv == Blatt::Kosten {
            self.dirty = true;
        }
        // Das LV rechnet nur, solange das Blatt AVA gezeigt wird; die Karte
        // behält ihre letzte Zahl
        if aktiv == Blatt::Ava && ava.sync(s, firma) {
            self.dirty = true;
        }
        if ava.follow(p) && aktiv == Blatt::Ava {
            self.dirty = true;
        }
        // Vor dem ersten Öffnen ohne Zahl, aber nicht leer (Bedienbarkeit 14)
        let ava_zahl = if ava.karte.is_empty() {
            AVA_VORHER.to_string()
        } else {
            ava.karte.clone()
        };
        // Lebende Zahlen der Karten
        let now = Instant::now();
        let mengen = mengen_zahl(&s.schedule_in(&list.leiste.umfang));
        let netto = kosten.blatt().map_or_else(String::new, |b| {
            format!("{} netto", kosten_view::euro_ganz(b.netto))
        });
        let a = self.karten.set_zahl(Blatt::Mengen, mengen, now);
        let b = self.karten.set_zahl(Blatt::Kosten, netto, now);
        let c = self.karten.set_zahl(Blatt::Ava, ava_zahl, now);
        if a || b || c {
            self.dirty = true;
        }
    }

    /// Blatt wechseln; der Inhalt gleitet in `anim_ms`.
    pub fn waehlen(&mut self, b: Blatt, t: &Theme) -> bool {
        let anim = t.size.anim_ms > 0.0;
        if !self.karten.waehlen(b, Instant::now(), anim) {
            return false;
        }
        self.hint = None;
        self.context = None;
        self.tip = None;
        self.dirty = true;
        true
    }

    /// Animationen weiterführen; `true`, solange weitere Bilder nötig sind.
    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        let pill = t.size.anim_ms > 0.0
            && self
                .list
                .as_ref()
                .is_some_and(|l| l.pill_key(t, now).is_some());
        // Rollen braucht weitere Bilder, aber kein ganzes: `frame` verschiebt
        let scrolling = self.list.as_mut().is_some_and(|l| l.tick(t, now));
        let mut flashing = self.list.as_ref().is_some_and(|l| l.flashing());
        // Karten (Gleiten, Aufglimmen) und Chips im Reiter Kosten
        if self.karten.tick(t, now) {
            flashing = true;
        }
        if self.blatt() == Blatt::Kosten && self.kosten.as_mut().is_some_and(|k| k.tick(t, now)) {
            flashing = true;
        }
        if self.blatt() == Blatt::Ava && self.ava.as_mut().is_some_and(|a| a.tick(t, now)) {
            flashing = true;
        }
        if let Some(at) = self.seam_flash {
            if now.duration_since(at) < SEAM_FLASH && t.size.anim_ms > 0.0 {
                flashing = true;
            } else {
                self.seam_flash = None;
                self.dirty = true;
            }
        }
        if let Some(h) = &self.hint {
            match h.alpha(now, Self::fade_ms(t)) {
                None => {
                    self.hint = None;
                    self.dirty = true;
                }
                Some(a) if a < 1.0 => flashing = true,
                Some(_) => {}
            }
        }
        if let Some(tip) = self.tip.as_mut().filter(|x| !x.3) {
            if now.duration_since(tip.2) >= crate::TIP_DELAY {
                tip.3 = true;
                self.dirty = true;
            }
        }
        if flashing || self.was_busy {
            self.dirty = true;
        }
        self.was_busy = flashing;
        scrolling || flashing || pill
    }

    fn list_out(&mut self, o: Option<ListOut>, t: &Theme) -> Option<Out> {
        match o? {
            ListOut::WaehlenIm(el) => {
                self.waehlen(Blatt::Kosten, t);
                if let Some(k) = self.kosten.as_mut() {
                    k.waehlen_fuer(el);
                }
                self.dirty = true;
                None
            }
            ListOut::Repaint => {
                self.dirty = true;
                None
            }
            ListOut::Picking { selection } => {
                self.bands_dirty = true;
                Some(Out::Picking { selection })
            }
            ListOut::Zoom(v) => Some(Out::Zoom(v)),
            ListOut::SaveCsv => {
                self.dirty = true;
                Some(Out::SaveCsv)
            }
            ListOut::SavePdf => {
                self.dirty = true;
                Some(Out::SavePdf)
            }
            ListOut::BlattWahl(w) => {
                self.blatt_wahl = w;
                self.dirty = true;
                Some(Out::BlattWahl(w))
            }
            // Die Zeilen baut das nächste `sync` neu
            ListOut::Grouping(g) => {
                self.grouping = g;
                self.dirty = true;
                None
            }
            ListOut::Kosten(w) => {
                self.dirty = true;
                Some(Out::Kosten(w))
            }
            ListOut::Verwaltung(g) => Some(Out::Verwaltung(g)),
            ListOut::Kennwort => Some(Out::Kennwort),
            ListOut::Ablauf(g) => Some(Out::Ablauf(g)),
            ListOut::Projektdaten(i) => Some(Out::Projektdaten(i)),
        }
    }

    /// Ereignis des Mengenfensters.
    pub fn handle(&mut self, e: &Event, t: &Theme, fonts: &Fonts, p: &mut Picking) -> Option<Out> {
        match *e {
            Event::MouseMove { x, y, .. } if self.context.is_none() => {
                let want = match self.blatt() {
                    Blatt::Mengen => self.list.as_ref().and_then(|l| l.tip_at(t, fonts, x, y)),
                    Blatt::Kosten => self.kosten.as_ref().and_then(|k| k.tip_at(t, fonts, x, y)),
                    Blatt::Ava => self.ava.as_ref().and_then(|a| a.tip_at(t, fonts, x, y)),
                };
                self.set_tip(want.map(|w| (w, (x, y))));
            }
            Event::MouseMove { .. }
            | Event::MouseLeave
            | Event::MouseDown { .. }
            | Event::Wheel { .. }
            | Event::Key { .. }
            | Event::Focus(false) => self.set_tip(None),
            _ => {}
        }
        if let Some(o) = self.handle_popups(e, t) {
            return o;
        }
        match *e {
            Event::Resized { width, height } => {
                (self.w, self.h) = (width, height);
                if let Some(l) = self.list.as_mut() {
                    (l.w, l.h) = (width, height);
                }
                if let Some(k) = self.kosten.as_mut() {
                    (k.w, k.h) = (width, height);
                }
                if let Some(a) = self.ava.as_mut() {
                    (a.w, a.h) = (width, height);
                }
                self.dirty = true;
                None
            }
            Event::ScaleChanged(s) => {
                self.title.scale = s;
                if let Some(l) = self.list.as_mut() {
                    l.scale = s;
                }
                if let Some(k) = self.kosten.as_mut() {
                    k.scale = s;
                }
                if let Some(a) = self.ava.as_mut() {
                    a.scale = s;
                }
                self.dirty = true;
                None
            }
            Event::Maximized(m) => {
                self.title.maximized = m;
                self.dirty = true;
                None
            }
            Event::Focus(f) => {
                self.title.active = f;
                self.dirty = true;
                None
            }
            Event::Redraw => {
                self.dirty = true;
                None
            }
            Event::CloseRequested { .. } => Some(Out::Close),
            Event::MouseMove { x, y, .. } => {
                let b = self.title.button_at(x, y, self.w);
                if b != self.title.hover {
                    self.title.hover = b;
                    self.dirty = true;
                }
                let karte = self.karte_at(t, x, y);
                if self.karten.set_hover(karte) {
                    self.dirty = true;
                }
                let o = match self.blatt() {
                    Blatt::Mengen => self.list.as_mut()?.mouse_move(t, fonts, p, x, y),
                    Blatt::Kosten => self.kosten.as_mut()?.mouse_move(t, fonts, p, x, y),
                    Blatt::Ava => self.ava.as_mut()?.mouse_move(t, fonts, p, x, y),
                };
                self.list_out(o, t)
            }
            Event::MouseLeave => {
                if self.title.hover.take().is_some() {
                    self.dirty = true;
                }
                if self.karten.set_hover(None) {
                    self.dirty = true;
                }
                let o = match self.blatt() {
                    Blatt::Mengen => self.list.as_mut()?.mouse_leave(p),
                    Blatt::Kosten => self.kosten.as_mut()?.mouse_leave(p),
                    Blatt::Ava => self.ava.as_mut()?.mouse_leave(p),
                };
                self.list_out(o, t)
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                if let Some(b) = self.title.button_at(x, y, self.w) {
                    self.title.pressed = Some(b);
                    self.dirty = true;
                    return None;
                }
                if let Some(b) = self.karte_at(t, x, y) {
                    self.waehlen(b, t);
                    return None;
                }
                let o = match self.blatt() {
                    Blatt::Mengen => self.list.as_mut()?.mouse_down(t, fonts, p, (x, y), mods),
                    Blatt::Kosten => self.kosten.as_mut()?.mouse_down(t, fonts, p, (x, y), mods),
                    Blatt::Ava => self.ava.as_mut()?.mouse_down(t, fonts, p, (x, y), mods),
                };
                self.list_out(o, t)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                if let Some(pressed) = self.title.pressed.take() {
                    self.dirty = true;
                    if self.title.button_at(x, y, self.w) != Some(pressed) {
                        return None;
                    }
                    return Some(match pressed {
                        Button::Minimize => Out::Command(WindowCommand::Minimize),
                        Button::Maximize => Out::Command(WindowCommand::ToggleMaximize),
                        _ => Out::Close,
                    });
                }
                let o = match self.blatt() {
                    Blatt::Mengen => self.list.as_mut()?.mouse_up(t, fonts, x, y),
                    Blatt::Kosten => self.kosten.as_mut()?.mouse_up(t, fonts, x, y),
                    Blatt::Ava => self.ava.as_mut()?.mouse_up(t, fonts, x, y),
                };
                self.list_out(o, t)
            }
            Event::Wheel { delta, .. } => {
                // Kein ganzes Bild: `frame` verschiebt um den neuen Rollstand
                let anim = t.size.anim_ms > 0.0;
                match self.blatt() {
                    Blatt::Mengen => {
                        self.list.as_mut()?.wheel(delta, t, anim);
                        None
                    }
                    Blatt::Kosten => {
                        let o = self.kosten.as_mut()?.wheel(delta, t);
                        self.list_out(o, t)
                    }
                    Blatt::Ava => {
                        let o = self.ava.as_mut()?.wheel(delta, t);
                        self.list_out(o, t)
                    }
                }
            }
            // Das Menü an der Zeile gibt es im Mengenblatt (H119)
            Event::MouseDown {
                button: MouseButton::Right,
                x,
                y,
                ..
            } if y >= self.title.height() as f64 && self.blatt() == Blatt::Mengen => {
                Some(Out::OpenContext { x, y })
            }
            // Das Preisblatt nimmt Tasten und Zeichen zuerst
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } if self.blatt() == Blatt::Kosten
                && self.kosten.as_ref().is_some_and(|k| {
                    k.blatt_offen() || (key == Key::Escape && k.unterschiede_offen())
                }) =>
            {
                let o = self.kosten.as_mut()?.key(t, key, mods)?;
                self.list_out(o, t)
            }
            Event::Text(ch) if self.blatt() == Blatt::Kosten => {
                let o = self.kosten.as_mut()?.text(ch);
                self.list_out(o, t)
            }
            Event::Key {
                key: Key::Delete,
                down: true,
                ..
            } => Some(Out::Delete),
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                let had = !p.selected.is_empty();
                match self.blatt() {
                    Blatt::Mengen => self.list.as_mut()?.clear_selection(p),
                    Blatt::Kosten => {
                        p.selected.clear();
                        self.kosten.as_mut()?.follow(p);
                    }
                    // Zuerst das offene Baumblatt (schmales Fenster)
                    Blatt::Ava if self.ava.as_mut()?.baum_blatt_schliessen() => {
                        self.dirty = true;
                        return None;
                    }
                    Blatt::Ava => {
                        p.selected.clear();
                        let a = self.ava.as_mut()?;
                        a.schliessen();
                        a.follow(p);
                        self.dirty = true;
                    }
                }
                self.bands_dirty = true;
                had.then_some(Out::Picking { selection: true })
            }
            _ => None,
        }
    }

    /// Menü und Hinweis nehmen Ereignisse zuerst. `Some(out)`: verbraucht.
    fn handle_popups(&mut self, e: &Event, t: &Theme) -> Option<Option<Out>> {
        let s = self.title.scale;
        if let Some((c, ids)) = self.context.as_mut() {
            match *e {
                Event::MouseMove { x, y, .. } => {
                    if c.mouse_move(t, s, x, y) {
                        self.dirty = true;
                    }
                    return Some(None);
                }
                Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if !c.press(t, s, x, y) {
                        self.context = None;
                        self.dirty = true;
                    }
                    return Some(None);
                }
                Event::MouseUp {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    let a = c.release(t, s, x, y);
                    let out = a.map(|a| Out::Action(a, c.target, std::mem::take(ids)));
                    if out.is_some() {
                        self.context = None;
                        self.dirty = true;
                    }
                    return Some(out);
                }
                Event::MouseDown {
                    button: MouseButton::Right,
                    ..
                } => {
                    // Schließt und öffnet an der neuen Stelle
                    self.context = None;
                    self.dirty = true;
                }
                Event::Key {
                    key: Key::Delete,
                    down: true,
                    ..
                } => {
                    self.context = None;
                    self.dirty = true;
                    return Some(Some(Out::Delete));
                }
                Event::Key {
                    key, down: true, ..
                } => {
                    self.dirty = true;
                    return Some(match c.key(key) {
                        Err(()) => {
                            self.context = None;
                            None
                        }
                        Ok(None) => None,
                        Ok(Some(a)) => {
                            let out = Out::Action(a, c.target, std::mem::take(ids));
                            self.context = None;
                            Some(out)
                        }
                    });
                }
                Event::MouseLeave | Event::Focus(false) => {}
                _ => {}
            }
        }
        if let Some(h) = self.hint.as_mut() {
            match *e {
                Event::MouseMove { x, y, .. } => {
                    if h.mouse_move(x, y, s, t, Instant::now()) {
                        self.dirty = true;
                    }
                }
                Event::MouseDown {
                    button: MouseButton::Left,
                    x,
                    y,
                    ..
                } => match h.click(x, y, s, t) {
                    None => {}
                    Some(None) => return Some(None),
                    Some(Some(l)) => {
                        self.hint = None;
                        self.dirty = true;
                        return Some(Some(Out::Link(l)));
                    }
                },
                _ => {}
            }
        }
        None
    }

    /// Neues Fensterbild, falls nötig: das ganze Bild oder nur die Zeilen
    /// (von, bis), die sich geändert haben. Hover und Auswahl zeichnen die
    /// Zeilen mit anderem Band, die Pille ihre Zeilen, Rollen verschiebt die
    /// Liste und zeichnet die frei werdenden Zeilen und die Laufleiste.
    pub fn frame(&mut self, t: &Theme, fonts: &Fonts, now: Instant) -> Option<Frame<'_>> {
        if self.w == 0 || self.h == 0 {
            return None;
        }
        let (w, h) = (self.w as i32, self.h as i32);
        self.place_hint(t, fonts);
        // Reiter Kosten und Gleiten: jedes neue Bild ganz (die Teilbilder
        // kennt nur das Mengenblatt)
        if self.blatt() != Blatt::Mengen || self.karten.gleiten(t, now).is_some() {
            let bands = std::mem::take(&mut self.bands_dirty);
            let overlay = match self.blatt() {
                Blatt::Ava => self.ava.as_ref().is_some_and(|a| a.overlay_open()),
                _ => self.kosten.as_ref().is_some_and(|k| k.overlay_open()),
            };
            if !(self.dirty || bands || overlay || self.shown.len() != (w * h * 4) as usize) {
                return None;
            }
            let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
            self.paint_rows_into(&mut c, t, fonts, now, 0, self.h);
            c.premul_rgba8_into(&mut self.shown);
            self.canvas = c;
            self.dirty = false;
            // Zurück im Mengenblatt zeichnet das erste Bild ganz
            self.pill_shown = None;
            self.bands_shown.clear();
            return Some((&self.shown, None));
        }
        let key = self.list.as_ref().and_then(|l| l.pill_key(t, now));
        // Neu zu zeichnende Zeilenbereiche und was davon gezeigt werden muss
        let mut paint: Vec<(i32, i32)> = Vec::new();
        let mut shown: Option<(i32, i32)> = None;
        let mut show = |a: i32, b: i32| {
            shown = Some(shown.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
        };
        // Über der Liste liegt etwas (Gebäudefeld, wackelnder Chip): ganz
        let mut full = self.dirty
            || self.shown.len() != (w * h * 4) as usize
            || self.list.as_ref().is_some_and(|l| l.overlay_open());
        let scroll = self.list.as_ref().map_or(0, |l| l.scroll_px());
        let scrolled = scroll != self.scroll_shown;
        let bands_dirty = std::mem::take(&mut self.bands_dirty);
        if !full {
            if let Some(l) = &self.list {
                if scrolled && (bands_dirty || l.scrollbar_x(t).is_none()) {
                    full = true;
                } else if bands_dirty {
                    let bands = l.row_bands(t);
                    let same_rows = bands.len() == self.bands_shown.len()
                        && bands
                            .iter()
                            .zip(&self.bands_shown)
                            .all(|(a, b)| (a.0, a.1) == (b.0, b.1));
                    if !same_rows || l.button_look() != self.button_shown {
                        full = true;
                    } else {
                        for (a, b) in bands.iter().zip(&self.bands_shown) {
                            if a.2 != b.2 {
                                paint.push((a.0, a.1));
                                show(a.0, a.1);
                            }
                        }
                        self.bands_shown = bands;
                    }
                }
            }
        }
        if !full && scrolled {
            let l = self.list.as_ref()?;
            let (top, d) = (l.list_y().clamp(0, h), scroll - self.scroll_shown);
            if d.abs() >= h - top {
                full = true;
            } else {
                // Liste verschieben, frei werdende Zeilen neu zeichnen
                let row = (w * 4) as usize;
                let (from, to, fresh) = if d > 0 {
                    (top + d, top, (h - d, h))
                } else {
                    (top, top - d, (top, top - d))
                };
                let n = (h - top - d.abs()) as usize * row;
                let src = from as usize * row;
                self.shown.copy_within(src..src + n, to as usize * row);
                paint.push(fresh);
                show(top, h);
                // Laufleiste: eigene Spalte rechts, über die ganze Liste
                if let Some(x) = l.scrollbar_x(t) {
                    let cw = (w - x).max(1) as usize;
                    let mut c = Canvas::new(cw, (h - top) as usize);
                    c.clear(t.ui.sheet_bg);
                    c.set_origin(x as f32, top as f32);
                    l.paint_scrollbar(&mut c, t);
                    let px = c.to_premul_rgba8();
                    for (k, line) in px.chunks_exact(cw * 4).enumerate() {
                        let at = (top as usize + k) * row + x as usize * 4;
                        self.shown[at..at + cw * 4].copy_from_slice(line);
                    }
                }
                self.scroll_shown = scroll;
                self.bands_shown = l.row_bands(t);
            }
        }
        if full {
            let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
            self.paint_rows_into(&mut c, t, fonts, now, 0, self.h);
            c.premul_rgba8_into(&mut self.shown);
            self.canvas = c;
            self.dirty = false;
            self.pill_shown = key;
            self.scroll_shown = scroll;
            if let Some(l) = &self.list {
                self.bands_shown = l.row_bands(t);
                self.button_shown = l.button_look();
            }
            return Some((&self.shown, None));
        }
        if key != self.pill_shown {
            self.pill_shown = key;
            if let Some((_, y, _, ph)) = self.list.as_ref().and_then(|l| l.pill_rect(t, fonts)) {
                paint.push((y, y + ph));
                show(y, y + ph);
            }
        }
        // Nur diese Zeilen zeichnen: wie im ganzen Bild (Schrift höchstens
        // eine Stufe anders gerundet)
        for (y0, y1) in paint {
            let (y0, y1) = (y0.clamp(0, h), y1.clamp(0, h));
            if y1 <= y0 {
                continue;
            }
            let mut c = std::mem::replace(&mut self.canvas, Canvas::new(0, 0));
            self.paint_rows_into(&mut c, t, fonts, now, y0 as u32, y1 as u32);
            let band = c.to_premul_rgba8();
            self.canvas = c;
            let at = (y0 * w * 4) as usize;
            self.shown[at..at + band.len()].copy_from_slice(&band);
        }
        let (y0, y1) = shown?;
        let (y0, y1) = (y0.clamp(0, h), y1.clamp(0, h));
        (y1 > y0).then_some((&self.shown[..], Some((y0 as u32, y1 as u32))))
    }

    /// Ein Blatt, um `dx` px seitlich verschoben.
    #[allow(clippy::too_many_arguments)]
    fn paint_blatt(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        now: Instant,
        b: Blatt,
        dx: f32,
        y0: u32,
    ) {
        c.set_origin(-dx, y0 as f32);
        match b {
            Blatt::Mengen => {
                if let Some(l) = &self.list {
                    l.paint(c, t, fonts, now);
                }
            }
            Blatt::Kosten => {
                if let Some(k) = &self.kosten {
                    k.paint(c, t, fonts, now);
                }
            }
            Blatt::Ava => {
                if let Some(a) = &self.ava {
                    a.paint(c, t, fonts, now);
                }
            }
        }
        c.set_origin(0.0, y0 as f32);
    }

    /// Hinweis unter seine Zeile legen (folgt dem Rollen).
    fn place_hint(&mut self, t: &Theme, fonts: &Fonts) {
        let (Some(h), Some(l)) = (self.hint.as_mut(), self.list.as_ref()) else {
            return;
        };
        let s = self.title.scale;
        let size = h.size(t, fonts, s);
        let before = h.rect;
        let top = l.list_y() as f32;
        h.place(size, l.hint_rect(t), (self.w as f32, self.h as f32, top), s);
        if h.rect != before {
            self.dirty = true;
        }
    }

    /// Ganzes Fensterbild: Blatt, Fuge zum Hauptfenster, Titelleiste.
    #[cfg(test)]
    pub fn paint(&self, t: &Theme, fonts: &Fonts, now: Instant) -> Canvas {
        self.paint_rows(t, fonts, now, 0, self.h)
    }

    /// Ausschnitt des Fensterbilds, Zeilen `y0..y1`.
    #[cfg(test)]
    fn paint_rows(&self, t: &Theme, fonts: &Fonts, now: Instant, y0: u32, y1: u32) -> Canvas {
        let mut c = Canvas::new(0, 0);
        self.paint_rows_into(&mut c, t, fonts, now, y0, y1);
        c
    }

    /// Wie [`QuantityWindow::paint_rows`] auf eine vorhandene Leinwand.
    fn paint_rows_into(
        &self,
        c: &mut Canvas,
        t: &Theme,
        fonts: &Fonts,
        now: Instant,
        y0: u32,
        y1: u32,
    ) {
        let (w, h) = (self.w as usize, y1.saturating_sub(y0) as usize);
        c.reuse(w, h);
        c.clear(t.ui.sheet_bg);
        c.set_origin(0.0, y0 as f32);
        let s = self.title.scale;
        // Blatt, beim Wechsel beide gleitend; die Karten bleiben stehen
        let w = self.w as f32;
        match self.karten.gleiten(t, now) {
            Some((vorher, k)) => {
                let dir = Karten::richtung(vorher, self.blatt());
                self.paint_blatt(c, t, fonts, now, vorher, -dir * k * w, y0);
                self.paint_blatt(c, t, fonts, now, self.blatt(), dir * (1.0 - k) * w, y0);
                c.set_origin(0.0, y0 as f32);
            }
            None => self.paint_blatt(c, t, fonts, now, self.blatt(), 0.0, y0),
        }
        // Die Kartenzeile deckt das gleitende Blatt nur unter den Karten zu;
        // rechts davon zeichnet das Blatt seinen Knopf (Notiz Kopf §2)
        let ende = Karten::ende(self.cards_x0(t), s, self.cards_breit(t)) + 8.0 * s;
        c.fill_rect(0.0, 32.0 * s, ende, cards::HEIGHT * s, t.ui.sheet_bg);
        self.karten.paint(
            c,
            t,
            fonts,
            (self.cards_x0(t), 32.0 * s, s, self.cards_breit(t)),
            now,
        );
        if let Some(hc) = &self.hint {
            if let (Some(r), Some(a)) = (hc.rect, hc.alpha(now, Self::fade_ms(t))) {
                let img = hc.paint(t, fonts, s);
                let m = (t.size.panel_shadow * s).round();
                c.blit_scaled(&img, r.x - m, r.y - m, 1.0, a);
            }
        }
        if let Some((menu, _)) = &self.context {
            let (img, x, y) = menu.paint(t, fonts, s);
            c.blit(&img, x, y);
        }
        // Voller Name rechts unter der Maus (wie im Hauptfenster)
        if let Some((text, (mx, my), _, true)) = &self.tip {
            let img = sk_ui::widgets::tooltip(fonts, text, s, t);
            let (iw, ih) = (img.width as f64, img.height as f64);
            let x = (mx + 12.0 * s as f64).min(self.w as f64 - iw).max(0.0);
            let mut y = my + 20.0 * s as f64;
            if y + ih > self.h as f64 {
                y = my - 8.0 * s as f64 - ih;
            }
            c.blit(&img, x.round() as i32, y.round() as i32);
        }
        // Titelleiste nur, wenn der Ausschnitt sie berührt
        if y0 < self.title.height() {
            c.set_origin(0.0, 0.0);
            let bar = self.title.paint(t, fonts.regular.as_ref(), self.w);
            c.blit(&bar, 0, -(y0 as i32));
            c.set_origin(0.0, y0 as f32);
        }
        if self.docked {
            let s = self.title.scale;
            let col = match self.seam_flash {
                Some(at) if now.duration_since(at) < SEAM_FLASH => t.ui.accent,
                _ => t.title.bg,
            };
            c.fill_rect(0.0, 0.0, (SEAM * s).max(1.0), self.h as f32, col);
        }
    }
}

/// Zweite Zeile der Karte AVA, bevor das LV einmal gerechnet ist.
const AVA_VORHER: &str = "Leistungsverzeichnis je Los";

/// Lebende Zahl der Karte Mengen: „14 Bauteile · 3 Geschosse“ im Umfang
/// (Geschosse mit Bauteilen, das Gründungsband zählt mit).
fn mengen_zahl(sched: &Schedule) -> String {
    let mut bauteile: Vec<sk_model::ElementId> = Vec::new();
    let mut geschosse = 0;
    let storeys = sched
        .buildings
        .iter()
        .flat_map(|b| &b.storeys)
        .chain(&sched.loose);
    for st in storeys {
        let mut belegt = false;
        for r in st.groups.iter().flat_map(|g| &g.rows) {
            belegt = true;
            if !bauteile.contains(&r.element) {
                bauteile.push(r.element);
            }
        }
        geschosse += usize::from(belegt);
    }
    let n = bauteile.len();
    format!(
        "{n} {} · {geschosse} {}",
        if n == 1 { "Bauteil" } else { "Bauteile" },
        if geschosse == 1 {
            "Geschoss"
        } else {
            "Geschosse"
        }
    )
}

#[cfg(test)]
#[path = "quantity_istbilder.rs"]
mod istbilder;

#[cfg(test)]
#[path = "quantity_muster_pdf.rs"]
mod muster_pdf;

#[cfg(test)]
mod tests {
    use super::*;
    use sk_math::vec3;
    use sk_model::{Model, RefSide, WallChain};

    /// Gleich bis auf eine Stufe (von 255): verschobene Schriftpfade runden
    /// in Gleitkomma minimal anders, unsichtbar.
    fn fast_gleich(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 1)
    }

    /// Review 1g (PR #18 auf B7 übertragen): Ziehen ändert die Revision je
    /// Bild, die Liste wartet aber auf das Loslassen. Nach dem ersten Bild
    /// mit der Pille „wird aktualisiert“ entsteht kein ganzes Fensterbild
    /// mehr, höchstens die Zeilen der Pille. Loslassen zeichnet neu.
    #[test]
    fn ziehen_zeichnet_die_liste_nicht_neu() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        s.add_wall(&WallChain {
            base: 0.0,
            points: vec![vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)],
            closed: false,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 3500.0,
            joints: Default::default(),
        })
        .unwrap();
        let p = Picking::default();
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 1000);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.tick(&t, now);
        assert!(
            matches!(q.frame(&t, &fonts, now), Some((_, None))),
            "erstes Bild ganz"
        );
        assert!(q.frame(&t, &fonts, now).is_none(), "nichts geändert");

        s.begin("Geschoss ziehen");
        let mut full = 0;
        for (i, top) in [2800.0, 2900.0, 2850.0, 2950.0].into_iter().enumerate() {
            let rev = s.model().revision();
            s.drag_storey_top(eg, top);
            assert_ne!(s.model().revision(), rev, "Ziehen ändert die Revision");
            let at = now + Duration::from_millis(100 * i as u64);
            q.sync(&mut s, &p, true);
            q.tick(&t, at);
            if let Some((_, None)) = q.frame(&t, &fonts, at) {
                full += 1;
            }
        }
        assert_eq!(
            full, 1,
            "nur das Bild, in dem die Pille erscheint, ist ganz"
        );
        s.commit();
        q.sync(&mut s, &p, true);
        q.tick(&t, now + Duration::from_millis(500));
        assert!(
            matches!(q.frame(&t, &fonts, now), Some((_, None))),
            "Loslassen: neue Mengen, ganzes Bild"
        );
    }

    /// Review 1h (U5): Hover aus dem Hauptfenster zeichnet nur die Zeilen,
    /// deren Band sich ändert, und das Ergebnis gleicht dem ganzen Bild.
    #[test]
    fn hover_zeichnet_nur_die_betroffenen_zeilen() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let run = s
            .add_wall(&WallChain {
                base: 0.0,
                points: vec![vec3(0.0, 0.0, 0.0), vec3(5000.0, 0.0, 0.0)],
                closed: false,
                ref_side: RefSide::Left,
                layers: Vec::new(),
                height: 3500.0,
                joints: Default::default(),
            })
            .unwrap();
        let wall = s.model().wall_at(run, 0).unwrap();
        let mut p = Picking::default();
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 1000);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));

        for (one, what) in [(Some(wall), "Hover an"), (None, "Hover aus")] {
            assert!(p.set_hover(one, Vec::new()));
            q.sync(&mut s, &p, false);
            let (_, rows) = q.frame(&t, &fonts, now).expect(what);
            let (y0, y1) = rows.expect("nur Zeilen, nicht das ganze Bild");
            assert!(y1 > y0 && y1 - y0 < 60, "{what}: eine Zeile, {y0}..{y1}");
            let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
            assert!(
                fast_gleich(&q.shown, &whole),
                "{what}: gleich dem ganzen Bild"
            );
            assert!(
                q.frame(&t, &fonts, now).is_none(),
                "{what}: danach nichts mehr"
            );
        }

        // Auswahl: ebenfalls nur Zeilen; auf- oder zugeklappt wird nichts
        p.selected = vec![wall];
        q.sync(&mut s, &p, false);
        let f = q.frame(&t, &fonts, now).map(|(_, r)| r);
        assert!(f.is_some());
        let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
        assert!(
            fast_gleich(&q.shown, &whole),
            "Auswahl: gleich dem ganzen Bild"
        );
    }

    /// Review 1i: Ein Streifen lässt Zeilen, Kopf und Titelleiste außerhalb
    /// weg und gleicht trotzdem dem ganzen Bild, auch bei 150 % und mit
    /// Hover und Auswahl (mit Schrift nur unter Windows).
    #[test]
    fn jeder_streifen_gleicht_dem_ganzen_bild() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let mut walls = Vec::new();
        for y in [0.0, 3000.0, 6000.0] {
            let run = s
                .add_wall(&WallChain {
                    base: 0.0,
                    points: vec![vec3(0.0, y, 0.0), vec3(5000.0, y, 0.0)],
                    closed: false,
                    ref_side: RefSide::Left,
                    layers: Vec::new(),
                    height: 3500.0,
                    joints: Default::default(),
                })
                .unwrap();
            walls.push(s.model().wall_at(run, 0).unwrap());
        }
        let mut p = Picking {
            selected: vec![walls[0]],
            ..Default::default()
        };
        p.set_hover(Some(walls[2]), Vec::new());
        let mut q = QuantityWindow::new();
        q.title.scale = 1.5;
        (q.w, q.h) = (780, 600);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
        let row = q.w as usize * 4;
        for y0 in (0..q.h - 9).step_by(13) {
            let y1 = (y0 + 9 + y0 % 31).min(q.h);
            let part = q.paint_rows(&t, &fonts, now, y0, y1).to_premul_rgba8();
            // Verschobene Pfade runden in Gleitkomma minimal anders: höchstens
            // eine Stufe (von 255) Unterschied, unsichtbar
            let w = &whole[y0 as usize * row..y1 as usize * row];
            let worst = part.iter().zip(w).map(|(a, b)| a.abs_diff(*b)).max();
            assert!(
                part.len() == w.len() && worst <= Some(1),
                "Streifen {y0}..{y1} weicht ab: {worst:?}"
            );
        }
    }

    /// U6b: Rollen verschiebt das gezeigte Bild und zeichnet nur die frei
    /// werdenden Zeilen und die Laufleiste; das Ergebnis gleicht dem ganzen
    /// Bild, nach unten wie nach oben, auch bei 150 %.
    #[test]
    fn rollen_verschiebt_das_bild() {
        let fonts = Fonts::system();
        let mut t = Theme::dark();
        t.size.anim_ms = 0.0;
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        for (k, y) in [0.0, 3000.0, 6000.0, 9000.0].into_iter().enumerate() {
            let cat = if k % 2 == 0 {
                sk_model::Category::ExteriorWall
            } else {
                sk_model::Category::InteriorWall
            };
            s.add_wall_as(
                &WallChain {
                    base: 0.0,
                    points: vec![vec3(0.0, y, 0.0), vec3(5000.0, y, 0.0)],
                    closed: false,
                    ref_side: RefSide::Left,
                    layers: Vec::new(),
                    height: 3500.0,
                    joints: Default::default(),
                },
                cat,
            )
            .unwrap();
        }
        for scale in [1.0f32, 1.5] {
            let p = Picking::default();
            let mut q = QuantityWindow::new();
            q.title.scale = scale;
            (q.w, q.h) = ((520.0 * scale) as u32, (330.0 * scale) as u32);
            let now = Instant::now();
            q.sync(&mut s, &p, false);
            assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));
            let mut p = p;
            let mut shifted = 0;
            // Bruchteile einer Raste: der Rollstand liegt auch zwischen Pixeln
            for delta in [-0.25, -0.13, 0.2, -0.4, 0.31, -0.07] {
                let before = q.list.as_ref().unwrap().scroll_px();
                q.handle(
                    &Event::Wheel {
                        delta,
                        x: 100.0,
                        y: 200.0,
                        mods: Default::default(),
                    },
                    &t,
                    &fonts,
                    &mut p,
                );
                q.tick(&t, now);
                let after = q.list.as_ref().unwrap().scroll_px();
                let f = q.frame(&t, &fonts, now).map(|(_, r)| r);
                if after == before {
                    assert_eq!(f, None, "nichts gerollt");
                    continue;
                }
                let list_y = q.list.as_ref().unwrap().list_y() as u32;
                assert_eq!(f, Some(Some((list_y, q.h))), "{scale}: nur die Liste");
                shifted += 1;
                let whole = q.paint(&t, &fonts, now).to_premul_rgba8();
                assert!(
                    fast_gleich(&q.shown, &whole),
                    "{scale}: Rollen {before} → {after} gleicht dem ganzen Bild"
                );
            }
            assert!(shifted >= 4, "{scale}: {shifted} Mal verschoben");
        }
    }

    /// Standardhaus RH-1 (Fundament, EG, OG) als Szene.
    fn standardhaus() -> Scene {
        let m = sk_model::szo::read_with(
            include_str!("../../crates/sk-cost/referenz/rh1-standardhaus.szo"),
            sk_model::GuidGen::with_seed(1),
            &sk_cost::lesen::ABSCHNITTE_SZO,
        )
        .expect("lädt")
        .model;
        Scene::with_model(m)
    }

    fn klick(x: f64, y: f64) -> Event {
        Event::MouseDown {
            button: MouseButton::Left,
            x,
            y,
            mods: Default::default(),
        }
    }

    /// KA-2a (paket-ka2 Abnahme 9): zwei Karten mit lebender Zahl; ein
    /// Klick wechselt das Blatt, der Inhalt gleitet, der Titel folgt; der
    /// Umfang gilt für beide Blätter; eine geänderte Zahl glimmt auf.
    #[test]
    fn karten_wechseln_das_blatt() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let mut s = standardhaus();
        let mut p = Picking::default();
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (780, 900);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        assert_eq!(q.blatt(), Blatt::Mengen);
        assert!(
            q.karten
                .zahl(Blatt::Mengen)
                .ends_with(" Bauteile · 3 Geschosse"),
            "{}",
            q.karten.zahl(Blatt::Mengen)
        );
        assert_eq!(q.karten.zahl(Blatt::Kosten), "60.090 € netto");
        assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));
        // Klick auf die Karte Kosten (rechts neben Mengen)
        let s1 = q.title.scale as f64;
        let x = q.cards_x0(&t) as f64 + (236.0 + 12.0 + 100.0) * s1;
        let y = (32.0 + 14.0 + 37.0) * s1;
        assert_eq!(q.handle(&klick(x, y), &t, &fonts, &mut p), None);
        assert_eq!(q.blatt(), Blatt::Kosten);
        let doc = crate::document::Document::new(s.model().revision());
        let titel = crate::windows::blatt_caption(q.blatt(), &doc, s.model().revision());
        assert!(titel.starts_with("Kosten – "), "{titel}");
        let mitten = now + Duration::from_millis(t.size.anim_ms as u64 / 2);
        assert!(q.karten.gleiten(&t, Instant::now()).is_some(), "gleitet");
        assert!(q.tick(&t, mitten));
        assert!(matches!(q.frame(&t, &fonts, mitten), Some((_, None))));
        // Chip EG im Reiter Kosten: Netto und Mengenkarte folgen
        let k = q.kosten.as_mut().unwrap();
        let chips = k.leiste.chips().to_vec();
        assert!(crate::umfang_view::klick(
            &mut k.leiste.umfang,
            &chips,
            1,
            true
        ));
        q.sync(&mut s, &p, true);
        let eg = q.kosten.as_ref().unwrap().blatt().unwrap().netto;
        assert!(eg.0 < 6_008_983);
        assert_eq!(
            q.karten.zahl(Blatt::Kosten),
            format!("{} netto", kosten_view::euro_ganz(eg))
        );
        assert!(q.karten.zahl(Blatt::Mengen).ends_with(" · 1 Geschoss"));
        assert_eq!(
            q.list.as_ref().unwrap().leiste.umfang,
            q.kosten.as_ref().unwrap().leiste.umfang
        );
        // Zurück: Mengenblatt mit demselben Umfang, Titel wie B7
        let x0 = q.cards_x0(&t) as f64 + 100.0 * s1;
        q.handle(&klick(x0, y), &t, &fonts, &mut p);
        assert_eq!(q.blatt(), Blatt::Mengen);
        let titel = crate::windows::blatt_caption(q.blatt(), &doc, s.model().revision());
        assert!(titel.starts_with("Mengenermittlung – "), "{titel}");
    }

    /// Gebäude mit einer Außenwand und drei Innenwänden im EG.
    fn haus_h119() -> (Scene, Vec<ElementId>) {
        let mut s = Scene::with_model(Model::with_seed(1));
        s.edit_model("Gebäude erstellt", |m| {
            m.add_building(2);
            true
        });
        let b = s.model().buildings().ids().last().unwrap();
        let eg = s.model().ground_of(Some(b)).unwrap();
        s.set_active_storey(eg);
        let mut walls = Vec::new();
        for (k, y) in [0.0, 3000.0, 6000.0, 9000.0].into_iter().enumerate() {
            // Zuerst der geschlossene Umriss (Außenwände: abgelehnt)
            let (cat, points, closed) = if k == 0 {
                let r = [
                    (0.0, 0.0),
                    (10000.0, 0.0),
                    (10000.0, 12000.0),
                    (0.0, 12000.0),
                ];
                let pts = r.iter().map(|&(x, y)| vec3(x, y, 0.0)).collect();
                (sk_model::Category::ExteriorWall, pts, true)
            } else {
                let pts = vec![vec3(0.0, y, 0.0), vec3(10000.0, y, 0.0)];
                (sk_model::Category::InteriorWall, pts, false)
            };
            let run = s
                .add_wall_as(
                    &WallChain {
                        base: 0.0,
                        points,
                        closed,
                        ref_side: RefSide::Left,
                        layers: Vec::new(),
                        height: 3500.0,
                        joints: Default::default(),
                    },
                    cat,
                )
                .unwrap();
            walls.push(s.model().wall_at(run, 0).unwrap());
        }
        (s, walls)
    }

    fn taste(k: Key) -> Event {
        Event::Key {
            key: k,
            down: true,
            repeat: false,
            mods: Default::default(),
        }
    }

    /// Band der gewählten Zeile (Fensterpixel von, bis).
    fn gewaehlte_zeile(q: &QuantityWindow, t: &Theme) -> (i32, i32) {
        let bands = q.list.as_ref().unwrap().row_bands(t);
        let b = bands
            .iter()
            .find(|b| b.2 == Some((t.ui.sheet_select, true)))
            .expect("gewählte Zeile sichtbar");
        (b.0, b.1)
    }

    /// H119: Entf im Mengenfenster meldet sich bei der App; gelöschte Zeilen
    /// blenden aus, die übrigen rücken nach, danach ist Ruhe. Ohne Hinweis,
    /// wenn alles gelöscht wurde.
    #[test]
    fn entf_blendet_aus_und_rueckt_nach() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[1]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.frame(&t, &fonts, now);
        assert_eq!(
            q.handle(&taste(Key::Delete), &t, &fonts, &mut p),
            Some(Out::Delete)
        );
        assert!(q.part_selected(&p));
        let d = s.delete_elements(&p.selected.clone());
        assert_eq!(d.removed.len(), 1);
        q.erased(&s, &d, &mut p, now);
        assert!(p.selected.is_empty(), "Auswahl bereinigt");
        assert!(q.hint.is_none(), "alles gelöscht: kein Hinweis");
        q.sync(&mut s, &p, true);
        // Das Nachrücken beginnt beim Angleichen (Uhrzeit); ab hier zählen,
        // damit ein langsamer Lauf (Debug, volle Maschine) nicht kippt
        let now = Instant::now();
        let l = q.list.as_ref().unwrap();
        assert!(l.flashing(), "Zeilen rücken nach");
        let fade = Duration::from_millis(t.size.fade_ms as u64);
        let anim = Duration::from_millis(t.size.anim_ms as u64);
        assert!(q.tick(&t, now + fade / 2), "blendet aus");
        let half = q.paint(&t, &fonts, now + fade / 2).to_premul_rgba8();
        let moved = q.paint(&t, &fonts, now + fade + anim / 2).to_premul_rgba8();
        assert_ne!(half, moved, "erst ausblenden, dann nachrücken");
        let end = now + fade + anim + Duration::from_millis(t.size.flash_ms as u64 + 50);
        q.tick(&t, end);
        assert!(!q.tick(&t, end), "danach Ruhe");
        assert!(!q.list.as_ref().unwrap().flashing());

        // Ohne Übergänge: sofort fertig
        let mut t0 = Theme::dark();
        t0.size.anim_ms = 0.0;
        p.selected = vec![walls[2]];
        q.sync(&mut s, &p, false);
        let d = s.delete_elements(&p.selected.clone());
        q.erased(&s, &d, &mut p, now);
        q.sync(&mut s, &p, true);
        assert!(!q.tick(&t0, now), "anim_ms = 0: nichts läuft");
    }

    /// H119: Abgelehnt (Außenwand) leuchtet die Zeile, der Hinweis steht
    /// darunter. Nach einem Klick auf die Geschosszeile löscht Entf nichts.
    #[test]
    fn ablehnung_hinweis_unter_der_zeile() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[0]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, true);
        q.frame(&t, &fonts, now);
        let (_, row_bottom) = gewaehlte_zeile(&q, &t);
        let rev = s.model().revision();
        let d = s.delete_elements(&p.selected.clone());
        assert!(d.removed.is_empty());
        q.erased(&s, &d, &mut p, now);
        assert_eq!(s.model().revision(), rev);
        let h = q.hint.as_ref().expect("Hinweis");
        assert_eq!(h.lines[0], "Außenwände gehören zum Gebäudeumriss.");
        assert!(q.list.as_ref().unwrap().flashing(), "Zeile leuchtet");
        q.sync(&mut s, &p, true);
        q.tick(&t, now + Duration::from_millis(10));
        q.frame(&t, &fonts, now + Duration::from_millis(10));
        let r = q.hint.as_ref().unwrap().rect.expect("gelegt");
        assert!(
            r.y >= row_bottom as f32,
            "unter der Zeile: {} < {row_bottom}",
            r.y
        );
        assert!(q.wait(&t, now).is_some());

        // Klick auf die Geschosszeile: kein Bauteil
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let (y0, y1, _) = bands[0];
        let down = Event::MouseDown {
            button: MouseButton::Left,
            x: 80.0,
            y: (y0 + y1) as f64 * 0.5,
            mods: Default::default(),
        };
        q.handle(&down, &t, &fonts, &mut p);
        assert!(!q.part_selected(&p), "Geschosszeile angeklickt");
        p.selected = vec![walls[1]];
        q.list.as_mut().unwrap().follow(&mut s, &p);
        assert!(q.part_selected(&p), "Auswahl von außen: wieder ein Bauteil");
        // Summe nach Baustoff (letzte Zeile): ebenfalls kein Bauteil
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let (y0, y1, _) = *bands.last().unwrap();
        let sum = Event::MouseDown {
            button: MouseButton::Left,
            x: 80.0,
            y: (y0 + y1) as f64 * 0.5,
            mods: Default::default(),
        };
        q.handle(&sum, &t, &fonts, &mut p);
        assert!(!q.part_selected(&p), "Summenzeile angeklickt");
    }

    /// H119: Rechtsklick auf eine Zeile öffnet das Menü des Modells mit „Im
    /// Modell zeigen“ oben und ohne „Eigenschaften“; Enter führt aus.
    #[test]
    fn menue_an_der_zeile() {
        let (t, fonts) = (Theme::dark(), Fonts::system());
        let (mut s, walls) = haus_h119();
        let mut p = Picking {
            selected: vec![walls[1]],
            ..Default::default()
        };
        let mut q = QuantityWindow::new();
        (q.w, q.h) = (520, 800);
        let now = Instant::now();
        q.sync(&mut s, &p, false);
        q.frame(&t, &fonts, now);
        let (y0, y1) = gewaehlte_zeile(&q, &t);
        let (x, y) = (120.0, (y0 + y1) as f64 * 0.5);
        let right = Event::MouseDown {
            button: MouseButton::Right,
            x,
            y,
            mods: Default::default(),
        };
        assert_eq!(
            q.handle(&right, &t, &fonts, &mut p),
            Some(Out::OpenContext { x, y })
        );
        assert!(q.open_context(&s, &mut p, x, y, &t, &fonts));
        let labels: Vec<String> = q
            .context
            .as_ref()
            .unwrap()
            .0
            .actions()
            .into_iter()
            .map(|a| a.0)
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(labels[0], "Im Modell zeigen");
        assert!(labels.contains(&"Löschen".to_string()));
        assert!(!labels.contains(&"Eigenschaften".to_string()));
        assert!(matches!(q.frame(&t, &fonts, now), Some((_, None))));
        q.handle(&taste(Key::Other(0x28)), &t, &fonts, &mut p);
        let out = q.handle(&taste(Key::Enter), &t, &fonts, &mut p);
        assert_eq!(
            out,
            Some(Out::Action(Action::ShowInModel, walls[1], vec![walls[1]]))
        );
        assert!(q.context.is_none());
        // Auf der Geschosszeile kein Menü
        let bands = q.list.as_ref().unwrap().row_bands(&t);
        let gy = (bands[0].0 + bands[0].1) as f64 * 0.5;
        assert!(!q.open_context(&s, &mut p, 80.0, gy, &t, &fonts));
    }
}

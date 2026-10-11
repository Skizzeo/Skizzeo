//! Geschossfenster links neben dem Hauptfenster (Jörn 11.10.): dieselben
//! Regeln wie für das Mengenfenster rechts, gespiegelt. Angedockt hängt es
//! mit seiner rechten Kante an der linken des Hauptfensters und wandert mit;
//! weggezogen löst es sich und kann auf einen Bildschirm links daneben, nahe
//! an der Kante rastet es wieder ein.

use super::*;

/// Breite des Geschossfensters beim ersten Öffnen (dip): so breit, wie der
/// Geschossmanager braucht.
pub const LEVELS_WIDTH_DIP: f32 = 460.0;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Links {
    width_dip: f32,
    /// Offen, auch über das Programmende hinaus gemerkt.
    pub(super) open: bool,
    /// Das Fenster gibt es gerade.
    pub(super) shown: bool,
    docked: bool,
    rect: Option<Rect>,
    /// So viel ist das Hauptfenster beim Öffnen nach rechts ausgewichen:
    /// beim Schließen bekommt es den Platz an seiner linken Kante zurück,
    /// auch wenn es inzwischen woanders steht (Review #70).
    give_back: Option<i32>,
    /// Das Hauptfenster war maximiert und wurde dafür verkleinert.
    pub(super) restore_max: bool,
}

impl Links {
    pub(super) fn new() -> Links {
        Links {
            width_dip: LEVELS_WIDTH_DIP,
            open: false,
            shown: false,
            docked: true,
            rect: None,
            give_back: None,
            restore_max: false,
        }
    }
}

/// Platz des angedockten Geschossfensters: links bündig am Hauptfenster,
/// gleiche Oberkante und Höhe.
fn links_at(main: Rect, width: i32) -> Rect {
    (main.0 - width, main.1, width, main.3)
}

impl Windows {
    pub fn levels_open(&self) -> bool {
        self.links.open
    }

    pub fn levels_docked(&self) -> bool {
        self.links.docked
    }

    /// Gemerkte Lage des Geschossfensters; `None`, solange es nie offen war.
    pub fn levels_rect(&self) -> Option<Rect> {
        self.links.rect
    }

    /// Gemerkter Zustand des Geschossfensters (einstellungen.txt).
    pub fn remember_levels(&mut self, open: bool, docked: bool, rect: Option<Rect>) {
        (self.links.open, self.links.docked, self.links.rect) = (open, docked, rect);
    }

    /// Breite beim ersten Andocken (dip).
    pub fn set_levels_width_dip(&mut self, w: f32) {
        if w > 0.0 {
            self.links.width_dip = w;
        }
    }

    fn levels_width(&self) -> i32 {
        self.links
            .rect
            .map_or_else(|| self.px(self.links.width_dip), |r| r.2)
            .max(1)
    }

    pub(super) fn left_docked_shown(&self) -> bool {
        self.links.shown && self.links.docked
    }

    /// Breite des angedockten Geschossfensters auf dem Bildschirm `work`, sonst 0.
    pub(super) fn left_beside(&self, work: Rect) -> i32 {
        match self.links.rect {
            Some(r) if self.left_docked_shown() && inside(r, work) => self.levels_width(),
            _ => 0,
        }
    }

    /// Knopf „Geschossmanager“, Argumente wie bei [`Windows::open_quantity`].
    /// Angedockt weicht das Hauptfenster nach rechts aus, wenn links kein
    /// Platz ist; maximiert teilt es sich den Bildschirm.
    pub fn open_levels(
        &mut self,
        main: Rect,
        maximized: bool,
        work: Rect,
        scale: f32,
    ) -> Vec<Action> {
        if self.links.shown {
            return vec![Action::Front(WindowId::Levels)];
        }
        self.scale = scale;
        self.links.open = true;
        self.links.shown = true;
        self.min = [false; 3];
        if !self.links.docked {
            if let Some(r) = self.links.rect {
                self.main = main;
                return vec![Action::Create(WindowId::Levels, r)];
            }
            self.links.docked = true;
        }
        let width = self.levels_width();
        if maximized {
            let m = (work.0 + width, work.1, (work.2 - width).max(1), work.3);
            let side = links_at(m, width);
            self.main = m;
            self.links.rect = Some(side);
            self.links.restore_max = true;
            return vec![
                Action::Place(WindowId::Main, m),
                Action::Create(WindowId::Levels, side),
            ];
        }
        // Neben dem Mengenfenster gefüllt: der Rest wird schmaler
        if let Some((before, _)) = self.filled {
            self.links.rect = Some((work.0, work.1, width, work.3));
            let fill = self.fill_rect(work);
            self.filled = Some((before, fill));
            let mut out = self.set_main(fill);
            out.retain(|a| !matches!(a, Action::Place(WindowId::Levels, _)));
            out.push(Action::Create(WindowId::Levels, links_at(fill, width)));
            return out;
        }
        let mut out = Vec::new();
        let mut m = main;
        if m.0 - width < work.0 {
            // Links zu wenig Platz: das Hauptfenster gibt an seiner linken Kante ab
            let dx = work.0 + width - m.0;
            self.links.give_back = Some(dx);
            m = (m.0 + dx, m.1, (m.2 - dx).max(1), m.3);
            out.push(Action::Place(WindowId::Main, m));
        }
        self.main = m;
        let r = links_at(m, width);
        self.links.rect = Some(r);
        out.push(Action::Create(WindowId::Levels, r));
        out
    }

    /// Das angedockte Geschossfenster an das Hauptfenster `m` setzen.
    pub(super) fn follow_left(&mut self, m: Rect) -> Vec<Action> {
        if !self.left_docked_shown() {
            return Vec::new();
        }
        let to = links_at(m, self.levels_width());
        if self.links.rect == Some(to) {
            return Vec::new();
        }
        self.links.rect = Some(to);
        vec![Action::Place(WindowId::Levels, to)]
    }

    /// Das Geschossfenster wird an der Titelleiste nach `r` gezogen: wie
    /// [`Windows::quantity_moved`], am linken Rand des Hauptfensters.
    pub fn levels_moved(&mut self, r: Rect) -> Vec<Action> {
        let m = self.main;
        let dx = (r.0 + r.2 - m.0).abs() as f32;
        let dy = (r.1 - m.1).abs() as f32;
        if self.links.docked {
            let release = RELEASE_DIP * self.scale;
            if dx <= release && dy <= release {
                return self.pin_left(r);
            }
            self.links.docked = false;
        } else {
            let beside = r.1 < m.1 + m.3 && r.1 + r.3 > m.1;
            if beside && dx <= SNAP_DIP * self.scale {
                self.links.docked = true;
                return self.pin_left(r);
            }
        }
        self.links.rect = Some(r);
        Vec::new()
    }

    fn pin_left(&mut self, r: Rect) -> Vec<Action> {
        let to = links_at(self.main, r.2);
        self.links.rect = Some(to);
        if to == r {
            Vec::new()
        } else {
            vec![Action::Place(WindowId::Levels, to)]
        }
    }

    /// Lage oder Größe des Geschossfensters hat sich geändert. Angedockt
    /// bleibt die rechte Kante am Hauptfenster; nur der linke Rand ändert
    /// die Breite.
    pub fn levels_resized(&mut self, r: Rect) -> Vec<Action> {
        if !self.links.docked {
            self.links.rect = Some(r);
            return Vec::new();
        }
        let to = links_at(self.main, (self.main.0 - r.0).max(1));
        self.links.rect = Some(r);
        if to == r {
            return Vec::new();
        }
        self.links.rect = Some(to);
        vec![Action::Place(WindowId::Levels, to)]
    }

    /// Schließen des Geschossfensters: das Hauptfenster bekommt seinen
    /// Platz zurück.
    pub(super) fn close_levels(&mut self) -> Vec<Action> {
        let was = self.links.shown;
        let width = self.levels_width();
        let docked = self.links.docked;
        self.links.open = false;
        self.links.shown = false;
        self.min = [false; 3];
        let give_back = self.links.give_back.take();
        let restore_max = std::mem::take(&mut self.links.restore_max);
        if !was {
            return Vec::new();
        }
        let mut out = vec![Action::Close(WindowId::Levels)];
        if !docked {
            return out;
        }
        let m = self.main;
        let right = self.shown && self.docked;
        if let Some((mut before, fill)) = self.filled {
            if right {
                let fill = (fill.0 - width, fill.1, fill.2 + width, fill.3);
                self.filled = Some((before, fill));
                out.extend(self.set_main(fill));
            } else {
                // Wiederherstell-Lage mit dem Platz von vor dem Öffnen
                if let Some(dx) = give_back {
                    before = (before.0 - dx, before.1, before.2 + dx, before.3);
                }
                self.filled = None;
                out.push(Action::Place(WindowId::Main, before));
                out.push(Action::Show(WindowId::Main, Show::Maximize));
            }
        } else if restore_max && !right {
            out.push(Action::Show(WindowId::Main, Show::Maximize));
        } else if restore_max {
            // Rechts bleibt das Mengenfenster: es maximiert beim Schließen
            self.restore_max = true;
            self.main = (m.0 - width, m.1, m.2 + width, m.3);
            out.push(Action::Place(WindowId::Main, self.main));
        } else if let Some(dx) = give_back.filter(|&dx| dx > 0) {
            self.main = (m.0 - dx, m.1, m.2 + dx, m.3);
            out.push(Action::Place(WindowId::Main, self.main));
        }
        out
    }

    /// Das Geschossfenster wurde vom System geschlossen.
    pub fn levels_gone(&mut self) {
        self.links.shown = false;
        self.links.give_back = None;
        self.links.restore_max = false;
        self.min = [false; 3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = (600, 100, 1200, 800);
    const WORK: Rect = (0, 0, 1920, 1040);

    #[test]
    fn oeffnet_links_angedockt_und_wandert_mit() {
        let mut w = Windows::new(520.0);
        assert_eq!(
            w.open_levels(MAIN, false, WORK, 1.0),
            [Action::Create(WindowId::Levels, (140, 100, 460, 800))]
        );
        assert!(w.levels_open() && w.levels_docked());
        assert_eq!(
            w.main_moved((700, 50, 1000, 700)),
            [Action::Place(WindowId::Levels, (240, 50, 460, 700))]
        );
        assert_eq!(
            w.open_levels(MAIN, false, WORK, 1.0),
            [Action::Front(WindowId::Levels)]
        );
    }

    #[test]
    fn links_kein_platz_hauptfenster_weicht_aus_und_bekommt_ihn_zurueck() {
        let mut w = Windows::new(520.0);
        let main = (100, 100, 1200, 800);
        assert_eq!(
            w.open_levels(main, false, WORK, 1.0),
            [
                Action::Place(WindowId::Main, (460, 100, 840, 800)),
                Action::Create(WindowId::Levels, (0, 100, 460, 800)),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (100, 100, 1200, 800)),
            ]
        );
        assert!(!w.levels_open());
    }

    #[test]
    fn maximiert_teilt_und_maximiert_wieder() {
        let mut w = Windows::new(520.0);
        assert_eq!(
            w.open_levels(MAIN, true, WORK, 1.5),
            [
                Action::Place(WindowId::Main, (690, 0, 1230, 1040)),
                Action::Create(WindowId::Levels, (0, 0, 690, 1040)),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
    }

    #[test]
    fn loesen_und_einrasten() {
        let mut w = Windows::new(520.0);
        w.open_levels(MAIN, false, WORK, 1.0);
        // Wenig gezogen: bleibt am Rand
        assert_eq!(
            w.levels_moved((150, 110, 460, 800)),
            [Action::Place(WindowId::Levels, (140, 100, 460, 800))]
        );
        // Weit gezogen, etwa auf den linken Bildschirm: frei
        assert_eq!(w.levels_moved((-1500, 80, 460, 900)), []);
        assert!(!w.levels_docked());
        assert_eq!(w.main_moved((650, 100, 1200, 800)), [], "frei folgt nicht");
        // Zurück an die Kante: rastet ein
        assert_eq!(
            w.levels_moved((185, 300, 460, 500)),
            [Action::Place(WindowId::Levels, (190, 100, 460, 800))]
        );
        assert!(w.levels_docked());
        // Frei geschlossen: Lage gemerkt, beim Öffnen wieder dort
        w.levels_moved((-1500, 80, 460, 900));
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [Action::Close(WindowId::Levels)]
        );
        assert_eq!(
            w.open_levels(MAIN, false, WORK, 1.0),
            [Action::Create(WindowId::Levels, (-1500, 80, 460, 900))]
        );
    }

    #[test]
    fn linker_rand_aendert_die_breite() {
        let mut w = Windows::new(520.0);
        w.open_levels(MAIN, false, WORK, 1.0);
        assert_eq!(w.levels_resized((40, 100, 560, 800)), []);
        assert_eq!(w.levels_rect(), Some((40, 100, 560, 800)));
        // Rechter Rand gezogen: zurück an die Kante
        assert_eq!(
            w.levels_resized((40, 100, 600, 800)),
            [Action::Place(WindowId::Levels, (40, 100, 560, 800))]
        );
    }

    #[test]
    fn alle_drei_minimieren_und_schliessen_zusammen() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.0);
        w.open_levels(MAIN, false, WORK, 1.0);
        assert_eq!(
            w.minimized(WindowId::Levels),
            [
                Action::Show(WindowId::Main, Show::Minimize),
                Action::Show(WindowId::Quantity, Show::Minimize),
            ]
        );
        assert_eq!(
            w.restored(WindowId::Main),
            [
                Action::Show(WindowId::Quantity, Show::RestoreNoActivate),
                Action::Show(WindowId::Levels, Show::RestoreNoActivate),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Main, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Close(WindowId::Levels),
                Action::Close(WindowId::Main),
            ]
        );
        assert!(w.levels_open(), "offen bleibt gemerkt");
    }

    #[test]
    fn maximieren_fuellt_zwischen_beiden() {
        let mut w = Windows::new(520.0);
        w.open_levels(MAIN, false, WORK, 1.0);
        assert!(w.fills_on_maximize(WORK));
        assert_eq!(
            w.maximize_main(MAIN, WORK),
            [
                Action::Place(WindowId::Main, (460, 0, 1460, 1040)),
                Action::Place(WindowId::Levels, (0, 0, 460, 1040)),
            ]
        );
        // Zum Mengenfenster dazu: der Rest wird schmaler
        let mut w = Windows::new(520.0);
        w.open_quantity((0, 0, 1200, 800), false, WORK, 1.0);
        w.maximize_main((0, 0, 1200, 800), WORK);
        assert_eq!(w.main, (0, 0, 1400, 1040));
        w.open_levels(w.main, false, WORK, 1.0);
        assert_eq!(w.main, (460, 0, 940, 1040));
        assert_eq!(w.levels_rect(), Some((0, 0, 460, 1040)));
        // Geschossfenster zu: der Rest wird wieder breiter
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (0, 0, 1400, 1040)),
            ]
        );
        // Mengenfenster zu, links noch offen: der Rest reicht bis rechts
        w.open_levels(w.main, false, WORK, 1.0);
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Place(WindowId::Main, (460, 0, 1460, 1040)),
            ]
        );
    }

    /// Review #70, 2: Das Hauptfenster bekommt den abgegebenen Platz an
    /// seiner linken Kante zurück, auch nach dem Verschieben auf einen
    /// anderen Bildschirm.
    #[test]
    fn abgegebener_platz_relativ() {
        let mut w = Windows::new(520.0);
        w.open_levels((100, 100, 1200, 800), false, WORK, 1.0);
        w.main_moved((2500, 100, 840, 800));
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (2140, 100, 1200, 800)),
            ]
        );
    }

    /// Review #70, 4: Abgegeben, gefüllt, geschlossen: wiederhergestellt
    /// wird die Lage von vor dem Öffnen.
    #[test]
    fn gefuellt_schliessen_mit_abgegebenem_platz() {
        let mut w = Windows::new(520.0);
        w.open_levels((100, 100, 1200, 800), false, WORK, 1.0);
        w.maximize_main((460, 100, 840, 800), WORK);
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (100, 100, 1200, 800)),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
    }

    /// Review #70, 5: Das Maximieren-Merken des Mengenfensters bleibt nach
    /// dem Schließen im gefüllten Zustand nicht stehen.
    #[test]
    fn mengen_maximieren_merken_verfaellt() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, true, WORK, 1.0);
        w.open_levels((0, 0, 1280, 1040), false, WORK, 1.0);
        w.maximize_main((460, 0, 820, 1040), WORK);
        w.close_requested(WindowId::Quantity, false);
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (0, 0, 1280, 1040)),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
        w.maximized(WindowId::Main);
        w.main_moved((0, 0, 1280, 1040));
        w.open_quantity((0, 0, 1280, 1040), false, WORK, 1.0);
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [Action::Close(WindowId::Quantity)]
        );
    }

    /// Review #70, 6: Geschossfenster geteilt geöffnet, Mengen dazu, das
    /// Geschossfenster zuerst zu: das Mengenfenster maximiert beim Schließen.
    #[test]
    fn maximieren_merken_geht_nach_rechts_ueber() {
        let mut w = Windows::new(520.0);
        w.open_levels(MAIN, true, WORK, 1.0);
        w.open_quantity((460, 0, 1460, 1040), false, WORK, 1.0);
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Place(WindowId::Main, (0, 0, 1400, 1040)),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
    }

    /// Review #70, 7: Mengen geteilt geöffnet, Geschossfenster dazu, Mengen
    /// zuerst zu: kein Maximieren über das angedockte Geschossfenster; das
    /// Hauptfenster bekommt rechts den Platz, das Geschossfenster maximiert
    /// beim Schließen.
    #[test]
    fn mengen_schliessen_maximiert_nicht_ueber_links() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, true, WORK, 1.0);
        w.open_levels((0, 0, 1280, 1040), false, WORK, 1.0);
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Place(WindowId::Main, (460, 0, 1460, 1040)),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Levels, false),
            [
                Action::Close(WindowId::Levels),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
    }
}

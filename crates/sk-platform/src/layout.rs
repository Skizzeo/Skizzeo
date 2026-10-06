//! Regeln für das Mengenfenster (F2, B7) neben dem Hauptfenster: Öffnen,
//! Andocken, Mitwandern, Lösen, Einrasten, Teilen des Bildschirms und
//! gemeinsames Minimieren.
//!
//! Reine Rechnung ohne Systemaufrufe: Jede Meldung des Systems liefert eine
//! Liste von [`Action`]s, die die Windows-Seite ausführt. Deren Rückmeldungen
//! (etwa das `WM_SIZE` des mitminimierten Fensters) kommen wieder hier an und
//! dürfen nichts Neues auslösen.

/// Welches Fenster ein Ereignis oder eine Tat betrifft.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowId {
    /// Hauptfenster mit den Ansichten.
    Main,
    /// Mengenfenster (B7): eigenes Programmfenster mit Taskleisteneintrag,
    /// ohne OpenGL.
    Quantity,
}

/// Rechteck in Bildschirmpixeln: `(x, y, Breite, Höhe)`.
pub type Rect = (i32, i32, i32, i32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// Minimieren ohne Aktivieren.
    Minimize,
    /// Aus dem Minimieren zurückholen, ohne es zu aktivieren.
    RestoreNoActivate,
    Maximize,
}

/// Was die Windows-Seite tun soll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Fenster anlegen und sichtbar machen (ohne Aktivieren).
    Create(WindowId, Rect),
    /// Lage und Größe setzen; ein maximiertes Fenster vorher zurücksetzen.
    Place(WindowId, Rect),
    Show(WindowId, Show),
    /// Nach vorn holen.
    Front(WindowId),
    Close(WindowId),
    /// „Änderungen speichern?“ fragen (das macht die App).
    AskSave,
}

/// Bis zu so vielen dip am rechten Rand des Hauptfensters rastet das
/// Mengenfenster ein.
pub const SNAP_DIP: f32 = 12.0;
/// Erst so viele dip weggezogen löst sich das angedockte Mengenfenster.
pub const RELEASE_DIP: f32 = 24.0;
/// Breite des Mengenfensters beim ersten Öffnen (dip).
pub const WIDTH_DIP: f32 = 520.0;

/// Zustand der beiden Fenster zueinander.
#[derive(Clone, Debug, PartialEq)]
pub struct Windows {
    /// Breite beim ersten Öffnen (dip).
    width_dip: f32,
    /// Mengenfenster offen; wird gemerkt, auch über das Programmende hinaus.
    open: bool,
    /// Das Fenster gibt es gerade (nach dem Start erst nach dem Öffnen).
    shown: bool,
    docked: bool,
    /// Lage des Mengenfensters (zuletzt gesetzt oder gemeldet).
    rect: Option<Rect>,
    /// Lage des Hauptfensters (nicht maximiert).
    main: Rect,
    /// Bildpunkte je dip am Hauptfenster.
    scale: f32,
    /// Minimiert: Haupt- und Mengenfenster.
    min: [bool; 2],
    /// Das Hauptfenster war maximiert und wurde zum Teilen verkleinert: beim
    /// Schließen wieder maximieren.
    restore_max: bool,
    /// Breite des Hauptfensters, bevor es beim Öffnen rechts Platz gemacht
    /// hat: beim Schließen bekommt es sie zurück.
    give_back: Option<i32>,
    /// Maximieren bei angedocktem Mengenfenster: Lage vorher und Lage, die
    /// das Hauptfenster dafür bekommen hat (Rest des Arbeitsbereichs).
    filled: Option<(Rect, Rect)>,
}

fn slot(id: WindowId) -> usize {
    match id {
        WindowId::Main => 0,
        WindowId::Quantity => 1,
    }
}

fn other(id: WindowId) -> WindowId {
    match id {
        WindowId::Main => WindowId::Quantity,
        WindowId::Quantity => WindowId::Main,
    }
}

/// Platz des angedockten Mengenfensters: rechts bündig am Hauptfenster,
/// gleiche Oberkante und Höhe.
fn docked_at(main: Rect, width: i32) -> Rect {
    (main.0 + main.2, main.1, width, main.3)
}

impl Windows {
    pub fn new(width_dip: f32) -> Windows {
        Windows {
            width_dip,
            open: false,
            shown: false,
            docked: true,
            rect: None,
            main: (0, 0, 0, 0),
            scale: 1.0,
            min: [false; 2],
            restore_max: false,
            give_back: None,
            filled: None,
        }
    }

    /// Gemerkter Zustand (einstellungen.txt): offen, angedockt, Lage.
    pub fn remembered(width_dip: f32, open: bool, docked: bool, rect: Option<Rect>) -> Windows {
        Windows {
            open,
            docked,
            rect,
            ..Windows::new(width_dip)
        }
    }

    /// Breite beim Andocken (dip), aus dem Schema.
    pub fn set_width_dip(&mut self, w: f32) {
        if w > 0.0 {
            self.width_dip = w;
        }
    }

    pub fn quantity_open(&self) -> bool {
        self.open
    }

    pub fn docked(&self) -> bool {
        self.docked
    }

    /// Lage des Mengenfensters; `(0, 0, 0, 0)`, solange es nie offen war.
    pub fn quantity_rect(&self) -> Rect {
        self.rect.unwrap_or_default()
    }

    /// Wie [`Windows::quantity_rect`], `None` ohne gemerkte Lage.
    pub fn remembered_rect(&self) -> Option<Rect> {
        self.rect
    }

    fn px(&self, dip: f32) -> i32 {
        (dip * self.scale).round() as i32
    }

    fn width(&self) -> i32 {
        self.rect
            .map_or_else(|| self.px(self.width_dip), |r| r.2)
            .max(1)
    }

    /// Knopf „Mengenermittlung“. `main`: Lage des Hauptfensters (bei
    /// `maximized` beliebig), `work`: Arbeitsbereich seines Bildschirms,
    /// `scale`: Bildpunkte je dip.
    pub fn open_quantity(
        &mut self,
        main: Rect,
        maximized: bool,
        work: Rect,
        scale: f32,
    ) -> Vec<Action> {
        if self.shown {
            return vec![Action::Front(WindowId::Quantity)];
        }
        self.scale = scale;
        self.open = true;
        self.shown = true;
        self.min = [false; 2];
        if !self.docked {
            if let Some(r) = self.rect {
                self.main = main;
                return vec![Action::Create(WindowId::Quantity, r)];
            }
            self.docked = true;
        }
        if maximized {
            // Bildschirm teilen: Hauptfenster zwei Drittel, Mengen ein Drittel
            let mw = work.2 * 2 / 3;
            self.main = (work.0, work.1, mw, work.3);
            let side = (work.0 + mw, work.1, work.2 - mw, work.3);
            self.rect = Some(side);
            self.restore_max = true;
            return vec![
                Action::Place(WindowId::Main, self.main),
                Action::Create(WindowId::Quantity, side),
            ];
        }
        let width = self.width();
        let mut out = Vec::new();
        let mut m = main;
        // Rechts zu wenig Platz: Hauptfenster gibt am rechten Rand Breite ab
        let right = work.0 + work.2;
        if m.0 + m.2 + width > right {
            let w = (right - width - m.0).max(1);
            self.give_back = Some(m.2);
            m = (m.0, m.1, w, m.3);
            out.push(Action::Place(WindowId::Main, m));
        }
        self.main = m;
        let r = docked_at(m, width);
        self.rect = Some(r);
        out.push(Action::Create(WindowId::Quantity, r));
        out
    }

    /// Hat das Hauptfenster für das angedockte Mengenfenster den Bildschirm
    /// gefüllt (zeigt sich wie maximiert)?
    pub fn main_filled(&self) -> bool {
        self.filled.is_some()
    }

    /// Wird Maximieren des Hauptfensters (Bildschirm `work`) zum Füllen des
    /// Rests neben dem Mengenfenster?
    pub fn fills_on_maximize(&self, work: Rect) -> bool {
        let Some(q) = self.rect else {
            return false;
        };
        let (cx, cy) = (q.0 + q.2 / 2, q.1 + q.3 / 2);
        let same = cx >= work.0 && cx < work.0 + work.2 && cy >= work.1 && cy < work.1 + work.3;
        self.shown && self.docked && same
    }

    /// Maximieren des Hauptfensters (Lage `main`, Bildschirm `work`). Bei
    /// angedocktem Mengenfenster auf demselben Bildschirm füllt es den Rest
    /// des Arbeitsbereichs; das Mengenfenster behält seine Breite. Noch
    /// einmal: zurück. Leer: ganz normal maximieren.
    pub fn maximize_main(&mut self, main: Rect, work: Rect) -> Vec<Action> {
        if let Some((before, _)) = self.filled.take() {
            return self.set_main(before);
        }
        if !self.fills_on_maximize(work) {
            return Vec::new();
        }
        let w = self.width();
        let fill = (work.0, work.1, (work.2 - w).max(1), work.3);
        self.filled = Some((main, fill));
        self.set_main(fill)
    }

    /// Hauptfenster setzen, das angedockte Mengenfenster gleich mit.
    fn set_main(&mut self, m: Rect) -> Vec<Action> {
        self.main = m;
        let mut out = vec![Action::Place(WindowId::Main, m)];
        if self.shown && self.docked {
            out.extend(self.place(docked_at(m, self.width())));
        }
        out
    }

    /// Lage des Hauptfensters merken, ohne etwas auszulösen (etwa vor dem
    /// Ziehen des Mengenfensters).
    pub fn note_main(&mut self, r: Rect) {
        self.main = r;
    }

    /// Das Hauptfenster wurde verschoben oder in der Größe geändert.
    pub fn main_moved(&mut self, r: Rect) -> Vec<Action> {
        self.main = r;
        // Vom Nutzer verschoben oder anders groß: nicht mehr „maximiert“
        if self.filled.is_some_and(|(_, f)| f != r) {
            self.filled = None;
        }
        if !self.shown || !self.docked || self.min.contains(&true) {
            return Vec::new();
        }
        let to = docked_at(r, self.width());
        self.place(to)
    }

    fn place(&mut self, to: Rect) -> Vec<Action> {
        if self.rect == Some(to) {
            return Vec::new();
        }
        self.rect = Some(to);
        vec![Action::Place(WindowId::Quantity, to)]
    }

    /// Das Mengenfenster wird an der Titelleiste nach `r` gezogen.
    ///
    /// Angedockt bleibt es am Rand, bis es mehr als [`RELEASE_DIP`] weggezogen
    /// ist. Frei rastet es ein, sobald seine linke Kante bis auf [`SNAP_DIP`]
    /// an den rechten Rand des Hauptfensters kommt und es neben ihm liegt;
    /// dann bekommt es Oberkante und Höhe des Hauptfensters.
    pub fn quantity_moved(&mut self, r: Rect) -> Vec<Action> {
        let m = self.main;
        let dx = (r.0 - (m.0 + m.2)).abs() as f32;
        let dy = (r.1 - m.1).abs() as f32;
        if self.docked {
            let release = RELEASE_DIP * self.scale;
            if dx <= release && dy <= release {
                return self.pin(r);
            }
            self.docked = false;
        } else {
            let beside = r.1 < m.1 + m.3 && r.1 + r.3 > m.1;
            if beside && dx <= SNAP_DIP * self.scale {
                self.docked = true;
                return self.pin(r);
            }
        }
        self.rect = Some(r);
        Vec::new()
    }

    /// Hält das gezogene Fenster `r` am Anker fest.
    fn pin(&mut self, r: Rect) -> Vec<Action> {
        let to = docked_at(self.main, r.2);
        self.rect = Some(to);
        if to == r {
            Vec::new()
        } else {
            vec![Action::Place(WindowId::Quantity, to)]
        }
    }

    /// Lage oder Größe des Mengenfensters hat sich geändert (Rand gezogen,
    /// Bildschirm gewechselt). Angedockt bleibt die linke Kante am Anker;
    /// nur der rechte Rand ändert die Breite.
    pub fn quantity_resized(&mut self, r: Rect) -> Vec<Action> {
        if !self.docked {
            self.rect = Some(r);
            return Vec::new();
        }
        let left = self.main.0 + self.main.2;
        let to = docked_at(self.main, (r.0 + r.2 - left).max(1));
        if to == r {
            self.rect = Some(r);
            return Vec::new();
        }
        self.rect = Some(r);
        self.place(to)
    }

    /// Ein Fenster wurde minimiert (Knopf, Taskleiste, Win+D, Win+M oder als
    /// Folge einer eigenen Tat): das andere mit.
    pub fn minimized(&mut self, id: WindowId) -> Vec<Action> {
        if self.min[slot(id)] {
            return Vec::new();
        }
        self.min[slot(id)] = true;
        let o = other(id);
        if !self.shown || self.min[slot(o)] {
            return Vec::new();
        }
        self.min[slot(o)] = true;
        vec![Action::Show(o, Show::Minimize)]
    }

    /// Ein Fenster kam aus dem Minimieren zurück (Taskleiste, Alt+Tab): das
    /// andere mit, ohne es zu aktivieren.
    pub fn restored(&mut self, id: WindowId) -> Vec<Action> {
        if !self.min[slot(id)] {
            return Vec::new();
        }
        self.min[slot(id)] = false;
        let o = other(id);
        if !self.shown || !self.min[slot(o)] {
            return Vec::new();
        }
        self.min[slot(o)] = false;
        vec![Action::Show(o, Show::RestoreNoActivate)]
    }

    /// Maximieren betrifft nur das eigene Fenster.
    pub fn maximized(&mut self, _id: WindowId) -> Vec<Action> {
        Vec::new()
    }

    /// Schließen angefordert; `changed`: ungespeicherte Änderungen.
    /// Das Mengenfenster geht ohne Frage zu, das Hauptfenster nimmt es mit.
    pub fn close_requested(&mut self, id: WindowId, changed: bool) -> Vec<Action> {
        match id {
            WindowId::Quantity => {
                let was = self.shown;
                self.open = false;
                self.shown = false;
                self.min = [false; 2];
                if !was {
                    return Vec::new();
                }
                let mut out = vec![Action::Close(WindowId::Quantity)];
                let m = self.main;
                if let Some((mut before, _)) = self.filled.take() {
                    // Gefüllt: jetzt richtig maximieren, vorher die alte Lage
                    // (mit der Breite von vor dem Öffnen, falls es abgab)
                    if let Some(w) = self.give_back {
                        before.2 = w;
                    }
                    out.push(Action::Place(WindowId::Main, before));
                    out.push(Action::Show(WindowId::Main, Show::Maximize));
                    self.restore_max = false;
                } else if std::mem::take(&mut self.restore_max) {
                    out.push(Action::Show(WindowId::Main, Show::Maximize));
                } else if let Some(w) = self.give_back.take() {
                    self.main = (m.0, m.1, w, m.3);
                    out.push(Action::Place(WindowId::Main, self.main));
                }
                self.give_back = None;
                out
            }
            WindowId::Main if changed => vec![Action::AskSave],
            WindowId::Main => {
                // Offen bleibt gemerkt: beim nächsten Start kommt es wieder
                let mut out = Vec::new();
                if std::mem::take(&mut self.shown) {
                    out.push(Action::Close(WindowId::Quantity));
                }
                out.push(Action::Close(WindowId::Main));
                out
            }
        }
    }

    /// Das Fenster wurde vom System geschlossen (nicht über
    /// [`Windows::close_requested`]).
    pub fn quantity_gone(&mut self) {
        self.shown = false;
        self.min = [false; 2];
        self.restore_max = false;
        self.give_back = None;
        self.filled = None;
    }
}

/// Liegt die Mitte von `r` auf einem der Bildschirme?
pub fn on_screen(r: Rect, monitors: &[Rect]) -> bool {
    let (cx, cy) = (r.0 + r.2 / 2, r.1 + r.3 / 2);
    monitors
        .iter()
        .any(|m| cx >= m.0 && cx < m.0 + m.2 && cy >= m.1 && cy < m.1 + m.3)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = (100, 100, 1200, 800);
    const WORK: Rect = (0, 0, 1920, 1040);

    #[test]
    fn gemerkt_frei_oeffnet_an_seinem_platz() {
        let mut w = Windows::remembered(520.0, true, false, Some((2100, 80, 600, 900)));
        assert!(w.quantity_open());
        assert_eq!(
            w.open_quantity(MAIN, false, WORK, 1.0),
            [Action::Create(WindowId::Quantity, (2100, 80, 600, 900))]
        );
        assert_eq!(w.main_moved((0, 0, 800, 600)), [], "frei folgt nicht");
    }

    #[test]
    fn angedockt_gemerkte_breite() {
        let mut w = Windows::remembered(520.0, true, true, Some((5, 5, 610, 400)));
        assert_eq!(
            w.open_quantity(MAIN, false, WORK, 1.0),
            [Action::Create(WindowId::Quantity, (1300, 100, 610, 800))],
            "gemerkte Breite, Höhe und Lage vom Hauptfenster"
        );
    }

    #[test]
    fn oeffnen_gibt_breite_ab_und_zurueck() {
        let mut w = Windows::new(520.0);
        let t = w.open_quantity(MAIN, false, WORK, 1.5);
        assert_eq!(
            t,
            [
                Action::Place(WindowId::Main, (100, 100, 1040, 800)),
                Action::Create(WindowId::Quantity, (1140, 100, 780, 800)),
            ]
        );
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Place(WindowId::Main, (100, 100, 1200, 800)),
            ]
        );
        // Abgegeben, dann gefüllt und geschlossen: Wiederherstellen mit alter Breite
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.5);
        w.maximize_main((100, 100, 1040, 800), WORK);
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Place(WindowId::Main, (100, 100, 1200, 800)),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
    }

    #[test]
    fn maximieren_fuellt_den_rest() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.0);
        assert!(w.fills_on_maximize(WORK));
        assert_eq!(
            w.maximize_main(MAIN, WORK),
            [
                Action::Place(WindowId::Main, (0, 0, 1400, 1040)),
                Action::Place(WindowId::Quantity, (1400, 0, 520, 1040)),
            ]
        );
        assert!(w.main_filled());
        // Rückmeldung der eigenen Tat ändert nichts
        assert_eq!(w.main_moved((0, 0, 1400, 1040)), []);
        assert!(w.main_filled());
        // Noch einmal: zurück
        assert_eq!(
            w.maximize_main((0, 0, 1400, 1040), WORK),
            [
                Action::Place(WindowId::Main, MAIN),
                Action::Place(WindowId::Quantity, (1300, 100, 520, 800)),
            ]
        );
        assert!(!w.main_filled());
        // Gefüllt schließen: richtig maximiert
        w.maximize_main(MAIN, WORK);
        assert_eq!(
            w.close_requested(WindowId::Quantity, false),
            [
                Action::Close(WindowId::Quantity),
                Action::Place(WindowId::Main, MAIN),
                Action::Show(WindowId::Main, Show::Maximize),
            ]
        );
        // Gelöst oder auf einem anderen Bildschirm: ganz normal
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.0);
        w.quantity_moved((2100, 80, 520, 800));
        assert_eq!(w.maximize_main(MAIN, WORK), []);
        let mut w = Windows::remembered(520.0, true, true, None);
        w.open_quantity(MAIN, false, WORK, 1.0);
        assert!(!w.fills_on_maximize((1920, 0, 1920, 1040)));
    }

    #[test]
    fn minimiert_folgt_nicht() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.0);
        w.minimized(WindowId::Main);
        assert_eq!(w.main_moved((0, 0, 100, 100)), []);
    }

    #[test]
    fn hauptfenster_schliessen_merkt_offen() {
        let mut w = Windows::new(520.0);
        w.open_quantity(MAIN, false, WORK, 1.0);
        w.close_requested(WindowId::Main, false);
        assert!(w.quantity_open());
        assert_eq!(
            w.open_quantity(MAIN, false, WORK, 1.0),
            [Action::Create(WindowId::Quantity, (1300, 100, 520, 800))]
        );
    }

    #[test]
    fn auf_dem_bildschirm() {
        let mons = [WORK, (1920, 0, 1920, 1040)];
        assert!(on_screen((2100, 80, 600, 900), &mons));
        assert!(!on_screen((2100, 80, 600, 900), &mons[..1]));
    }
}

//! Bemaßung bedienen (Konzept `bemassung/konzept.md` §4, §5, Jörn 12.10.
//! 14:23–14:35): Klick auf eine Maßzahl öffnet sie zur Eingabe; das Bauteil
//! am rechten bzw. oberen Ende (Pfeil) springt bei jeder Ziffer an die
//! Stelle, an der das Maß stimmt, Tab nimmt das andere Ende. Enter oder ein
//! Klick in die Ansicht übernimmt (ein Schritt), Esc bricht ab.
//!
//! Ketten lassen sich greifen: Klick auf die Maßlinie und ziehen schiebt die
//! gewählten Ketten quer in 10-mm-Schritten; Strg+Klick nimmt eine Kette
//! dazu oder weg, Umschalt+Klick den ganzen Block der Seite, ein Rechteck
//! von rechts nach links im freien Bereich jede Kette, die es kreuzt.
//! Doppelklick setzt den Versatz zurück, Esc hebt die Wahl auf.
//!
//! Beim Ziehen am Gummiband rechnet [`Masse::bezugsmass`] das Maß hinter dem
//! Bauteil; getippt wird dann dieses Maß ([`crate::wall_edit`]).

use super::mass_treffer::{self as tr, Bezugsmass, KettenId, MassId, Treffer};
use super::EditOutcome;
use crate::bemassung::Stand;
use crate::camera::Camera;
use crate::measure_input::{InputOutcome, MeasureInput};
use crate::meldung::Meldung;
use crate::scene::Scene;
use crate::ui::MeasureKind;
use sk_math::{vec3, Vec3};
use sk_model::oeffnung::{Opening, MAX_REVEAL};
use sk_model::{ElementId, RunId, WallChain};
use sk_plan::aussen::{Bezug, Quelle};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_render::{Helper, SOLID};
use sk_ui::theme::Theme;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Schritte im Verlauf.
pub const SCHRITT_MASS: &str = "Maß geändert";
pub const SCHRITT_KETTE: &str = "Maßkette verschoben";
pub const SCHRITT_ZURUECK: &str = "Maßkette zurückgesetzt";
/// Feldname der Eingabe (auch beim Ziehen am Band mit Kettenbezug).
pub const LABEL: &str = "Maß";
/// Hinweis an einer Maßzahl und an einer Maßlinie.
pub const TIP_ZAHL: &str = "Klicken: Maß überschreiben · Ziehen: Kette verschieben";
pub const TIP_LINIE: &str =
    "Ziehen: Kette verschieben · Strg: dazuwählen · Umschalt: ganzer Block · Doppelklick: zurück";
/// Raster beim Verschieben einer Kette (mm).
const RASTER: f64 = 10.0;
/// Bis hierhin ist es ein Klick (px bei 96 dpi).
const KLICK_PX: f64 = 4.0;
/// Zwei Klicks auf dieselbe Kette in dieser Zeit: Doppelklick.
const DOPPEL: Duration = Duration::from_millis(500);
/// Strichlänge des Kreuzungsrechtecks (dip).
const STRICH: f32 = 5.0;

/// Was das Bild der Ketten hervorhebt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hervor {
    /// Gewählte Ketten (Auswahlfarbe).
    pub gewaehlt: Vec<KettenId>,
    /// Kette unter der Maus.
    pub ueber: Option<KettenId>,
    /// Maßzahl unter der Maus (`false`) bzw. in Arbeit (`true`).
    pub zahl: Option<(MassId, bool)>,
    /// Maßzahl in Eingabe und ihr bewegtes Ende (0 Anfang, 1 Ende).
    pub ende: Option<(MassId, usize)>,
}

/// Gedrückt auf einer Kette, noch nicht gezogen.
struct Druck {
    x: f64,
    y: f64,
    treffer: Treffer,
    mods: Modifiers,
}

/// Gewählte Ketten beim Ziehen: Ebene, Startpunkt, je Kette ihre Normale
/// und ihr Versatz beim Greifen.
struct Kettenzug {
    z: f64,
    start: Vec3,
    ketten: Vec<(KettenId, Vec3, f64)>,
}

/// Was sich bewegt, wenn das Maß überschrieben wird: der Stand beim Öffnen.
#[derive(Clone)]
enum Bauteil {
    /// Segment eines Außenwandzugs quer zur Kette.
    Wand {
        run: RunId,
        seg: usize,
        chain: WallChain,
    },
    /// Eine angeschlossene Innenwand als Ganzes.
    Innenwand { run: RunId, chain: WallChain },
    /// Eine Öffnung: ganz, oder nur die Kante bei `kante` (Breite).
    Oeffnung {
        id: ElementId,
        o: Opening,
        kante: Option<f64>,
    },
}

impl Bauteil {
    /// Das Bauteil am Bezug `q` (das andere Ende ist `r`), wie es jetzt steht.
    fn von(scene: &Scene, q: &Bezug, r: &Bezug) -> Option<Bauteil> {
        Some(match q.quelle? {
            Quelle::Wand { run, seg } => Bauteil::Wand {
                run,
                seg,
                chain: scene.chain(run)?.clone(),
            },
            Quelle::Innenwand { run } => Bauteil::Innenwand {
                run,
                chain: scene.chain(run)?.clone(),
            },
            Quelle::Oeffnung { id, .. } => {
                let breite = matches!(r.quelle, Some(Quelle::Oeffnung { id: x, .. }) if x == id);
                Bauteil::Oeffnung {
                    id,
                    o: scene.model().opening(id)?.clone(),
                    kante: breite.then_some(q.t),
                }
            }
        })
    }

    /// Verschiebt das Bauteil um `dt` längs `t` (im offenen Schritt).
    fn setzen(&self, scene: &mut Scene, t: Vec3, dt: f64) -> bool {
        match self {
            Bauteil::Wand { run, seg, chain } => {
                let Some(n) = chain.segment_normal(*seg) else {
                    return false;
                };
                let k = n.dot(t);
                if k.abs() < 0.5 {
                    return false;
                }
                match chain.with_segment_moved_snap(*seg, dt / k, 0.0) {
                    Some(c) => scene.set_run_points(*run, &c.points),
                    None => false,
                }
            }
            Bauteil::Innenwand { run, chain } => {
                let d = vec3(t.x, t.y, 0.0) * dt;
                let pts: Vec<Vec3> = chain.points.iter().map(|p| *p + d).collect();
                scene.set_run_points(*run, &pts)
            }
            Bauteil::Oeffnung { id, o, kante } => {
                let d = vec3(t.x, t.y, 0.0) * dt;
                let mut o = o.clone();
                let mut bewegt = false;
                // Bei „Tragende Schicht“ steht die Kante um den Anschlag
                // vor dem Pfad (Bedienbarkeit 40.1): der nächste Pfadpunkt
                let naechst = kante.map_or(0.0, |k| {
                    o.path
                        .iter()
                        .map(|p| (p.dot(t) - k).abs())
                        .fold(f64::INFINITY, f64::min)
                });
                let weit = if naechst <= MAX_REVEAL {
                    naechst + 1.0
                } else {
                    1.0
                };
                for p in &mut o.path {
                    if kante.is_none_or(|k| (p.dot(t) - k).abs() < weit) {
                        *p += d;
                        bewegt = true;
                    }
                }
                // Keine Kante an der Stelle: Fehler statt leerem Schritt
                bewegt && scene.set_opening_live(*id, o)
            }
        }
    }

    /// Zug, der live mitgeht.
    fn lauf(&self, scene: &Scene) -> Option<RunId> {
        match self {
            Bauteil::Wand { run, .. } | Bauteil::Innenwand { run, .. } => Some(*run),
            Bauteil::Oeffnung { .. } => scene.model().step_touched().first().copied(),
        }
    }
}

/// Eine Maßzahl in Eingabe.
struct Eingabe {
    mass: MassId,
    /// Bezüge beim Öffnen: Anfang und Ende.
    r: [Bezug; 2],
    /// Richtung längs der Kette.
    t: Vec3,
    /// Maß beim Öffnen (mm).
    wert: f64,
    /// Bewegtes Ende: 0 Anfang, 1 Ende.
    ende: usize,
    bauteil: Bauteil,
    input: MeasureInput,
    lauf: Option<RunId>,
}

impl Eingabe {
    /// Setzt den getippten Wert vom Stand beim Öffnen aus; ohne Wert der
    /// Stand beim Öffnen. Gilt er nicht, bleibt das Bauteil und die Eingabe
    /// zeigt den Fehler; der Fehler gilt nur für diesen Versuch (nach Tab
    /// kann das andere Ende den Wert annehmen, Review W36).
    fn anwenden(&mut self, scene: &mut Scene) {
        scene.rollback();
        scene.begin(SCHRITT_MASS);
        self.input.error = self.input.value(0).and_then(Result::err);
        let Some(Ok(v)) = self.input.value(0).filter(|_| !self.input.vorbelegt()) else {
            return;
        };
        if v < 1.0 {
            self.input.error = Some(Meldung::satz("Ein Maß ist mindestens 1 mm lang."));
            return;
        }
        let dt = if self.ende == 1 {
            v - self.wert
        } else {
            self.wert - v
        };
        if dt.abs() < 0.05 {
            return;
        }
        if self.bauteil.setzen(scene, self.t, dt) {
            self.lauf = self.bauteil.lauf(scene).or(self.lauf);
        } else {
            scene.rollback();
            scene.begin(SCHRITT_MASS);
            self.input.error = Some(Meldung::satz("So kann das Bauteil nicht stehen."));
        }
    }

    /// Gültiger Wert, der gesetzt ist?
    fn gilt(&self) -> bool {
        matches!(self.input.value(0), Some(Ok(_)))
            && self.input.error.is_none()
            && !self.input.vorbelegt()
    }
}

/// Zustand der Bemaßung beim Bedienen; Teil von [`crate::wall_edit::WallEdit`].
#[derive(Default)]
pub struct Masse {
    /// Ketten der Ansicht (`None`: Ansicht ohne Bemaßung).
    stand: Option<Rc<Stand>>,
    /// Größe der Ansicht beim letzten Ereignis (für das Rechteck).
    groesse: (f64, f64),
    ueber: Option<Treffer>,
    /// Gewählte Ketten.
    gewaehlt: Vec<KettenId>,
    druck: Option<Druck>,
    zug: Option<Kettenzug>,
    /// Gedrückt im freien Bereich; ab dem Ziehen nach links das Rechteck
    /// bis `rechteck`.
    frei: Option<(f64, f64)>,
    rechteck: Option<(f64, f64)>,
    eingabe: Option<Eingabe>,
    /// Letzter Klick auf eine Maßlinie (Doppelklick).
    klick: Option<(Instant, KettenId)>,
    /// Zählt jede sichtbare Änderung (neu zeichnen).
    rev: u64,
}

impl Masse {
    /// Ketten der Ansicht vor jedem Ereignis. Ohne Bemaßung (andere
    /// Ansicht, ausgeblendet) endet alles, was läuft.
    pub fn setze(&mut self, stand: Option<Rc<Stand>>, scene: &mut Scene) {
        if stand.is_none() && self.stand.is_some() {
            if self.eingabe.take().is_some() || self.zug.take().is_some() {
                scene.rollback();
            }
            self.gewaehlt.clear();
            (self.ueber, self.druck, self.frei, self.rechteck) = (None, None, None, None);
            self.neu();
        }
        self.stand = stand;
    }

    fn neu(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

    /// Zählt jede sichtbare Änderung.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// Gewählte Ketten.
    #[cfg(test)]
    pub fn gewaehlt(&self) -> &[KettenId] {
        &self.gewaehlt
    }

    /// Läuft etwas (Eingabe, Ziehen, Rechteck, gedrückt)? Dann nimmt die
    /// Bemaßung alle Ereignisse der Ansicht.
    pub fn aktiv(&self) -> bool {
        self.eingabe.is_some()
            || self.zug.is_some()
            || self.druck.is_some()
            || self.rechteck.is_some()
    }

    /// Unter der Maus oder aktiv.
    pub fn is_busy(&self) -> bool {
        self.ueber.is_some() || self.aktiv()
    }

    /// Zug, der beim Überschreiben live mitgeht.
    pub fn lauf(&self) -> Option<RunId> {
        self.eingabe.as_ref().and_then(|e| e.lauf)
    }

    /// Offene Eingabe.
    pub fn input(&self) -> Option<&MeasureInput> {
        self.eingabe.as_ref().map(|e| &e.input)
    }

    /// Pille der Eingabe.
    pub fn pill(&self) -> Option<(String, bool)> {
        self.eingabe
            .as_ref()
            .map(|e| (e.input.text([LABEL, ""]), true))
    }

    /// Mauszeiger über einer Zahl oder Maßlinie.
    pub fn cursor(&self) -> Option<Cursor> {
        (self.ueber.is_some() || self.druck.is_some() || self.zug.is_some()).then_some(Cursor::Hand)
    }

    /// Hinweis unter der Maus.
    pub fn tip(&self) -> Option<&'static str> {
        if self.aktiv() {
            return None;
        }
        match self.ueber? {
            Treffer::Zahl(_) => Some(TIP_ZAHL),
            Treffer::Linie(_) => Some(TIP_LINIE),
        }
    }

    /// Was das Bild hervorhebt.
    pub fn hervor(&self) -> Hervor {
        let zahl = match (&self.eingabe, self.ueber) {
            (Some(e), _) => Some((e.mass, true)),
            (None, Some(Treffer::Zahl(m))) => Some((m, false)),
            _ => None,
        };
        let ueber = match self.ueber {
            Some(Treffer::Linie(k)) if self.eingabe.is_none() => Some(k),
            _ => None,
        };
        Hervor {
            gewaehlt: self.gewaehlt.clone(),
            ueber,
            zahl,
            ende: self.eingabe.as_ref().map(|e| (e.mass, e.ende)),
        }
    }

    /// Bezugsmaß beim Ziehen am Band ([`tr::bezugsmass`]).
    pub fn bezugsmass(&self, run: RunId, seg: usize, n: Vec3, p: Vec3) -> Option<Bezugsmass> {
        tr::bezugsmass(self.stand.as_deref()?, run, seg, n, p)
    }

    /// Verarbeitet ein Ereignis der Ansicht. `None`: nicht für die
    /// Bemaßung, das Band bekommt es. `band`: die Maus steht über dem
    /// Gummiband oder einem Griff (dort beginnt kein Rechteck). Neu
    /// gezeichnet wird, wenn sich [`Masse::rev`] ändert.
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        scene: &mut Scene,
        cam: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        enabled: bool,
        band: bool,
    ) -> Option<EditOutcome> {
        self.groesse = (w, h);
        // Ein anderer hat den Schritt geschlossen: Eingabe und Ziehen sind vorbei
        if (self.eingabe.is_some() || self.zug.is_some()) && !scene.model().in_step() {
            (self.eingabe, self.zug) = (None, None);
            self.neu();
        }
        let stand = self.stand.clone()?;
        let mut out = EditOutcome {
            consumed: true,
            ..Default::default()
        };
        if self.eingabe.is_some() {
            return self.eingabe_event(e, scene, &mut out).then_some(out);
        }
        if self.zug.is_some() {
            self.zug_event(e, scene, cam, w, h);
            return Some(out);
        }
        let unter = |x: f64, y: f64| {
            enabled
                .then(|| tr::treffer(&stand, cam, w, h, scale, x, y))
                .flatten()
        };
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some(d) = &self.druck {
                    if (x - d.x).hypot(y - d.y) > KLICK_PX * scale {
                        self.zug_starten(&stand, scene, cam, w, h);
                        self.zug_event(e, scene, cam, w, h);
                    }
                    return Some(out);
                }
                if let Some((fx, fy)) = self.frei {
                    let weit = (x - fx).hypot(y - fy) > KLICK_PX * scale;
                    if self.rechteck.is_some() || (enabled && weit && x < fx) {
                        self.rechteck = Some((x, y));
                        self.neu();
                        return Some(out);
                    }
                    if weit {
                        // Von links nach rechts: frei für eine spätere Fensterauswahl
                        self.frei = None;
                    }
                }
                let t = unter(x, y);
                if t != self.ueber {
                    self.ueber = t;
                    self.neu();
                }
                t.map(|_| EditOutcome::default())
            }
            Event::MouseLeave => {
                if self.ueber.take().is_some() {
                    self.neu();
                }
                self.frei = None;
                None
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                let Some(t) = unter(x, y) else {
                    // Klick ins Freie hebt die Wahl auf; das Rechteck
                    // beginnt erst beim Ziehen nach links
                    if !self.gewaehlt.is_empty() {
                        self.gewaehlt.clear();
                        self.neu();
                    }
                    self.frei = (enabled && !band).then_some((x, y));
                    return None;
                };
                let k = t.kette();
                // Doppelklick auf die Maßlinie: Versatz zurück
                if let Some((t0, k0)) = self.klick.take() {
                    if k0 == k && t0.elapsed() <= DOPPEL && matches!(t, Treffer::Linie(_)) {
                        self.zuruecksetzen(scene, k);
                        return Some(out);
                    }
                }
                if mods.ctrl {
                    match self.gewaehlt.iter().position(|x| *x == k) {
                        Some(i) => {
                            self.gewaehlt.remove(i);
                        }
                        None => self.gewaehlt.push(k),
                    }
                } else if mods.shift {
                    for b in tr::block(&stand, k) {
                        if !self.gewaehlt.contains(&b) {
                            self.gewaehlt.push(b);
                        }
                    }
                } else if !self.gewaehlt.contains(&k) {
                    self.gewaehlt = vec![k];
                }
                self.druck = Some(Druck {
                    x,
                    y,
                    treffer: t,
                    mods,
                });
                self.neu();
                Some(out)
            }
            Event::MouseUp {
                button: MouseButton::Left,
                mods,
                ..
            } => {
                let frei = self.frei.take();
                if let (Some(bis), Some(von)) = (self.rechteck.take(), frei) {
                    if !mods.ctrl {
                        self.gewaehlt.clear();
                    }
                    for k in tr::im_rechteck(&stand, cam, w, h, von, bis) {
                        if !self.gewaehlt.contains(&k) {
                            self.gewaehlt.push(k);
                        }
                    }
                    self.neu();
                    return Some(out);
                }
                let d = self.druck.take()?;
                self.neu();
                match d.treffer {
                    Treffer::Zahl(m) if !d.mods.ctrl && !d.mods.shift => {
                        self.gewaehlt.clear();
                        self.oeffnen(&stand, scene, m);
                        out.changed = self.eingabe.is_some();
                    }
                    Treffer::Linie(k) if !d.mods.ctrl && !d.mods.shift => {
                        self.gewaehlt = vec![k];
                        self.klick = Some((Instant::now(), k));
                    }
                    _ => {}
                }
                Some(out)
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } if !self.gewaehlt.is_empty() => {
                self.gewaehlt.clear();
                self.neu();
                Some(out)
            }
            _ => None,
        }
    }

    /// Öffnet das Maß `m` zur Eingabe (ein offener Schritt).
    fn oeffnen(&mut self, stand: &Stand, scene: &mut Scene, m: MassId) {
        let Some((r0, r1)) = tr::enden(stand, m) else {
            return;
        };
        let (Some(ende), Some(f)) = (tr::erstes_ende(&r0, &r1), tr::finde(stand, m.kette)) else {
            return;
        };
        let r = [r0, r1];
        let Some(bauteil) = Bauteil::von(scene, &r[ende], &r[1 - ende]) else {
            return;
        };
        scene.begin(SCHRITT_MASS);
        // Mit dem Maß in m vorbelegt und markiert: Die Zahl steht in cm,
        // getippt wird in m (Bedienbarkeit 40.3)
        let mut input = MeasureInput::new(MeasureKind::Length, None);
        input.vorbelegen(&crate::ui::m_text(r1.t - r0.t));
        self.eingabe = Some(Eingabe {
            mass: m,
            r,
            t: f.seite.t,
            wert: r1.t - r0.t,
            ende,
            lauf: bauteil.lauf(scene),
            bauteil,
            input,
        });
    }

    /// Ereignis bei offener Eingabe; `false`: nicht für sie (Mausrad).
    fn eingabe_event(&mut self, e: &Event, scene: &mut Scene, out: &mut EditOutcome) -> bool {
        match *e {
            // Klick in die Ansicht übernimmt (Jörn 14:26)
            Event::MouseDown {
                button: MouseButton::Left,
                ..
            } => {
                self.schliessen(scene, true);
                out.changed = true;
            }
            Event::Key {
                key: Key::Tab,
                down: true,
                ..
            } => {
                out.changed = self.anderes_ende(scene);
                self.neu();
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => {
                let Some(ein) = self.eingabe.as_mut() else {
                    return false;
                };
                match ein.input.key(key, mods) {
                    InputOutcome::Changed | InputOutcome::Emptied => {
                        ein.anwenden(scene);
                        out.changed = true;
                    }
                    InputOutcome::Enter => {
                        self.schliessen(scene, true);
                        out.changed = true;
                    }
                    InputOutcome::EnterEmpty | InputOutcome::Escape => {
                        self.schliessen(scene, false);
                        out.changed = true;
                    }
                    InputOutcome::Refused | InputOutcome::Ignored => {}
                }
                self.neu();
            }
            Event::Wheel { .. } | Event::MouseLeave => return false,
            _ => {}
        }
        true
    }

    /// Übernimmt (wenn der Wert gilt) oder verwirft die Eingabe.
    fn schliessen(&mut self, scene: &mut Scene, uebernehmen: bool) {
        let Some(ein) = self.eingabe.take() else {
            return;
        };
        if uebernehmen && ein.gilt() {
            scene.commit();
        } else {
            scene.rollback();
        }
        self.neu();
    }

    /// Tab: das andere Ende bewegt sich (vom Stand beim Öffnen aus).
    fn anderes_ende(&mut self, scene: &mut Scene) -> bool {
        let Some(ein) = self.eingabe.as_mut() else {
            return false;
        };
        let neu = 1 - ein.ende;
        if !tr::beweglich(&ein.r[0], &ein.r[1], neu) {
            return false;
        }
        scene.rollback();
        match Bauteil::von(scene, &ein.r[neu], &ein.r[1 - neu]) {
            Some(b) => {
                ein.ende = neu;
                ein.lauf = b.lauf(scene);
                ein.bauteil = b;
            }
            None => {
                scene.begin(SCHRITT_MASS);
                return false;
            }
        }
        ein.anwenden(scene);
        true
    }

    /// Beginnt das Ziehen der gewählten Ketten am Druckpunkt.
    fn zug_starten(&mut self, stand: &Stand, scene: &mut Scene, cam: &Camera, w: f64, h: f64) {
        let Some(d) = self.druck.take() else {
            return;
        };
        let k = d.treffer.kette();
        if !self.gewaehlt.contains(&k) {
            self.gewaehlt.push(k);
        }
        let Some(z) = tr::finde(stand, k).map(|f| f.a.z) else {
            return;
        };
        let Some(start) = cam.plane_point(d.x, d.y, w, h, z) else {
            return;
        };
        let m = scene.model();
        let ketten = self
            .gewaehlt
            .iter()
            .filter_map(|&g| {
                let f = tr::finde(stand, g)?;
                Some((g, f.seite.n, m.massversatz(g.st, g.grad, g.art)))
            })
            .collect();
        scene.begin(SCHRITT_KETTE);
        self.zug = Some(Kettenzug { z, start, ketten });
        self.neu();
    }

    fn zug_event(&mut self, e: &Event, scene: &mut Scene, cam: &Camera, w: f64, h: f64) {
        match *e {
            Event::MouseMove { x, y, .. } => {
                let Some(z) = &self.zug else {
                    return;
                };
                let Some(p) = cam.plane_point(x, y, w, h, z.z) else {
                    return;
                };
                let v = p - z.start;
                let mut neu = false;
                for &(k, n, v0) in &z.ketten {
                    let soll = v0 + (v.dot(n) / RASTER).round() * RASTER;
                    if (scene.model().massversatz(k.st, k.grad, k.art) - soll).abs() > 0.5 {
                        neu |= scene.set_massversatz(k.st, k.grad, k.art, soll);
                    }
                }
                if neu {
                    self.neu();
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                self.zug = None;
                scene.commit();
                self.neu();
            }
            Event::Key {
                key: Key::Escape,
                down: true,
                ..
            } => {
                self.zug = None;
                scene.rollback();
                self.neu();
            }
            _ => {}
        }
    }

    /// Doppelklick: Versatz der gewählten Ketten (bzw. von `k`) zurück.
    fn zuruecksetzen(&mut self, scene: &mut Scene, k: KettenId) {
        let ks = if self.gewaehlt.contains(&k) {
            self.gewaehlt.clone()
        } else {
            vec![k]
        };
        scene.begin(SCHRITT_ZURUECK);
        for k in ks {
            scene.set_massversatz(k.st, k.grad, k.art, 0.0);
        }
        scene.commit();
        self.neu();
    }

    /// Kreuzungsrechteck (gestrichelt, am Bildschirm).
    pub fn helpers(&self, cam: &Camera, scale: f32, theme: &Theme) -> Vec<Helper> {
        let mut out = Vec::new();
        let (w, h) = self.groesse;
        if let (Some(von), Some(bis)) = (self.frei, self.rechteck.filter(|_| w > 0.0 && h > 0.0)) {
            let vor = cam.near() * 4.0;
            let at = |x: f64, y: f64| {
                let (o, d) = cam.ray(x, y, w, h);
                (o + d * vor).to_f32()
            };
            let ecken = [
                at(von.0, von.1),
                at(bis.0, von.1),
                at(bis.0, bis.1),
                at(von.0, bis.1),
            ];
            for i in 0..4 {
                out.push(Helper {
                    a: ecken[i],
                    b: ecken[(i + 1) % 4],
                    color: theme.interact.select,
                    width: scale,
                    dash: STRICH * scale,
                    pattern: SOLID,
                    occlude: false,
                    round: false,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::super::mass_treffer::tests::{haus, kamera, schrift, stand};
    use super::*;
    use crate::ui::ViewKind;
    use crate::wall_edit::WallEdit;
    use sk_model::bemassung::Kettenart;
    use sk_plan::bild::Zahlart;

    const W: f64 = 1600.0;
    const H: f64 = 1000.0;

    fn ev(e: &mut WallEdit, s: &mut Scene, c: &Camera, ereignis: Event) -> EditOutcome {
        let st = stand(s);
        e.masse.setze(st, s);
        e.handle(&ereignis, s, c, W, H, 1.0, true)
    }

    fn maus(x: f64, y: f64, mods: Modifiers) -> [Event; 3] {
        [
            Event::MouseMove { x, y, mods },
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            },
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                mods,
            },
        ]
    }

    fn klick(e: &mut WallEdit, s: &mut Scene, c: &Camera, (x, y): (f64, f64), mods: Modifiers) {
        for m in maus(x, y, mods) {
            ev(e, s, c, m);
        }
    }

    fn taste(e: &mut WallEdit, s: &mut Scene, c: &Camera, key: Key) -> EditOutcome {
        ev(
            e,
            s,
            c,
            Event::Key {
                key,
                down: true,
                repeat: false,
                mods: Modifiers::default(),
            },
        )
    }

    fn tippen(e: &mut WallEdit, s: &mut Scene, c: &Camera, text: &str) {
        for ch in text.chars() {
            taste(e, s, c, Key::Char(ch));
        }
    }

    /// Mitte der Zahl `i` der Kette `art` auf der Seite `grad` (Pixel) und
    /// ihr Wert.
    fn zahl(s: &Scene, c: &Camera, grad: i32, art: Kettenart, i: usize) -> ((f64, f64), f64) {
        let st = stand(s).unwrap();
        let (a, b) = (&st.geschosse[0].a, &st.geschosse[0].b);
        let si = a.seiten.iter().position(|x| x.grad == grad).unwrap();
        let ki = a.seiten[si]
            .ketten
            .iter()
            .position(|k| k.art == art)
            .unwrap();
        let z = b
            .zahlen
            .iter()
            .find(|z| {
                z.art
                    == Zahlart::Mass {
                        seite: si,
                        kette: ki,
                        i,
                    }
            })
            .unwrap();
        let m = z.rahmen.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.25;
        (c.project(m, W, H).unwrap(), z.wert)
    }

    /// Maß `i` der Kette (mm, aus den Bezügen).
    fn mass(s: &Scene, grad: i32, art: Kettenart, i: usize) -> f64 {
        let st = stand(s).unwrap();
        let k = KettenId {
            st: st.geschosse[0].a.storey,
            grad,
            art,
        };
        let (r0, r1) = tr::enden(&st, MassId { kette: k, i }).unwrap();
        r1.t - r0.t
    }

    /// Feld von der Innenwand zur Ostwand überschreiben: die Ostwand
    /// wandert, ein Schritt, Rückgängig stellt alles zurück.
    #[test]
    fn zahl_ueberschreiben_bewegt_das_rechte_ende() {
        let (mut s, ..) = haus();
        if stand(&s).is_none() {
            eprintln!("Liberation Sans fehlt, Test übersprungen");
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let gesamt = mass(&s, 270, Kettenart::Gesamt, 0);
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        let serial = s.step_serial();
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        assert!(e.is_dragging() && e.input().is_some());
        assert_eq!(e.input_label(), LABEL);
        assert_eq!(e.mass_hervor(&s).zahl.map(|z| z.0.i), Some(3));
        tippen(&mut e, &mut s, &c, "5");
        // Live: die Ostwand steht schon an der neuen Stelle
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5000.0).abs() < 0.5);
        assert!(e.dragging_run().is_some());
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!(!e.is_dragging());
        assert_eq!(s.step_serial(), serial + 1);
        assert_eq!(s.undo_label(), Some(SCHRITT_MASS));
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5000.0).abs() < 0.5);
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - (gesamt + 5000.0 - wert)).abs() < 1.0);
        s.undo();
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - gesamt).abs() < 0.5);
    }

    /// Pfeil am bewegten Ende: drei Striche in der Ziehfarbe, gleich nach
    /// dem Tippen am neuen Ende und von dort nach außen.
    #[test]
    fn pfeil_folgt_dem_bewegten_ende() {
        let (mut s, ..) = haus();
        let Some(f) = schrift() else {
            return;
        };
        let c = kamera();
        let theme = Theme::dark();
        let mut e = WallEdit::default();
        let (p, _) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "5");
        let h = e.mass_hervor(&s);
        let (m, ende) = h.ende.unwrap();
        assert_eq!((m.i, ende), (3, 1));
        let mut mk = crate::bemassung::Massketten::default();
        let blick = crate::bemassung::Blick {
            cam: &c,
            w: W,
            h: H,
        };
        let (linien, _) = mk.zeichnen(&s, ViewKind::Plan, &[], Some(&f), &blick, 1.0, &theme, &h);
        let pf: Vec<_> = linien
            .iter()
            .filter(|l| l.color == theme.interact.drag)
            .collect();
        assert_eq!(pf.len(), 3);
        let st = stand(&s).unwrap();
        let k = tr::finde(&st, m.kette).unwrap();
        let r = &k.kette.bezuege[m.i + 1];
        let (t, n) = (k.seite.t, k.seite.n);
        let q = t * r.t + n * k.kette.lage;
        let a = vec3(pf[0].a[0].into(), pf[0].a[1].into(), 0.0) - vec3(q.x, q.y, 0.0);
        let b = vec3(pf[0].b[0].into(), pf[0].b[1].into(), 0.0) - vec3(q.x, q.y, 0.0);
        // Hinter dem Symbol am neuen Ende, auf der Maßlinie, nach außen
        assert!(a.dot(t) > 0.0 && a.dot(t) < 2000.0, "{a:?}");
        assert!(a.dot(n).abs() < 1.0 && b.dot(t) > a.dot(t));
        taste(&mut e, &mut s, &c, Key::Escape);
        assert_eq!(e.mass_hervor(&s).ende, None);
    }

    /// Tab bewegt das andere Ende (die Innenwand), Esc bricht ohne Schritt ab.
    #[test]
    fn tab_nimmt_das_andere_ende_esc_bricht_ab() {
        let (mut s, _, iw, _) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let gesamt = mass(&s, 270, Kettenart::Gesamt, 0);
        let x0 = s.model().run(iw).unwrap().points[0].x;
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "5");
        taste(&mut e, &mut s, &c, Key::Tab);
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - gesamt).abs() < 0.5);
        let x1 = s.model().run(iw).unwrap().points[0].x;
        assert!((x1 - (x0 + wert - 5000.0)).abs() < 0.5, "{x0} {x1}");
        // Klick in einen freien Bereich übernimmt
        let frei = c.project(vec3(5000.0, 4000.0, 0.0), W, H).unwrap();
        klick(&mut e, &mut s, &c, frei, Modifiers::default());
        assert!(!e.is_dragging());
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5000.0).abs() < 0.5);
        // Neu öffnen, tippen, Esc: nichts bleibt, kein Schritt
        let serial = s.step_serial();
        let (p, _) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "3");
        taste(&mut e, &mut s, &c, Key::Escape);
        assert!(!e.is_dragging());
        assert_eq!(s.step_serial(), serial);
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5000.0).abs() < 0.5);
    }

    /// Nimmt das eine Ende den Wert nicht an, das andere nach Tab aber
    /// schon, gilt er wieder: kein alter Fehler, Enter übernimmt (Review
    /// W36). Unter 1 mm gilt nichts.
    #[test]
    fn fehler_gilt_nur_fuer_das_ende() {
        let (mut s, ..) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        // Von der Westecke zum Fenster: rechts wandert das Fenster
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Oeffnung, 0);
        assert!((wert - 1500.0).abs() < 0.5, "{wert}");
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "12");
        assert!(
            e.input().unwrap().error.is_some(),
            "Fenster jenseits der Ostwand"
        );
        taste(&mut e, &mut s, &c, Key::Tab);
        assert!(e.input().unwrap().error.is_none());
        assert!((mass(&s, 270, Kettenart::Oeffnung, 0) - 12_000.0).abs() < 0.5);
        let serial = s.step_serial();
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!(!e.is_dragging());
        assert_eq!(s.step_serial(), serial + 1);
        // Unter 1 mm: Fehler, nichts bewegt sich
        let (p, _) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        let vorher = mass(&s, 270, Kettenart::Gesamt, 0);
        tippen(&mut e, &mut s, &c, "0,0004");
        assert!(e.input().unwrap().error.is_some());
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - vorher).abs() < 0.5);
        taste(&mut e, &mut s, &c, Key::Escape);
    }

    /// Öffnungsbreite: zwei Kanten derselben Öffnung, das rechte Ende
    /// wandert, die linke Kante bleibt.
    #[test]
    fn oeffnungsbreite_ueberschreiben() {
        let (mut s, _, _, fenster) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Oeffnung, 1);
        assert!((wert - 1010.0).abs() < 0.5);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "1,26");
        taste(&mut e, &mut s, &c, Key::Enter);
        let o = s.model().opening(fenster).unwrap();
        let xs: Vec<f64> = o.path.iter().map(|p| p.x).collect();
        assert!(xs.iter().any(|x| (x - 1500.0).abs() < 0.5), "{xs:?}");
        assert!(xs.iter().any(|x| (x - 2760.0).abs() < 0.5), "{xs:?}");
        // Ein Wert, mit dem die Öffnung nicht stehen kann, gilt nicht
        let (p, _) = zahl(&s, &c, 270, Kettenart::Oeffnung, 1);
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "0,05");
        assert!(e.input().unwrap().error.is_some());
        let serial = s.step_serial();
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!(e.is_dragging(), "Enter mit Fehler schließt nicht");
        taste(&mut e, &mut s, &c, Key::Escape);
        assert_eq!(s.step_serial(), serial);
    }

    /// Das Feld steht mit dem Maß in m vorbelegt und markiert (Bedienbarkeit
    /// 40.3); Enter ohne Tippen ändert nichts. Zwischen Fenster und Ecke
    /// wandert das Fenster, nicht die Hauswand (40.4).
    #[test]
    fn oeffnungskette_vorbelegt_und_fenster_wandert() {
        let (mut s, _, _, fenster) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let gesamt = mass(&s, 270, Kettenart::Gesamt, 0);
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Oeffnung, 2);
        assert!((wert - 7490.0).abs() < 0.5, "{wert}");
        let serial = s.step_serial();
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        let i = e.input().unwrap();
        assert_eq!((i.fields[0].as_str(), i.vorbelegt()), ("7,49", true));
        assert_eq!(e.mass_hervor(&s).ende.map(|(_, ende)| ende), Some(0));
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!(!e.is_dragging());
        assert_eq!(s.step_serial(), serial, "unverändert: kein Schritt");
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        tippen(&mut e, &mut s, &c, "7");
        taste(&mut e, &mut s, &c, Key::Enter);
        assert_eq!(s.step_serial(), serial + 1);
        let xs: Vec<f64> = s
            .model()
            .opening(fenster)
            .unwrap()
            .path
            .iter()
            .map(|p| p.x)
            .collect();
        assert!(xs.iter().any(|x| (x - 1990.0).abs() < 0.5), "{xs:?}");
        assert!(xs.iter().any(|x| (x - 3000.0).abs() < 0.5), "{xs:?}");
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - gesamt).abs() < 0.5);
    }

    /// F1 über einer Kette und bei offener Maßzahl: Thema „Bemaßung“
    /// (Bedienbarkeit 40.2); im Freien wieder kein eigenes Thema.
    #[test]
    fn f1_ueber_der_kette_zeigt_bemassung() {
        let (mut s, ..) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let (p, _) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        let m = Modifiers::default();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: p.0,
                y: p.1,
                mods: m,
            },
        );
        assert!(e.eigenes_thema());
        assert_eq!(e.thema(), crate::help::Topic::Bemassung);
        klick(&mut e, &mut s, &c, p, m);
        assert!(e.input().is_some());
        assert_eq!(e.thema(), crate::help::Topic::Bemassung);
        taste(&mut e, &mut s, &c, Key::Escape);
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: 5.0,
                y: 5.0,
                mods: m,
            },
        );
        assert!(!e.eigenes_thema());
    }

    /// Bei „Tragende Schicht“ misst die Kette das Loch im Mauerwerk, 3 cm
    /// je Seite größer als das gezeichnete Fenster im WDVS (Bedienbarkeit
    /// 40.1); überschrieben wird dieses Maß.
    #[test]
    fn tragende_schicht_ueberschreibt_das_loch_im_mauerwerk() {
        let (mut s, _, _, fenster) = haus();
        if stand(&s).is_none() {
            return;
        }
        let mut stil = s.model().massstil().clone();
        stil.bezug = sk_model::bemassung::Aussenbezug::Rohbau;
        s.begin_settings();
        assert!(s.edit_attr(|m| m.set_massstil(stil)));
        s.commit();
        let c = kamera();
        let mut e = WallEdit::default();
        let (p, wert) = zahl(&s, &c, 270, Kettenart::Oeffnung, 1);
        assert!((wert - 1070.0).abs() < 0.5, "{wert}");
        klick(&mut e, &mut s, &c, p, Modifiers::default());
        assert_eq!(e.input().unwrap().fields[0], "1,07");
        tippen(&mut e, &mut s, &c, "1,32");
        taste(&mut e, &mut s, &c, Key::Enter);
        let xs: Vec<f64> = s
            .model()
            .opening(fenster)
            .unwrap()
            .path
            .iter()
            .map(|p| p.x)
            .collect();
        assert!(xs.iter().any(|x| (x - 1500.0).abs() < 0.5), "{xs:?}");
        assert!(xs.iter().any(|x| (x - 2760.0).abs() < 0.5), "{xs:?}");
    }

    /// Maßlinie greifen und nach außen ziehen: Versatz in 10-mm-Schritten,
    /// ein Schritt; Doppelklick setzt zurück.
    #[test]
    fn kette_ziehen_und_zuruecksetzen() {
        let (mut s, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let c = kamera();
        let mut e = WallEdit::default();
        let k = KettenId {
            st: st.geschosse[0].a.storey,
            grad: 270,
            art: Kettenart::Gesamt,
        };
        let f = tr::finde(&st, k).unwrap();
        let l = st.geschosse[0]
            .b
            .linien
            .iter()
            .find(|l| {
                (l.a - l.b).dot(f.seite.t).abs() > 1.0
                    && (l.a.dot(f.seite.n) - f.kette.lage).abs() < 1.0
            })
            .copied()
            .unwrap();
        let p0 = l.a + (l.b - l.a) * 0.03;
        let a = c.project(p0, W, H).unwrap();
        let b = c.project(p0 + f.seite.n * 503.0, W, H).unwrap();
        let m = Modifiers::default();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: a.0,
                y: a.1,
                mods: m,
            },
        );
        assert_eq!(e.masse.ueber, Some(Treffer::Linie(k)));
        assert_eq!(e.masse.cursor(), Some(Cursor::Hand));
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseDown {
                button: MouseButton::Left,
                x: a.0,
                y: a.1,
                mods: m,
            },
        );
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: b.0,
                y: b.1,
                mods: m,
            },
        );
        assert_eq!(s.model().massversatz(k.st, 270, Kettenart::Gesamt), 500.0);
        let serial = s.step_serial();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseUp {
                button: MouseButton::Left,
                x: b.0,
                y: b.1,
                mods: m,
            },
        );
        assert_eq!(s.step_serial(), serial + 1);
        assert_eq!(s.undo_label(), Some(SCHRITT_KETTE));
        assert_eq!(e.masse.gewaehlt(), [k]);
        // Doppelklick auf die verschobene Linie
        let p1 = c.project(p0 + f.seite.n * 500.0, W, H).unwrap();
        klick(&mut e, &mut s, &c, p1, m);
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseDown {
                button: MouseButton::Left,
                x: p1.0,
                y: p1.1,
                mods: m,
            },
        );
        assert_eq!(s.model().massversatz(k.st, 270, Kettenart::Gesamt), 0.0);
        assert_eq!(s.undo_label(), Some(SCHRITT_ZURUECK));
        // Esc hebt die Wahl auf
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseUp {
                button: MouseButton::Left,
                x: p1.0,
                y: p1.1,
                mods: m,
            },
        );
        taste(&mut e, &mut s, &c, Key::Escape);
        assert!(e.masse.gewaehlt().is_empty());
    }

    /// In 3D liegen die Ketten flach auf der Geschossebene: Zahl
    /// überschreiben und Kette ziehen gehen dort wie im Grundriss.
    #[test]
    fn auch_in_3d() {
        let (mut s, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let mitte = vec3(5000.0, 4000.0, 0.0);
        let auge = mitte + vec3(0.55, -1.0, 0.9).normalized() * 30_000.0;
        let c = Camera::looking_at(auge, mitte, 45.0);
        let mut e = WallEdit::default();
        let m = Modifiers::default();
        // Zahl überschreiben
        let (p, _) = zahl(&s, &c, 270, Kettenart::Wand, 3);
        klick(&mut e, &mut s, &c, p, m);
        assert_eq!(e.mass_hervor(&s).zahl.map(|z| z.0.i), Some(3));
        tippen(&mut e, &mut s, &c, "5");
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!(!e.is_dragging());
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5000.0).abs() < 0.5);
        // Gesamtmaß greifen und 503 mm nach außen ziehen: 500 mm Versatz
        let k = KettenId {
            st: st.geschosse[0].a.storey,
            grad: 270,
            art: Kettenart::Gesamt,
        };
        let st = stand(&s).unwrap();
        let f = tr::finde(&st, k).unwrap();
        let l = st.geschosse[0]
            .b
            .linien
            .iter()
            .find(|l| {
                (l.a - l.b).dot(f.seite.t).abs() > 1.0
                    && (l.a.dot(f.seite.n) - f.kette.lage).abs() < 1.0
            })
            .copied()
            .unwrap();
        let p0 = l.a + (l.b - l.a) * 0.5;
        let a = c.project(p0, W, H).unwrap();
        let b = c.project(p0 + f.seite.n * 503.0, W, H).unwrap();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: a.0,
                y: a.1,
                mods: m,
            },
        );
        assert_eq!(e.masse.ueber, Some(Treffer::Linie(k)));
        let unten = Event::MouseDown {
            button: MouseButton::Left,
            x: a.0,
            y: a.1,
            mods: m,
        };
        ev(&mut e, &mut s, &c, unten);
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: b.0,
                y: b.1,
                mods: m,
            },
        );
        let oben = Event::MouseUp {
            button: MouseButton::Left,
            x: b.0,
            y: b.1,
            mods: m,
        };
        ev(&mut e, &mut s, &c, oben);
        assert_eq!(s.model().massversatz(k.st, 270, Kettenart::Gesamt), 500.0);
        assert_eq!(s.undo_label(), Some(SCHRITT_KETTE));
    }

    /// Strg nimmt dazu, Umschalt den Block, das Rechteck von rechts nach
    /// links jede gekreuzte Kette; von links nach rechts nichts.
    #[test]
    fn mehrfachwahl_und_kreuzungsrechteck() {
        let (mut s, ..) = haus();
        let Some(st) = stand(&s) else {
            return;
        };
        let c = kamera();
        let mut e = WallEdit::default();
        let sid = st.geschosse[0].a.storey;
        let id = |grad, art| KettenId { st: sid, grad, art };
        let strg = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let umsch = Modifiers {
            shift: true,
            ..Default::default()
        };
        let (p, _) = zahl(&s, &c, 270, Kettenart::Gesamt, 0);
        klick(&mut e, &mut s, &c, p, strg);
        let (q, _) = zahl(&s, &c, 90, Kettenart::Gesamt, 0);
        klick(&mut e, &mut s, &c, q, strg);
        assert_eq!(
            e.masse.gewaehlt(),
            [id(270, Kettenart::Gesamt), id(90, Kettenart::Gesamt)]
        );
        assert!(!e.is_dragging(), "Strg+Klick öffnet keine Eingabe");
        klick(&mut e, &mut s, &c, q, strg);
        assert_eq!(e.masse.gewaehlt(), [id(270, Kettenart::Gesamt)]);
        klick(&mut e, &mut s, &c, p, umsch);
        assert_eq!(
            e.masse.gewaehlt().len(),
            3,
            "Block Süd: Öffnung, Wand, Gesamt"
        );
        // Klick ins Freie hebt die Wahl auf
        let frei = c.project(vec3(5000.0, 4000.0, 0.0), W, H).unwrap();
        klick(&mut e, &mut s, &c, frei, Modifiers::default());
        assert!(e.masse.gewaehlt().is_empty());
        // Rechteck von rechts nach links über den Südblock
        let m = Modifiers::default();
        let r = c.project(vec3(2300.0, -1000.0, 0.0), W, H).unwrap();
        let l = c.project(vec3(2000.0, -6000.0, 0.0), W, H).unwrap();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: r.0,
                y: r.1,
                mods: m,
            },
        );
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseDown {
                button: MouseButton::Left,
                x: r.0,
                y: r.1,
                mods: m,
            },
        );
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: l.0,
                y: l.1,
                mods: m,
            },
        );
        assert!(e.is_dragging());
        assert_eq!(e.helpers(&s, &c, 1.0, false, &Theme::dark()).len(), 4);
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseUp {
                button: MouseButton::Left,
                x: l.0,
                y: l.1,
                mods: m,
            },
        );
        assert_eq!(e.masse.gewaehlt().len(), 3);
        assert!(e.masse.gewaehlt().iter().all(|k| k.grad == 270));
        // Von links nach rechts: keine Wahl
        klick(&mut e, &mut s, &c, frei, m);
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseDown {
                button: MouseButton::Left,
                x: l.0,
                y: l.1,
                mods: m,
            },
        );
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: r.0,
                y: r.1,
                mods: m,
            },
        );
        assert!(!e.is_dragging());
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseUp {
                button: MouseButton::Left,
                x: r.0,
                y: r.1,
                mods: m,
            },
        );
        assert!(e.masse.gewaehlt().is_empty());
    }

    /// Ostwand am Band nach außen ziehen und „6“ tippen: das Maß hinter ihr
    /// (Innenwand bis Ostwand) wird 6,00 m; es leuchtet, sobald die Richtung
    /// feststeht, und bleibt nach dem Loslassen offen.
    #[test]
    fn band_ziehen_mit_kettenbezug() {
        let (mut s, ..) = haus();
        if stand(&s).is_none() {
            return;
        }
        let c = kamera();
        let mut e = WallEdit::default();
        let m = Modifiers::default();
        let gesamt = mass(&s, 270, Kettenart::Gesamt, 0);
        let z = s
            .model()
            .storeys()
            .iter()
            .next()
            .map(|(_, st)| st.elevation);
        e.stand(None, z, None);
        let (x, y) = c.project(vec3(10_000.0, 2000.0, 0.0), W, H).unwrap();
        ev(&mut e, &mut s, &c, Event::MouseMove { x, y, mods: m });
        assert!(e.hover.is_some(), "Band der Ostwand");
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: m,
            },
        );
        assert!(e.drag.as_ref().is_some_and(|d| d.mass.is_some()));
        assert_eq!(e.mass_hervor(&s).zahl, None, "noch keine Richtung");
        let (x2, y2) = c.project(vec3(10_400.0, 2000.0, 0.0), W, H).unwrap();
        ev(
            &mut e,
            &mut s,
            &c,
            Event::MouseMove {
                x: x2,
                y: y2,
                mods: m,
            },
        );
        let h = e.mass_hervor(&s).zahl.unwrap();
        assert_eq!(
            (h.0.kette.grad, h.0.kette.art, h.0.i),
            (270, Kettenart::Wand, 3)
        );
        tippen(&mut e, &mut s, &c, "6");
        assert_eq!(e.input_label(), LABEL);
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 6000.0).abs() < 0.5);
        // Nach dem Loslassen bleibt das Maß offen (keine Ausblendung)
        let (text, offen) = e.pill(&s).unwrap();
        assert!(offen && text.starts_with(LABEL), "{text}");
        // Nachkorrektur: der neue Wert ersetzt den Schritt
        tippen(&mut e, &mut s, &c, "5,5");
        taste(&mut e, &mut s, &c, Key::Enter);
        assert!((mass(&s, 270, Kettenart::Wand, 3) - 5500.0).abs() < 0.5);
        s.undo();
        assert!((mass(&s, 270, Kettenart::Gesamt, 0) - gesamt).abs() < 0.5);
    }
}

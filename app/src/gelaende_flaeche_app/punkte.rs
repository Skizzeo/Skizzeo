//! Höhenpunkte im Grundriss (I4, gelaende/analyse-interaktives-gelaende.md
//! §4.3, §4.3a): Bei offener Leiste „Gelände“ (§4.0 Regel 1) greift die
//! Maus die Punkte des Geländes. Ein Klick wählt einen Punkt, Strg+Klick
//! nimmt weitere dazu oder heraus. Einen Punkt ziehen verschiebt ihn und
//! die übrigen gewählten in der Lage, mit Fang an Haus, Grenze und Flächen
//! (Alt ohne, §5 J10). Am gewählten Punkt steht ein Höhengriff: nach oben
//! ziehen hebt, nach unten senkt, ein Bildpunkt 1 cm, mit Umschalt 10 cm.
//! Strg beim Ziehen oder Loslassen macht eine Kopie (immer ein eigener
//! Punkt), Strg+D kopiert die gewählten um 1,00 m versetzt. Entf löscht die
//! gewählten. Ein Doppelklick aufs Gelände setzt einen Punkt in der Höhe,
//! die das Gelände dort hat; so springt beim Setzen nichts.
//!
//! Gezogen wird ohne das Modell zu ändern; Loslassen ist ein Schritt. Esc
//! bricht den Zug ab, ohne Zug hebt es die Wahl auf.

use super::fang::{self, Ziele};
use super::STEP;
use crate::camera::Camera;
use crate::meldung::Meldung;
use crate::App;
use sk_math::vec3;
use sk_model::TerrainSpot;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_render::Helper;
use sk_ui::theme::Theme;
use std::time::{Duration, Instant};

/// So nah (Bildpunkte) greift die Maus einen Punkt oder den Höhengriff.
const GREIF_PX: f64 = 7.0;
/// Größe der Punkte (Bildpunkte bei 96 dpi).
const PUNKT_PX: f32 = 7.0;
/// Der Höhengriff sitzt so weit rechts über dem Punkt (Bildpunkte).
const GRIFF_DX: f64 = 18.0;
const GRIFF_DY: f64 = -18.0;
/// Länge des Pfeils am Höhengriff (Bildpunkte).
const PFEIL_PX: f64 = 14.0;
/// Höhe je Bildpunkt (mm), mit Umschalt zehnmal so viel.
const MM_JE_PX: f64 = 10.0;
/// Doppelklick: so schnell und so nah.
const DOPPEL: Duration = Duration::from_millis(450);
const DOPPEL_PX: f64 = 5.0;
/// Strg+D versetzt um so viel (mm).
const VERSATZ: f64 = 1000.0;

/// Statuszeile (§4.0 Regel 2).
pub const PUNKT: &str =
    "Höhenpunkt {}: {}. Klicken wählt, ziehen verschiebt (Strg kopiert, Alt ohne Fang).";
pub const HOEHE: &str = "Höhe ziehen: nach oben hebt, 1 Bildpunkt 1 cm, mit Umschalt 10 cm.";
/// Messpunkte des Vermessers bleiben, bis das Schloss kommt (I5, §5 J2).
pub const MESSPUNKT: &str =
    "Punkte des Vermessers bleiben, wie gemessen. Mit Strg ziehen setzt eine \
     eigene Kopie, Entf nimmt sie aus dem Gelände.";
pub const DREI: &str = "Das Gelände behält mindestens drei Punkte.";
pub const BELEGT: &str = "Dort liegt schon ein Höhenpunkt; nichts geändert.";
/// Schritte im Rückgängig (§4.0 Regel 5).
pub const LABEL_GESETZT: &str = "Höhenpunkt gesetzt";
pub const LABEL_LAGE: &str = "Höhenpunkte verschoben";
pub const LABEL_HOEHE: &str = "Höhe geändert";
pub const LABEL_KOPIE: &str = "Höhenpunkte kopiert";
pub const LABEL_GELOESCHT: &str = "Höhenpunkte gelöscht";

/// Was die Maus greift.
#[derive(Clone, Debug, PartialEq)]
pub enum Ziel {
    /// Ein Punkt (Nummer).
    Punkt(String),
    /// Der Höhengriff der Wahl.
    Hoehe,
}

/// Ein Zug: in der Lage oder in der Höhe.
#[derive(Clone, Debug)]
struct Zug {
    hoehe: bool,
    /// Bildpunkt und Bodenpunkt beim Greifen.
    px: (f64, f64),
    start: [f64; 2],
    /// Der gegriffene Punkt (Lage) für den Fang.
    griff: [f64; 2],
    /// Die gezogenen Punkte: Index, Lage und Höhe beim Greifen.
    alt: Vec<(usize, [f64; 3])>,
    /// Lage und Höhe jetzt.
    neu: Vec<[f64; 3]>,
    ziele: Ziele,
    kopie: bool,
    /// Wurde wirklich gezogen (mehr als ein paar Bildpunkte)?
    bewegt: bool,
    /// Unter den gezogenen ist ein Punkt des Vermessers: nur kopieren.
    fest: bool,
    /// Mit Strg auf einen schon gewählten Punkt geklickt: ohne Zug geht
    /// er aus der Wahl.
    raus: bool,
    nr: String,
}

/// Höhenpunkte im Grundriss.
#[derive(Clone, Debug, Default)]
pub struct Punkte {
    /// Die gewählten Punkte (Nummern).
    pub(super) gewaehlt: Vec<String>,
    hover: Option<Ziel>,
    zug: Option<Zug>,
    /// Der letzte Klick ins Leere, für den Doppelklick.
    klick: Option<(Instant, f64, f64)>,
    /// Punkte zum Stand des Modells.
    merker: Option<(u64, Spots)>,
}

/// Die Punkte mit Nummer, geteilt bis zur nächsten Änderung.
type Spots = std::sync::Arc<Vec<(String, TerrainSpot)>>;

impl Punkte {
    pub fn busy(&self) -> bool {
        self.hover.is_some() || self.zug.is_some()
    }

    pub fn zieht(&self) -> bool {
        self.zug.is_some()
    }
}

/// Der Punkt unter dem Bildpunkt (der nächste bis [`GREIF_PX`]).
fn treffen<'a>(
    spots: &'a [(String, TerrainSpot)],
    cam: &Camera,
    z: f64,
    (x, y): (f64, f64),
    w: f64,
    h: f64,
    scale: f64,
) -> Option<&'a (String, TerrainSpot)> {
    let r = GREIF_PX * scale;
    let mut best: Option<(f64, &(String, TerrainSpot))> = None;
    for s in spots {
        let Some(q) = cam.project(vec3(s.1.p[0], s.1.p[1], z), w, h) else {
            continue;
        };
        let d = (q.0 - x).hypot(q.1 - y);
        if d <= r && best.is_none_or(|b| d < b.0) {
            best = Some((d, s));
        }
    }
    best.map(|b| b.1)
}

/// Lage beim Ziehen: der gegriffene Punkt am Fangziel (Alt ohne), sonst im
/// Raster; alle gewählten um dasselbe.
fn lage(z: &Zug, p: [f64; 2], tol: f64, mods: Modifiers) -> [f64; 2] {
    let ziel = [
        z.griff[0] + p[0] - z.start[0],
        z.griff[1] + p[1] - z.start[1],
    ];
    let q = (!mods.alt)
        .then(|| z.ziele.fangen(ziel, tol).map(|f| f.0))
        .flatten()
        .unwrap_or_else(|| ziel.map(|v| (v / STEP).round() * STEP + 0.0));
    [q[0] - z.griff[0], q[1] - z.griff[1]]
}

/// Höhe beim Ziehen am Griff: Bildpunkte nach oben, auf cm (mit Umschalt
/// dm) gerundet.
fn hub(dy_px: f64, scale: f64, shift: bool) -> f64 {
    let je = if shift { MM_JE_PX * 10.0 } else { MM_JE_PX };
    let d = -dy_px / scale.max(1e-9) * je;
    (d / je).round() * je + 0.0
}

/// Meter mit Komma und zwei Stellen, mit Vorzeichen.
fn m_vz(mm: f64) -> String {
    let v = mm / 1000.0;
    let t = super::de(v.abs(), 2);
    if v < -5e-4 {
        format!("−{t}")
    } else {
        format!("+{t}")
    }
}

impl App {
    /// Sind die Höhenpunkte greifbar? Wie die Griffe der Grenze.
    fn punkte_an(&self) -> bool {
        self.grenz_griffe_an()
    }

    /// Die Punkte im Gelände mit Nummer: gemerkt, wenn der Stand des
    /// Modells passt, sonst neu.
    pub(super) fn spots(&self) -> Spots {
        let m = self.scene.model();
        if let Some((r, s)) = &self.platte.punkte.merker {
            if *r == m.revision() {
                return s.clone();
            }
        }
        let nr = |i: usize| {
            m.terrain()
                .and_then(|t| t.points.get(i))
                .map_or_else(String::new, |p| p.nr.clone())
        };
        std::sync::Arc::new(
            m.terrain_spots()
                .into_iter()
                .map(|s| (nr(s.i), s))
                .collect(),
        )
    }

    /// Die Punkte zum jetzigen Stand merken.
    fn spots_merken(&mut self) {
        let rev = self.scene.model().revision();
        if self
            .platte
            .punkte
            .merker
            .as_ref()
            .is_none_or(|m| m.0 != rev)
        {
            let s = self.spots();
            self.platte.punkte.merker = Some((rev, s));
        }
    }

    /// Höhe für die Zeile: über NHN, wenn das Gelände vermessen ist, dazu
    /// über ±0,00.
    fn punkt_hoehe(&self, z: f64) -> String {
        let m = self.scene.model();
        match (m.terrain_nhn(), m.terrain().is_some_and(|t| !t.assumed())) {
            (Some(h0), true) => {
                format!("{} m ü. NHN ({} m)", super::de(h0 + z / 1000.0, 2), m_vz(z))
            }
            _ => format!("{} m über ±0,00", m_vz(z)),
        }
    }

    /// Bildpunkt des Höhengriffs am ersten gewählten Punkt.
    fn hoehen_griff(&self, vw: f64, vh: f64, sc: f64) -> Option<(f64, f64)> {
        let erster = self.platte.punkte.gewaehlt.first()?.clone();
        let spots = self.spots();
        let s = spots.iter().find(|s| s.0 == erster)?;
        let z = self.scene.model().terrain_z();
        let q = self.cam.project(vec3(s.1.p[0], s.1.p[1], z), vw, vh)?;
        Some((q.0 + GRIFF_DX * sc, q.1 + GRIFF_DY * sc))
    }

    /// Ereignis an die Höhenpunkte; `true`, wenn sie es nehmen.
    pub(crate) fn punkte_handle(&mut self, ev: &Event, vw: f64, vh: f64, sc: f64) -> bool {
        if !self.punkte_an() || self.platte.griffe.zieht() {
            if self.platte.punkte.zug.take().is_some() || self.platte.punkte.hover.take().is_some()
            {
                self.punkte_zeile(None);
                self.redraw = true;
            }
            return false;
        }
        self.spots_merken();
        match *ev {
            Event::MouseMove { x, y, mods } => {
                if self.platte.punkte.zug.is_some() {
                    self.punkte_ziehen(x, y, vw, vh, sc, mods);
                    return true;
                }
                let ziel = self.punkte_ziel(x, y, vw, vh, sc);
                if ziel != self.platte.punkte.hover {
                    self.platte.punkte.hover = ziel.clone();
                    self.punkte_zeile(ziel.as_ref());
                    self.redraw = true;
                }
                ziel.is_some()
            }
            Event::MouseLeave => {
                if self.platte.punkte.zug.is_none() && self.platte.punkte.hover.take().is_some() {
                    self.punkte_zeile(None);
                    self.redraw = true;
                }
                false
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods,
            } => {
                let ziel = self.punkte_ziel(x, y, vw, vh, sc);
                match ziel {
                    Some(z) => {
                        self.platte.punkte.klick = None;
                        self.punkte_greifen(z, x, y, vw, vh, mods)
                    }
                    None => self.punkte_ins_leere(x, y, vw, vh),
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                mods,
                ..
            } => {
                let Some(z) = self.platte.punkte.zug.take() else {
                    return false;
                };
                self.punkte_loslassen(z, mods);
                true
            }
            _ => false,
        }
    }

    /// Was unter dem Bildpunkt liegt: der Höhengriff vor den Punkten.
    fn punkte_ziel(&self, x: f64, y: f64, vw: f64, vh: f64, sc: f64) -> Option<Ziel> {
        if let Some(g) = self.hoehen_griff(vw, vh, sc) {
            if (g.0 - x).hypot(g.1 - y) <= GREIF_PX * sc * 1.5 {
                return Some(Ziel::Hoehe);
            }
        }
        let spots = self.spots();
        let z = self.scene.model().terrain_z();
        treffen(&spots, &self.cam, z, (x, y), vw, vh, sc).map(|s| Ziel::Punkt(s.0.clone()))
    }

    /// Klick neben die Punkte: die Wahl geht; der zweite schnell danach
    /// setzt einen Punkt. `true`, wenn er gesetzt ist.
    fn punkte_ins_leere(&mut self, x: f64, y: f64, vw: f64, vh: f64) -> bool {
        let jetzt = Instant::now();
        let doppel = self.platte.punkte.klick.is_some_and(|(t, px, py)| {
            jetzt.duration_since(t) <= DOPPEL && (px - x).hypot(py - y) <= DOPPEL_PX
        });
        if !self.platte.punkte.gewaehlt.is_empty() {
            self.platte.punkte.gewaehlt.clear();
            self.redraw = true;
        }
        if !doppel {
            self.platte.punkte.klick = Some((jetzt, x, y));
            return false;
        }
        self.platte.punkte.klick = None;
        let Some(g) = self.cam.ground_point(x, y, vw, vh) else {
            return false;
        };
        let r = |v: f64| (v / STEP).round() * STEP + 0.0;
        let (px, py) = (r(g.x), r(g.y));
        let m = self.scene.model();
        // unter dem Haus ist kein Gelände
        let im_haus = m
            .terrain_patches(0.0)
            .iter()
            .any(|(b, _)| (b[0]..=b[2]).contains(&px) && (b[1]..=b[3]).contains(&py));
        if im_haus {
            return false;
        }
        let z = (m.terrain_z_at(m.level_building(), px, py) / 10.0).round() * 10.0;
        let mut neu = Vec::new();
        let ok = self.scene.edit_model(LABEL_GESETZT, |m| {
            neu = m.add_terrain_points(&[[px, py, z]]);
            !neu.is_empty()
        });
        if ok {
            let nr = self
                .scene
                .model()
                .terrain()
                .and_then(|t| t.points.get(neu[0]))
                .map(|p| p.nr.clone());
            self.platte.punkte.gewaehlt = nr.into_iter().collect();
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
        }
        true
    }

    /// Greifen: am Punkt wählen (Strg: dazu) und den Zug vorbereiten; am
    /// Höhengriff die Höhe der Wahl.
    fn punkte_greifen(
        &mut self,
        ziel: Ziel,
        x: f64,
        y: f64,
        vw: f64,
        vh: f64,
        mods: Modifiers,
    ) -> bool {
        let Some(start) = self.cam.ground_point(x, y, vw, vh).map(|p| [p.x, p.y]) else {
            return false;
        };
        let (hoehe, nr, raus) = match ziel {
            Ziel::Hoehe => (true, self.platte.punkte.gewaehlt[0].clone(), false),
            Ziel::Punkt(nr) => {
                let w = &mut self.platte.punkte.gewaehlt;
                let drin = w.contains(&nr);
                if !drin {
                    if !mods.ctrl {
                        w.clear();
                    }
                    w.push(nr.clone());
                }
                (false, nr, drin && mods.ctrl)
            }
        };
        let spots = self.spots();
        let w = &self.platte.punkte.gewaehlt;
        let alt: Vec<(usize, [f64; 3])> = spots
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| (s.1.i, s.1.p))
            .collect();
        let griff = spots
            .iter()
            .find(|s| s.0 == nr)
            .map_or(start, |s| [s.1.p[0], s.1.p[1]]);
        let neu = alt.iter().map(|a| a.1).collect();
        let fest = spots.iter().any(|s| w.contains(&s.0) && !s.1.hand);
        self.platte.punkte.zug = Some(Zug {
            hoehe,
            px: (x, y),
            start,
            griff,
            alt,
            neu,
            ziele: fang::ziele(&self.scene),
            kopie: mods.ctrl,
            bewegt: false,
            fest,
            raus,
            nr,
        });
        self.redraw = true;
        true
    }

    /// Ziehen: nur die Vorschau, das Modell bleibt bis zum Loslassen.
    fn punkte_ziehen(&mut self, x: f64, y: f64, vw: f64, vh: f64, sc: f64, mods: Modifiers) {
        let tol = {
            let a = self.cam.ground_point(x, y, vw, vh);
            let b = self.cam.ground_point(x + fang::FANG_PX * sc, y, vw, vh);
            a.zip(b).map_or(0.0, |(a, b)| (b.x - a.x).hypot(b.y - a.y))
        };
        let boden = self.cam.ground_point(x, y, vw, vh).map(|p| [p.x, p.y]);
        let Some(z) = self.platte.punkte.zug.as_mut() else {
            return;
        };
        if !z.bewegt && (x - z.px.0).hypot(y - z.px.1) < 3.0 * sc {
            return;
        }
        z.kopie |= mods.ctrl;
        if z.fest && !z.kopie {
            self.status(Meldung::satz(MESSPUNKT), crate::NOTICE_TIME);
            return;
        }
        let Some(z) = self.platte.punkte.zug.as_mut() else {
            return;
        };
        z.bewegt = true;
        let text = if z.hoehe {
            let d = hub(y - z.px.1, sc, mods.shift);
            z.neu = z.alt.iter().map(|a| [a.1[0], a.1[1], a.1[2] + d]).collect();
            let erster = z.neu[0][2];
            (erster, format!("Höhe {}", m_vz(d)))
        } else {
            let Some(p) = boden else {
                return;
            };
            let d = lage(z, p, tol, mods);
            z.neu = z
                .alt
                .iter()
                .map(|a| [a.1[0] + d[0], a.1[1] + d[1], a.1[2]])
                .collect();
            let l = d[0].hypot(d[1]) / 1000.0;
            (z.neu[0][2], format!("verschoben um {} m", super::de(l, 2)))
        };
        let kopie = if z.kopie { " · Kopie" } else { "" };
        let nr = z.nr.clone();
        let h = self.punkt_hoehe(text.0);
        let zeile = format!("Höhenpunkt {nr}: {h} · {}{kopie}", text.1);
        self.status(Meldung::mit("{}", &[&zeile]), Duration::from_secs(3600));
        self.redraw = true;
    }

    /// Loslassen: ein Schritt (verschieben oder kopieren); ein Klick ohne
    /// Zug mit Strg nimmt den Punkt aus der Wahl, wenn er schon drin war.
    fn punkte_loslassen(&mut self, z: Zug, mods: Modifiers) {
        if !z.bewegt {
            if z.raus {
                self.platte.punkte.gewaehlt.retain(|n| *n != z.nr);
            }
            self.punkte_zeile(self.platte.punkte.hover.clone().as_ref());
            self.redraw = true;
            return;
        }
        let kopie = z.kopie || mods.ctrl;
        let ok = if kopie {
            let mut neu = Vec::new();
            let ok = self.scene.edit_model(LABEL_KOPIE, |m| {
                neu = m.add_terrain_points(&z.neu);
                !neu.is_empty()
            });
            if ok {
                self.punkte_waehlen(&neu);
            }
            ok
        } else {
            let paare: Vec<(usize, [f64; 3])> = z
                .alt
                .iter()
                .map(|a| a.0)
                .zip(z.neu.iter().copied())
                .collect();
            let label = if z.hoehe { LABEL_HOEHE } else { LABEL_LAGE };
            self.scene
                .edit_model(label, |m| m.move_terrain_points(&paare))
        };
        if ok {
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
            self.redraw = true;
        }
    }

    /// Die Punkte mit diesen Indizes wählen (nach Kopie oder Setzen).
    fn punkte_waehlen(&mut self, idx: &[usize]) {
        let m = self.scene.model();
        let Some(t) = m.terrain() else {
            return;
        };
        self.platte.punkte.gewaehlt = idx
            .iter()
            .filter_map(|&i| t.points.get(i).map(|p| p.nr.clone()))
            .collect();
    }

    /// Nach einem Schritt: Bild, Mengen und Leiste nachziehen.
    fn punkte_fertig(&mut self) {
        self.upload_model();
        self.quantity.dirty = true;
        self.platte.leiste.neu = true;
        self.gelaende_sync();
        self.punkte_zeile(None);
        self.redraw = true;
    }

    /// Esc: den Zug abbrechen, sonst die Wahl aufheben. `true`, wenn Esc
    /// den Punkten galt.
    pub(crate) fn punkte_escape(&mut self) -> bool {
        if self.platte.punkte.zug.take().is_some() {
            self.punkte_zeile(self.platte.punkte.hover.clone().as_ref());
            self.redraw = true;
            return true;
        }
        if self.platte.punkte.gewaehlt.is_empty() || !self.punkte_an() {
            return false;
        }
        self.platte.punkte.gewaehlt.clear();
        self.redraw = true;
        true
    }

    /// Entf löscht die gewählten Punkte; `true`, wenn Entf ihnen galt.
    pub(crate) fn punkte_entf(&mut self) -> bool {
        if self.platte.punkte.gewaehlt.is_empty() || !self.punkte_an() {
            return false;
        }
        let w = self.platte.punkte.gewaehlt.clone();
        let idx: Vec<usize> = self
            .spots()
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| s.1.i)
            .collect();
        if self
            .scene
            .edit_model(LABEL_GELOESCHT, |m| m.remove_terrain_points(&idx))
        {
            self.platte.punkte.gewaehlt.clear();
            self.platte.punkte.hover = None;
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(DREI), crate::NOTICE_TIME);
        }
        true
    }

    /// Strg+D: die gewählten Punkte um 1,00 m versetzt kopieren und die
    /// Kopien wählen. `true`, wenn die Taste den Punkten galt.
    pub(crate) fn punkte_taste(&mut self, key: Key, mods: Modifiers) -> bool {
        if !(key == Key::Char('D') && mods.ctrl && !mods.alt)
            || self.platte.punkte.gewaehlt.is_empty()
            || !self.punkte_an()
        {
            return false;
        }
        let w = self.platte.punkte.gewaehlt.clone();
        let neu: Vec<[f64; 3]> = self
            .spots()
            .iter()
            .filter(|s| w.contains(&s.0))
            .map(|s| [s.1.p[0] + VERSATZ, s.1.p[1], s.1.p[2]])
            .collect();
        let mut idx = Vec::new();
        if self.scene.edit_model(LABEL_KOPIE, |m| {
            idx = m.add_terrain_points(&neu);
            !idx.is_empty()
        }) {
            self.punkte_waehlen(&idx);
            self.punkte_fertig();
        } else {
            self.status(Meldung::satz(BELEGT), crate::NOTICE_TIME);
        }
        true
    }

    /// Statuszeile zum Ziel unter der Maus; ohne Ziel geht ein Satz der
    /// Punkte wieder aus der Zeile.
    fn punkte_zeile(&mut self, ziel: Option<&Ziel>) {
        match ziel {
            Some(Ziel::Hoehe) => self.status(Meldung::satz(HOEHE), Duration::from_secs(3600)),
            Some(Ziel::Punkt(nr)) => {
                let spots = self.spots();
                let Some(s) = spots.iter().find(|s| s.0 == *nr) else {
                    return;
                };
                let h = self.punkt_hoehe(s.1.p[2]);
                self.status(Meldung::mit(PUNKT, &[nr, &h]), Duration::from_secs(3600));
            }
            None => {
                let unsere = |t: &str| t == HOEHE || t.starts_with("Höhenpunkt ");
                if self.notice.as_ref().is_some_and(|n| unsere(&n.text)) {
                    self.notice = None;
                    self.renderer
                        .set_overlay(crate::OVERLAY_NOTICE, 0, 0, 0, 0, &[]);
                    self.redraw = true;
                }
            }
        }
    }

    /// Die gewählten Punkte und der unter der Maus, der Höhengriff an der
    /// Wahl, beim Ziehen die Punkte an ihrer neuen Stelle.
    pub(crate) fn punkte_linien(&self, z: f64, scale: f32, theme: &Theme) -> Vec<Helper> {
        if !self.punkte_an() {
            return Vec::new();
        }
        let col = &theme.interact;
        let p = |q: [f64; 2]| [q[0] as f32, q[1] as f32, z as f32 + 3.0];
        let punkt = |q: [f64; 2], px: f32, color, round| Helper {
            a: p(q),
            b: p(q),
            color,
            width: px * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: false,
            round,
        };
        let linie = |a: [f64; 2], b: [f64; 2], color, w: f32| Helper {
            a: p(a),
            b: p(b),
            color,
            width: w * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: false,
            round: false,
        };
        let mut out = Vec::new();
        let spots = self.spots();
        let w = &self.platte.punkte.gewaehlt;
        let heiss = match &self.platte.punkte.hover {
            Some(Ziel::Punkt(nr)) => Some(nr.clone()),
            _ => None,
        };
        for s in spots.iter() {
            let an = heiss.as_ref() == Some(&s.0);
            if w.contains(&s.0) || an {
                let farbe = if an { col.drag } else { col.select };
                out.push(punkt([s.1.p[0], s.1.p[1]], PUNKT_PX * 1.3, farbe, true));
            }
        }
        if let Some(zug) = self.platte.punkte.zug.as_ref().filter(|z| z.bewegt) {
            for (a, n) in zug.alt.iter().zip(&zug.neu) {
                let (q0, q1) = ([a.1[0], a.1[1]], [n[0], n[1]]);
                if q0 != q1 {
                    out.push(linie(q0, q1, col.guide, 1.0));
                }
                out.push(punkt(q1, PUNKT_PX * 1.3, col.drag, true));
            }
        }
        // Höhengriff: senkrechter Pfeil rechts über dem ersten gewählten
        let (vw, vh, sc) = self.view_size();
        if let Some(g) = self.hoehen_griff(vw, vh, sc) {
            let welt = |x: f64, y: f64| self.cam.ground_point(x, y, vw, vh).map(|v| [v.x, v.y]);
            let an = self.platte.punkte.hover == Some(Ziel::Hoehe)
                || self.platte.punkte.zug.as_ref().is_some_and(|z| z.hoehe);
            let farbe = if an { col.drag } else { col.select };
            let l = PFEIL_PX * sc;
            if let (Some(a), Some(b), Some(c), Some(d)) = (
                welt(g.0, g.1 + l * 0.5),
                welt(g.0, g.1 - l * 0.5),
                welt(g.0 - l * 0.3, g.1 - l * 0.15),
                welt(g.0 + l * 0.3, g.1 - l * 0.15),
            ) {
                out.push(linie(a, b, farbe, 2.0));
                out.push(linie(b, c, farbe, 2.0));
                out.push(linie(b, d, farbe, 2.0));
                out.push(punkt(a, 5.0, farbe, true));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    /// Grundriss von oben über 0..20 m, 800 × 800 Bildpunkte.
    fn kamera() -> Camera {
        Camera::parallel(vec3(10000.0, 10000.0, 0.0), FRAC_PI_2, -FRAC_PI_2, 12500.0)
    }

    fn spot(nr: &str, i: usize, x: f64, y: f64) -> (String, TerrainSpot) {
        (
            nr.into(),
            TerrainSpot {
                i,
                p: [x, y, 0.0],
                hand: true,
            },
        )
    }

    /// Der nächste Punkt bis 7 Bildpunkte, daneben keiner.
    #[test]
    fn punkt_unter_der_maus() {
        let cam = kamera();
        let s = [spot("H1", 0, 5000.0, 5000.0), spot("H2", 1, 5200.0, 5000.0)];
        let t = |x: f64, y: f64| {
            let q = cam.project(vec3(x, y, 0.0), 800.0, 800.0).unwrap();
            treffen(&s, &cam, 0.0, q, 800.0, 800.0, 1.0).map(|s| s.0.clone())
        };
        assert_eq!(t(5050.0, 5000.0).as_deref(), Some("H1"));
        assert_eq!(t(5150.0, 5000.0).as_deref(), Some("H2"));
        assert_eq!(t(6000.0, 5000.0), None);
    }

    /// Setzen, Höhe ändern, kopieren und löschen sind je ein Schritt der
    /// Szene, den Rückgängig ganz zurücknimmt.
    #[test]
    fn schritte_der_szene() {
        let mut m = sk_model::Model::with_seed(1);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let mut s = crate::scene::Scene::with_model(m);
        let mut neu = Vec::new();
        assert!(s.edit_model(LABEL_GESETZT, |m| {
            neu = m.add_terrain_points(&[[15000.0, 4000.0, -300.0]]);
            !neu.is_empty()
        }));
        let i = neu[0];
        let vorher = s.model().terrain_spots();
        assert!(s.edit_model(LABEL_HOEHE, |m| m
            .move_terrain_points(&[(i, [15000.0, 4000.0, 200.0])])));
        let k = s.edit_model(LABEL_KOPIE, |m| {
            !m.add_terrain_points(&[[16000.0, 4000.0, 200.0]]).is_empty()
        });
        assert!(k);
        assert_eq!(s.model().terrain_spots().len(), vorher.len() + 1);
        assert!(s.undo() && s.undo());
        assert_eq!(s.model().terrain_spots(), vorher);
    }

    #[test]
    fn hub_in_cm() {
        assert_eq!(hub(-10.0, 1.0, false), 100.0);
        assert_eq!(hub(4.0, 1.0, false), -40.0);
        assert_eq!(hub(-3.0, 2.0, false), 20.0, "auf cm gerundet");
        assert_eq!(
            hub(-10.0, 1.0, true),
            1000.0,
            "mit Umschalt 10 cm je Bildpunkt"
        );
        assert_eq!(m_vz(-320.0), "−0,32");
        assert_eq!(m_vz(0.0), "+0,00");
    }

    /// Lage beim Ziehen: im Raster, mit Alt ohne Fang.
    #[test]
    fn lage_im_raster() {
        let z = Zug {
            hoehe: false,
            px: (0.0, 0.0),
            start: [1000.0, 1000.0],
            griff: [1000.0, 1000.0],
            alt: vec![(0, [1000.0, 1000.0, 0.0])],
            neu: vec![],
            ziele: Ziele::default(),
            kopie: false,
            bewegt: true,
            fest: false,
            raus: false,
            nr: "H1".into(),
        };
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        let d = lage(&z, [1234.0, 987.0], 50.0, alt);
        assert_eq!(
            d,
            [
                (1234.0f64 / STEP).round() * STEP - 1000.0,
                (987.0f64 / STEP).round() * STEP - 1000.0
            ]
        );
    }
}

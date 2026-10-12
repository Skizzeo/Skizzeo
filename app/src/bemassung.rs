//! Bemaßung im Grundriss und in 3D (Konzept `bemassung/konzept.md`): die
//! Außenmaßketten aus [`sk_plan`], flach auf der Geschossebene. Im
//! Grundriss die Ebene des aktiven Geschosses, in 3D die des gewählten
//! Bauteils (sonst die des aktiven), in jedem Gebäude, das auf ihr ein
//! Geschoss hat. Linien, Hilfslinien, Kreise und Schrägstriche sind
//! Hilfslinien mit der Strichstärke des Stils, Schrift, Punkte und Pfeile
//! sind Tinte ([`sk_render::Ink`]).
//!
//! Gezeigt wird nur, was das Zahnrad der Ansicht erlaubt ([`Model::mass_sicht`]);
//! „Maßbezugspunkte zeigen“ setzt an jeden Bezugspunkt einen Punkt mit
//! gestrichelter Linie zur Kette, in der Spurlinienfarbe. Gewählte Ketten,
//! die Kette oder Zahl unter der Maus und das Maß in Arbeit leuchten in den
//! Farben der Interaktion ([`Hervor`]); bedient wird in
//! [`crate::wall_edit::mass_edit`].

use crate::camera::Camera;
use crate::scene::{neuer_stand, Scene};
use crate::ui::ViewKind;
use crate::wall_edit::mass_edit::Hervor;
use crate::wall_edit::mass_treffer::{finde, id_von, MassId};
use sk_math::{vec3, Vec3};
use sk_model::bemassung::{Kettenart, Stil};
use sk_model::{ElementId, LevelKind, Model, StoreyId};
use sk_paint::font::Font;
use sk_plan::aussen::{aussenmasse_mit, Aussenmasse, Verzeichnis};
use sk_plan::bild::{bild, Bild, Strichart, Zahlart};
use sk_plan::platz::{dazu, kasten, nachbarn, ohne, zu_zweit, Weg};
use sk_plan::schrift::Schrift;
use sk_render::{Helper, Ink, SOLID};
use sk_ui::theme::Theme;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[cfg(all(test, target_os = "linux"))]
#[path = "bemassung_istbilder.rs"]
mod istbilder;
#[cfg(test)]
#[path = "bemassung_probe.rs"]
mod probe;

/// Wählbare Schriften im Reiter „Bemaßung“ und ihre Dateien im
/// Schriftenordner (ohne Datei: Ausweichschrift Arial, dann die Schrift der
/// Oberfläche).
pub const SCHRIFTEN: [(&str, &[&str]); 4] = [
    ("Segoe UI", &["segoeui.ttf"]),
    ("Arial", &["arial.ttf"]),
    ("Calibri", &["calibri.ttf"]),
    ("ISOCPEUR", &["isocpeur.ttf", "ISOCPEUR.TTF"]),
];

/// Punkt am Bezugspunkt (dip) und Strichlänge der Bezugslinie (dip).
const BEZUG_PUNKT: f32 = 6.0;
const BEZUG_STRICH: f32 = 4.0;
/// Kleiner am Bildschirm (Bildpunkte) ist die Schrift nicht mehr lesbar und
/// der Kreis der Begrenzung kein Kreis: dann ohne Schrift bzw. ohne Kreise
/// (Review W30, weit herausgezoomt über viele Häuser).
const LESBAR_PX: f64 = 2.0;
const SYMBOL_PX: f64 = 1.5;
/// Pfeil am bewegten Ende einer Maßzahl in Eingabe: Länge und Spitze (mm
/// auf dem Papier), Strichbreite (dip).
const PFEIL_MM: f64 = 5.0;
const SPITZE_MM: f64 = 1.5;
const PFEIL_STRICH: f32 = 2.0;

/// Kamera und Größe der Ansicht (Bildpunkte): Geschosse außerhalb des
/// Bildes zeichnet die Bemaßung nicht.
pub struct Blick<'a> {
    pub cam: &'a Camera,
    pub w: f64,
    pub h: f64,
}

impl Blick<'_> {
    /// Bildpunkte je mm am Bau im Rahmen `[lo, hi]`; `None`, wenn er ganz
    /// außerhalb des Bildes liegt. Liegt er teils hinter der Kamera, zählt
    /// er als nah.
    fn massstab(&self, [lo, hi]: [Vec3; 2]) -> Option<f64> {
        let (cam, w, h) = (self.cam, self.w, self.h);
        let ecken = [lo, vec3(hi.x, lo.y, lo.z), hi, vec3(lo.x, hi.y, hi.z)];
        let Some(p) = ecken
            .iter()
            .map(|&e| cam.project(e, w, h))
            .collect::<Option<Vec<_>>>()
        else {
            return Some(f64::INFINITY);
        };
        let (x0, x1) = p
            .iter()
            .fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.0), a.1.max(q.0)));
        let (y0, y1) = p
            .iter()
            .fold((f64::MAX, f64::MIN), |a, q| (a.0.min(q.1), a.1.max(q.1)));
        if x1 < 0.0 || x0 > w || y1 < 0.0 || y0 > h {
            return None;
        }
        let m = (lo + hi) * 0.5;
        match (
            cam.project(m, w, h),
            cam.project(m + cam.right() * 1000.0, w, h),
        ) {
            (Some(a), Some(b)) => Some(((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt() / 1000.0),
            _ => Some(f64::INFINITY),
        }
    }
}

/// Ketten eines Geschosses (ohne die weichenden) und ihr Bild, mit Linien
/// und Tinte, wie sie zuletzt gezeichnet wurden.
pub struct Geschoss {
    pub a: Rc<Aussenmasse>,
    pub b: Bild,
    /// Eindeutig je Rechnung: Schlüssel der Tinte im Renderer.
    nr: u64,
    /// Kleinster Quader um das Bild.
    rahmen: [Vec3; 2],
    /// Linien mit und ohne Kreise der Begrenzung; Tinte im Grundriss und
    /// verdeckbar in 3D.
    linien: RefCell<[Option<Linien>; 2]>,
    tinte: RefCell<[Option<Rc<[Ink]>>; 2]>,
}

/// Wofür die gemerkten Linien eines Geschosses gelten: Maßstab der
/// Oberfläche, Bildpunkte je mm, verdeckbar, Bezugspunkte, deren Farbe.
type Linienart = (u32, u32, bool, bool, [u32; 4]);

/// Gemerkte Linien eines Geschosses und wofür sie gelten.
type Linien = (Linienart, Rc<[Helper]>);

/// Tinte eines Geschosses unter ihrem Schlüssel im Renderer.
pub type Teil = (u64, Rc<[Ink]>);

/// Gerechnete Ketten der Geschosse einer Ebene.
pub struct Stand {
    pub geschosse: Vec<Rc<Geschoss>>,
}

impl Stand {
    /// Ketten und Bild je Geschoss.
    pub fn paare(&self) -> impl Iterator<Item = (&Aussenmasse, &Bild)> {
        self.geschosse.iter().map(|g| (&*g.a, &g.b))
    }
}

/// Tinte eines hervorgehobenen Geschosses: gilt für das Geschoss
/// (Nummer), die Ansicht (verdeckbar) und die Farben je Dreieck.
type HellTinte = (u64, bool, Vec<Option<[f32; 4]>>, Teil);

/// Wofür die Ketten eines Geschosses gelten: Stand der Szene im Geschoss
/// ([`Scene::mass_stand`]) und seine Versätze. Der Stil gilt für alle.
type Schluessel = ((u64, u64), Vec<((i32, Kettenart), f64)>);

/// Alle Ketten eines Geschosses, eindeutig nummeriert je Rechnung, und der
/// Kasten um ihre Bereiche ([`sk_plan::platz::kasten`]).
#[derive(Clone)]
struct Ketten {
    nr: u64,
    a: Rc<Aussenmasse>,
    kasten: Option<[f64; 4]>,
}

/// Bemaßung der App: gemerkte Schriften, je Geschoss die zuletzt
/// gerechneten Ketten und ihr Bild. Beim Ziehen rechnet nur das Geschoss
/// des gezogenen Zugs neu, beim Neuzeichnen ohne Änderung nichts (Review
/// W30). Stehen Gebäude zu dicht, weichen Ketten ([`sk_plan::platz`]):
/// gerechnet nur für die Paare mit dem geänderten Geschoss, das Bild eines
/// Nachbarn wird nur neu, wenn sich ändert, was bei ihm weicht.
#[derive(Default)]
pub struct Massketten {
    schriften: HashMap<String, Option<Rc<Schrift>>>,
    /// Stil der gemerkten Geschosse; ein anderer rechnet alle neu (auch
    /// „Abbrechen“ im Einstellungsfenster).
    stil: Option<Stil>,
    ketten: HashMap<StoreyId, (Schluessel, Option<Ketten>)>,
    /// Zuletzt geprüfter Platz: Nummern der Ketten und was je Geschoss
    /// weicht.
    platz: Option<(Vec<u64>, Vec<Weg>)>,
    /// Was je Paar von Nachbarn weicht, nach den Nummern ihrer Ketten.
    paare: HashMap<(u64, u64), [Weg; 2]>,
    /// Ab so vielen Paaren fallen die mit nicht mehr gemerkten Ketten heraus.
    paare_grenze: usize,
    /// Bild je Geschoss und wofür es gilt: Nummer der Ketten, was weicht.
    geschosse: HashMap<StoreyId, ((u64, Weg), Rc<Geschoss>)>,
    /// Zuletzt zusammengestellter Stand und die Nummern seiner Geschosse.
    stand: Option<(Vec<u64>, Rc<Stand>)>,
    /// Tinte hervorgehobener Geschosse beim letzten Zeichnen: Gleiche
    /// Farben behalten ihren Schlüssel, der Renderer lädt nichts neu.
    hell: Vec<HellTinte>,
}

/// Ansicht mit Bemaßung (Stufe 1: Grundriss und 3D).
pub fn ansicht(v: ViewKind) -> bool {
    matches!(v, ViewKind::Plan | ViewKind::Persp)
}

/// Geschosse, deren Ketten die Ansicht `v` zeigt: die Ebene des aktiven
/// Geschosses, in 3D die des ersten gewählten Bauteils, in jedem Gebäude.
/// Die Gründung hat keine.
pub fn geschosse(scene: &Scene, v: ViewKind, gewaehlt: &[ElementId]) -> Vec<StoreyId> {
    let m = scene.model();
    let normal = |st: StoreyId| {
        m.storey(st)
            .is_some_and(|s| s.kind != LevelKind::Foundation)
    };
    let aus_wahl = (v == ViewKind::Persp)
        .then(|| {
            gewaehlt
                .iter()
                .filter_map(|&id| m.element(id).map(|e| e.storey))
                .find(|&st| normal(st))
        })
        .flatten();
    let Some(z) = aus_wahl
        .or(Some(scene.active_storey()))
        .filter(|&st| normal(st))
        .and_then(|st| m.storey(st))
        .map(|s| s.elevation)
    else {
        return Vec::new();
    };
    m.storeys()
        .iter()
        .filter(|(id, s)| normal(*id) && (s.elevation - z).abs() < 1.0)
        .map(|(id, _)| id)
        .collect()
}

/// Schrift zum Namen `name` aus dem Schriftenordner des Systems; fehlt sie,
/// Arial, dann die Schrift der Oberfläche (`ui`).
pub fn schrift_laden(name: &str, ui: Option<&Font>) -> Option<Schrift> {
    let eigene: Vec<String> = SCHRIFTEN
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, f)| f.iter().map(|s| s.to_string()).collect())
        .unwrap_or_else(|| vec![format!("{}.ttf", name.replace(' ', "").to_lowercase())]);
    let mut dateien: Vec<&str> = eigene.iter().map(String::as_str).collect();
    dateien.push("arial.ttf");
    Font::system(&dateien)
        .or_else(|| ui.and_then(|f| Font::parse(f.data().to_vec())))
        .map(Schrift::new)
}

impl Massketten {
    /// Schrift des Stils, einmal geladen.
    fn schrift(&mut self, name: &str, ui: Option<&Font>) -> Option<Rc<Schrift>> {
        self.schriften
            .entry(name.to_string())
            .or_insert_with(|| schrift_laden(name, ui).map(Rc::new))
            .clone()
    }

    /// Ketten und Bilder der Geschosse `st`; neu gerechnet nur Geschosse,
    /// deren Stand sich geändert hat, neu gezeichnet auch die, bei denen
    /// sich ändert, was weicht.
    pub fn stand(
        &mut self,
        scene: &Scene,
        st: Vec<StoreyId>,
        ui: Option<&Font>,
    ) -> Option<Rc<Stand>> {
        let m = scene.model();
        let stil = m.massstil();
        if self.stil.as_ref() != Some(stil) {
            self.ketten.clear();
            self.geschosse.clear();
            self.platz = None;
            self.paare.clear();
            self.stil = Some(stil.clone());
        }
        let schrift = self.schrift(&stil.schrift, ui)?;
        // Gemerkt bleiben alle Geschosse, die es noch gibt: Zurück auf eine
        // andere Ebene rechnet nichts neu (Review W33)
        if self.ketten.keys().any(|&k| m.storey(k).is_none()) {
            self.ketten.retain(|&k, _| m.storey(k).is_some());
            self.geschosse.retain(|&k, _| m.storey(k).is_some());
        }
        let keys: Vec<Schluessel> = st
            .iter()
            .map(|&s| (scene.mass_stand(s), versaetze(m, s)))
            .collect();
        // Züge, Öffnungen und Anschlüsse in einem Durchgang durchs Modell,
        // nur für die Geschosse, die neu rechnen
        let neu: HashSet<StoreyId> = st
            .iter()
            .zip(&keys)
            .filter(|(s, key)| self.ketten.get(s).is_none_or(|(k, _)| k != *key))
            .map(|(s, _)| *s)
            .collect();
        let mut vz: Option<Verzeichnis> = None;
        let mut da: Vec<(StoreyId, Ketten)> = Vec::new();
        for (&s, key) in st.iter().zip(keys) {
            let k = match self.ketten.get(&s) {
                Some((k, a)) if *k == key => a.clone(),
                _ => {
                    let vz = vz.get_or_insert_with(|| Verzeichnis::fuer(m, |s| neu.contains(&s)));
                    let k = ketten(scene, vz, s).map(|a| Ketten {
                        nr: neuer_stand(),
                        kasten: kasten(&a, stil),
                        a: Rc::new(a),
                    });
                    self.ketten.insert(s, (key, k.clone()));
                    k
                }
            };
            da.extend(k.map(|k| (s, k)));
        }
        let nrn: Vec<u64> = da.iter().map(|(_, k)| k.nr).collect();
        if self.platz.as_ref().is_none_or(|(k, _)| *k != nrn) {
            let kaesten: Vec<_> = da.iter().map(|(_, k)| k.kasten).collect();
            let mut wege = vec![Weg::new(); da.len()];
            let paare = nachbarn(&kaesten);
            for &(i, j) in &paare {
                let (a, b) = (&da[i].1, &da[j].1);
                let w = self
                    .paare
                    .entry((a.nr, b.nr))
                    .or_insert_with(|| zu_zweit(&a.a, &b.a, stil));
                dazu(&mut wege[i], &w[0]);
                dazu(&mut wege[j], &w[1]);
            }
            // Paare mit alten Ketten fallen gelegentlich heraus; die anderer
            // Ebenen bleiben, solange deren Ketten gemerkt sind (Review W33)
            if self.paare.len() > self.paare_grenze.max(2 * paare.len() + 16) {
                let lebend: HashSet<u64> = self
                    .ketten
                    .values()
                    .filter_map(|(_, k)| k.as_ref().map(|k| k.nr))
                    .collect();
                self.paare
                    .retain(|(a, b), _| lebend.contains(a) && lebend.contains(b));
                self.paare_grenze = 2 * self.paare.len() + 16;
            }
            self.platz = Some((nrn, wege));
        }
        let wege = &self.platz.as_ref()?.1;
        let mut liste = Vec::new();
        for ((s, k), weg) in da.iter().zip(wege) {
            let (n, a) = (k.nr, &k.a);
            let g = match self.geschosse.get(s) {
                Some(((kn, kw), g)) if *kn == n && kw == weg => g.clone(),
                _ => {
                    let a = if weg.is_empty() {
                        a.clone()
                    } else {
                        Rc::new(ohne(a, weg))
                    };
                    let g = Rc::new(geschoss(a, stil, &schrift));
                    self.geschosse.insert(*s, ((n, weg.clone()), g.clone()));
                    g
                }
            };
            liste.push(g);
        }
        let nr: Vec<u64> = liste.iter().map(|g| g.nr).collect();
        if let Some((k, s)) = &self.stand {
            if *k == nr {
                return Some(s.clone());
            }
        }
        let s = Rc::new(Stand { geschosse: liste });
        self.stand = Some((nr, s.clone()));
        Some(s)
    }

    /// Ketten der Ansicht `v`, wenn das Zahnrad sie zeigt.
    pub fn gezeigt(
        &mut self,
        scene: &Scene,
        v: ViewKind,
        gewaehlt: &[ElementId],
        ui: Option<&Font>,
    ) -> Option<Rc<Stand>> {
        if !ansicht(v) || !scene.mass_sicht(v).0 {
            return None;
        }
        let st = geschosse(scene, v, gewaehlt);
        self.stand(scene, st, ui)
    }

    /// Hilfslinien und Tinte der Bemaßung für die Ansicht `v` (leer, wenn
    /// das Zahnrad sie ausblendet oder es nichts zu bemaßen gibt), mit der
    /// Hervorhebung `hervor`. Die
    /// Tinte kommt in Teilen je Geschoss mit einem Schlüssel für ihren
    /// Inhalt ([`sk_render::Renderer::set_ink_teile`]); Linien und Tinte
    /// eines Geschosses werden nur neu gebaut, wenn es neu gerechnet wurde
    /// oder Maßstab, Ansicht oder Farben wechseln. Geschosse außerhalb von
    /// `blick` fehlen, sehr kleine ohne Schrift und Kreise. Hervorgehobene
    /// Geschosse bekommen ihre Linien bei jedem Zeichnen neu, ihre Tinte,
    /// wenn sich die Farben ändern.
    #[allow(clippy::too_many_arguments)]
    pub fn zeichnen(
        &mut self,
        scene: &Scene,
        v: ViewKind,
        gewaehlt: &[ElementId],
        ui: Option<&Font>,
        blick: &Blick,
        scale: f32,
        theme: &Theme,
        hervor: &Hervor,
    ) -> (Vec<Helper>, Vec<Teil>) {
        let Some(stand) = self.gezeigt(scene, v, gewaehlt, ui) else {
            self.hell.clear();
            return (Vec::new(), Vec::new());
        };
        let (m, bezug) = (scene.model(), scene.mass_sicht(v).1);
        let occlude = v == ViewKind::Persp;
        let stil = m.massstil();
        let art: Linienart = (
            scale.to_bits(),
            theme.px_per_mm.to_bits(),
            occlude,
            bezug,
            theme.interact.track.map(f32::to_bits),
        );
        let (mut out, mut ink) = (Vec::new(), Vec::new());
        let mut hell = Vec::new();
        for g in &stand.geschosse {
            let Some(px) = blick.massstab(g.rahmen) else {
                continue;
            };
            let kreise = stil.am_bau(stil.symbol_mm) * 0.5 * px >= SYMBOL_PX;
            let lesbar = stil.am_bau(stil.schrift_mm) * px >= LESBAR_PX;
            let (ketten, zahlen) = farben(&g.a, &g.b, hervor, theme);
            if !ketten.is_empty() || zahlen.iter().any(Option::is_some) {
                let mut v = linien(&g.b, stil, scale, theme, occlude, &ketten);
                if !kreise {
                    v.retain(|h| !h.round);
                }
                if bezug {
                    v.extend(bezugspunkte(&g.b, scale, theme, occlude));
                }
                out.extend(v);
                if !lesbar {
                    continue;
                }
                let alt = self
                    .hell
                    .iter()
                    .position(|h| h.0 == g.nr && h.1 == occlude && h.2 == zahlen);
                let t = match alt {
                    Some(i) => self.hell.swap_remove(i).3,
                    None => {
                        let t: Rc<[Ink]> = tinte(&g.b, stil, occlude, &zahlen).into();
                        (neuer_stand() << 1 | occlude as u64, t)
                    }
                };
                ink.push(t.clone());
                hell.push((g.nr, occlude, zahlen, t));
                continue;
            }
            let mut l = g.linien.borrow_mut();
            let l = &mut l[!kreise as usize];
            if l.as_ref().is_none_or(|(a, _)| *a != art) {
                let mut v = linien(&g.b, stil, scale, theme, occlude, &[]);
                if !kreise {
                    v.retain(|h| !h.round);
                }
                if bezug {
                    v.extend(bezugspunkte(&g.b, scale, theme, occlude));
                }
                *l = Some((art, v.into()));
            }
            out.extend_from_slice(&l.as_ref().unwrap().1);
            if !lesbar {
                continue;
            }
            let t = g.tinte.borrow_mut()[occlude as usize]
                .get_or_insert_with(|| tinte(&g.b, stil, occlude, &[]).into())
                .clone();
            ink.push((g.nr << 1 | occlude as u64, t));
        }
        self.hell = hell;
        let pfeil = hervor.ende.and_then(|(z, e)| pfeil(&stand, z, e, stil));
        for (a, b) in pfeil.into_iter().flatten() {
            out.push(Helper {
                a: a.to_f32(),
                b: b.to_f32(),
                color: theme.interact.drag,
                width: PFEIL_STRICH * scale,
                dash: 0.0,
                pattern: SOLID,
                occlude: false,
                round: true,
            });
        }
        (out, ink)
    }
}

/// Pfeil am bewegten Ende `ende` (0 Anfang, 1 Ende) der Maßzahl `m`:
/// Schaft und Spitze, auf der Maßlinie nach außen, hinter dem
/// Begrenzungssymbol.
fn pfeil(stand: &Stand, m: MassId, ende: usize, stil: &Stil) -> Option<[(Vec3, Vec3); 3]> {
    let f = finde(stand, m.kette)?;
    let r = f.kette.bezuege.get(m.i + ende)?;
    let (t, n) = (f.seite.t, f.seite.n);
    let q = t * r.t + n * f.kette.lage;
    let p = vec3(q.x, q.y, f.a.z);
    let d = if ende == 1 { t } else { t * -1.0 };
    let luft = stil.am_bau(stil.symbol_mm);
    let (lang, spitze) = (stil.am_bau(PFEIL_MM), stil.am_bau(SPITZE_MM));
    let a = p + d * luft;
    let b = a + d * lang;
    let s = b - d * spitze;
    Some([
        (a, b),
        (b, s + n * (0.6 * spitze)),
        (b, s - n * (0.6 * spitze)),
    ])
}

/// Farben der Hervorhebung: je Kette (Seite, Kette) im Bild und je Dreieck
/// der Tinte, `None` in der Farbe des Stils.
type Kettenfarben = Vec<((usize, usize), [f32; 4])>;

fn farben(
    a: &Aussenmasse,
    b: &Bild,
    h: &Hervor,
    theme: &Theme,
) -> (Kettenfarben, Vec<Option<[f32; 4]>>) {
    let col = &theme.interact;
    let mut ketten = Vec::new();
    for (si, s) in a.seiten.iter().enumerate() {
        for ki in 0..s.ketten.len() {
            let Some(k) = id_von(a, si, ki) else { continue };
            if h.gewaehlt.contains(&k) {
                ketten.push(((si, ki), col.select));
            } else if h.ueber == Some(k) {
                ketten.push(((si, ki), col.drag_hot));
            }
        }
    }
    let mut zahlen = vec![None; b.tinte.len()];
    let mut hell = false;
    for z in &b.zahlen {
        let (si, ki) = match z.art {
            Zahlart::Mass { seite, kette, .. } => (seite, kette),
            Zahlart::Hoehe { seite, .. } | Zahlart::Brh { seite, .. } => {
                let Some(ki) = a.seiten[seite]
                    .ketten
                    .iter()
                    .position(|k| k.art == Kettenart::Oeffnung)
                else {
                    continue;
                };
                (seite, ki)
            }
        };
        let mut c = ketten.iter().find(|x| x.0 == (si, ki)).map(|x| x.1);
        if let (Zahlart::Mass { seite, kette, i }, Some((hm, aktiv))) = (z.art, h.zahl) {
            let m = id_von(a, seite, kette).map(|k| MassId { kette: k, i });
            if m == Some(hm) {
                c = Some(if aktiv { col.drag } else { col.drag_hot });
            }
        }
        if let Some(c) = c {
            hell = true;
            for f in &mut zahlen[z.tinte.clone()] {
                *f = Some(c);
            }
        }
    }
    if !hell {
        zahlen.clear();
    }
    (ketten, zahlen)
}

/// Versätze der Ketten im Geschoss `st` (nur dessen Abschnitt der
/// geordneten Versätze, Review W33).
fn versaetze(m: &Model, st: StoreyId) -> Vec<((i32, Kettenart), f64)> {
    let Some(g) = m.storey(st).map(|s| s.guid) else {
        return Vec::new();
    };
    m.bemassung()
        .versatz
        .range((g, i32::MIN, Kettenart::Oeffnung)..)
        .take_while(|(k, _)| k.0 == g)
        .map(|(k, v)| ((k.1, k.2), *v))
        .collect()
}

/// Ketten des Geschosses `st`, die Züge aus dem Zwischenspeicher der Szene.
fn ketten(scene: &Scene, vz: &Verzeichnis, st: StoreyId) -> Option<Aussenmasse> {
    let m = scene.model();
    aussenmasse_mit(m, vz, st, |id| {
        scene
            .chain(id)
            .map(Cow::Borrowed)
            .or_else(|| m.chain(id).map(Cow::Owned))
    })
}

/// Bild der Ketten `a`, eindeutig nummeriert.
fn geschoss(a: Rc<Aussenmasse>, stil: &Stil, schrift: &Schrift) -> Geschoss {
    let b = bild(&a, stil, schrift);
    let punkte = b
        .striche
        .iter()
        .flat_map(|s| [s.a, s.b])
        .chain(b.tinte.iter().flatten().copied())
        .chain(b.bezuege.iter().map(|r| r.am_bauteil));
    let (lo, hi) = (
        vec3(f64::MAX, f64::MAX, f64::MAX),
        vec3(f64::MIN, f64::MIN, f64::MIN),
    );
    let rahmen = punkte.fold([lo, hi], |[lo, hi], p| {
        [
            vec3(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z)),
            vec3(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z)),
        ]
    });
    Geschoss {
        a,
        b,
        nr: neuer_stand(),
        rahmen,
        linien: RefCell::new([None, None]),
        tinte: RefCell::new([None, None]),
    }
}

/// Farbe des Stils als RGBA.
pub fn farbe(s: &Stil) -> [f32; 4] {
    let [r, g, b] = s.farbe;
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
}

/// Striche des Bildes als Hilfslinien: Strichstärke aus dem Stil (mm auf
/// dem Papier) wie bei allen Stiften; hervorgehobene Ketten in ihrer Farbe.
pub fn linien(
    b: &Bild,
    s: &Stil,
    scale: f32,
    theme: &Theme,
    occlude: bool,
    hervor: &[((usize, usize), [f32; 4])],
) -> Vec<Helper> {
    let px = theme.px_per_mm * scale;
    let stil = farbe(s);
    b.striche
        .iter()
        .map(|st| {
            let color = hervor
                .iter()
                .find(|h| h.0 == st.kette)
                .map_or(stil, |h| h.1);
            let mm = match st.art {
                Strichart::Masslinie => s.linie,
                Strichart::Hilfslinie => s.hilfslinie,
                Strichart::Symbol => s.symbol_strich,
            };
            Helper {
                a: st.a.to_f32(),
                b: st.b.to_f32(),
                color,
                width: (mm as f32 * px).max(1.0),
                dash: 0.0,
                pattern: SOLID,
                occlude,
                round: st.art == Strichart::Symbol,
            }
        })
        .collect()
}

/// Gefüllte Dreiecke des Bildes in der Farbe des Stils, hervorgehobene in
/// ihrer (`hervor` je Dreieck, leer: keine).
pub fn tinte(b: &Bild, s: &Stil, occlude: bool, hervor: &[Option<[f32; 4]>]) -> Vec<Ink> {
    let stil = farbe(s);
    b.tinte
        .iter()
        .enumerate()
        .map(|(i, d)| Ink {
            p: d.map(|p| p.to_f32()),
            color: hervor.get(i).copied().flatten().unwrap_or(stil),
            occlude,
        })
        .collect()
}

/// Maßbezugspunkte: Punkt am Bauteil, gestrichelte Linie zur Kette.
pub fn bezugspunkte(b: &Bild, scale: f32, theme: &Theme, occlude: bool) -> Vec<Helper> {
    let color = theme.interact.track;
    let mut out = Vec::new();
    for r in &b.bezuege {
        out.push(Helper {
            a: r.am_bauteil.to_f32(),
            b: r.an_kette.to_f32(),
            color,
            width: scale,
            dash: BEZUG_STRICH * scale,
            pattern: SOLID,
            occlude,
            round: false,
        });
        out.push(Helper {
            a: r.am_bauteil.to_f32(),
            b: r.am_bauteil.to_f32(),
            color,
            width: BEZUG_PUNKT * scale,
            dash: 0.0,
            pattern: SOLID,
            occlude: false,
            round: true,
        });
    }
    out
}

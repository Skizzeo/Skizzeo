//! Raumstempel im Grundriss (Grundriss G4, plan.md §4.4): je Raum des
//! aktiven Geschosses der Name und darunter die Grundfläche „12,34 m²“ an
//! der fertigen Oberfläche ([`sk_raum::flaechen::grundflaeche`], ohne
//! Faktor), wie gedruckt auf dem Papier des Grundrisses.
//!
//! Platz: Der Stempel steht an seinem Punkt (verschoben, sonst die Mitte
//! des größten Kreises im Raum), ganz im Raum, nicht unter einem Paneel und
//! nicht über einem anderen Stempel; passt er so nicht, nur die Fläche,
//! sonst entfällt er. Größere Räume gehen vor, das offene Namensfeld vor
//! allen. Ein nicht verschobener Stempel meidet die Türaufschläge (G7,
//! [`aufschlag`]): Überdeckt er an der Mitte einen Bogen, rückt er an die
//! Stelle mit dem größten Abstand zu Wänden und Bögen; passt er auch dort
//! nicht, steht er wie bisher an der Mitte, lieber über dem Bogen als nur
//! mit der Fläche.
//!
//! Bedienung, nicht bei eingeschaltetem Wandwerkzeug: Ziehen verschiebt den
//! Stempel innerhalb des Raums, ein Schritt „Raumstempel verschoben“; ein
//! Klick wählt den Raum und öffnet sein Namensfeld. Einen neu erkannten Raum
//! öffnet die App selbst. Der Name ist markiert, Tippen überschreibt;
//! darunter die Raumvorlagen aus dem Raumkatalog
//! ([`sk_model::Model::space_type_choices`]), gefiltert nach dem Getippten.
//! Tab übernimmt die nächste Vorlage ins Feld, ein Klick eine bestimmte; eine
//! gewählte Vorlage setzt mit dem Namen auch die Nutzung des Raums, ein frei
//! getippter Name lässt sie stehen. Enter übernimmt („Raum benannt“), ein
//! Klick daneben ebenso, Esc lässt den Namen.
//!
//! Aufwand (Review #87 R2): Die Auslegung gilt, bis sich Modell, Ansicht
//! oder Namensfeld ändern; Räume außerhalb des Bildes oder zu klein für
//! den kleinsten Stempel sieht sie gar nicht erst an. Ein gezogener Stempel
//! setzt nur seine eigene Lage ein, eine gezogene Wand ersetzt nur die
//! Flächen ihrer Hausgruppe ([`sk_raum::Erkennung::raumformen`]).

use crate::camera::Camera;
use crate::scene::Scene;
use crate::terrace_label::{inside, touches, Box, PAD};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, Guid, StoreyId};
use sk_paint::{font::Font, Canvas, Path, Rgba};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_raum::{Raumflaeche, Raumformen};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, FieldState, Fonts, Rect};

#[path = "aufschlag.rs"]
mod aufschlag;

/// Zeilen der Vorschlagsliste höchstens.
pub const ZEILEN: usize = 6;
/// Weg, ab dem ein Druck auf den Stempel zum Ziehen wird (dip).
const ZIEHEN: f64 = 4.0;
/// Breite des Namensfelds mindestens (dip).
const FELD: f64 = 150.0;
/// Platz für den Schatten um die Vorschlagsliste (dip).
const SCHATTEN: f64 = 10.0;

/// Ein Stempel, wie er zuletzt ausgelegt wurde (Pixel der Ansicht).
#[derive(Clone, Debug, PartialEq)]
pub struct Bild {
    /// Gespeicherter Raum; `None`: Fläche ohne Raum, ein Klick legt ihn an.
    pub raum: Option<ElementId>,
    /// Stempelpunkt im Modell (Grundrissebene).
    pub punkt: Vec3,
    pub name: String,
    pub flaeche: String,
    /// Kasten samt Rand.
    pub kasten: Box,
    /// Nur die Fläche: Der Name passt nicht in den Raum.
    pub kurz: bool,
    /// Fläche in der Erkennung: Hausgruppe und Nummer darin.
    form: (usize, usize),
}

/// Gedrückter bzw. gezogener Stempel.
struct Zug {
    raum: Option<ElementId>,
    punkt: Vec3,
    form: (usize, usize),
    /// Mauspunkt beim Drücken und Versatz zum Stempelpunkt (Pixel).
    start: (f64, f64),
    versatz: (f64, f64),
    /// Schritt offen (erst ab [`ZIEHEN`] dip Weg).
    offen: bool,
}

/// Offenes Namensfeld.
pub struct Namen {
    pub raum: ElementId,
    pub feld: TextEdit,
    /// Getippter Suchtext; `None` bis zum ersten Tippen (alle Vorschläge).
    such: Option<String>,
    /// Mit Tab gewählte Raumvorlage; Tippen nimmt sie zurück.
    vorlage: Option<Guid>,
    /// Vorschlag unter der Maus bzw. mit Tab gewählt (Index in der Liste).
    pub wahl: Option<usize>,
}

/// Erkannte Flächen des Geschosses je Hausgruppe zum Modellstand `rev`.
struct Formen {
    rev: u64,
    raeume: Raumformen,
    /// Türaufschläge des Geschosses, bei Bedarf.
    tueren: aufschlag::Tueren,
}

impl Formen {
    fn flaeche(&self, (g, i): (usize, usize)) -> Option<&Raumflaeche> {
        self.raeume.gruppen.get(g)?.get(i)
    }
}

/// Wovon die Auslegung abhängt: Sie gilt, solange sich nichts davon ändert.
#[derive(Clone, PartialEq)]
struct Anlass {
    rev: u64,
    geschoss: StoreyId,
    kamera: Camera,
    w: f64,
    h: f64,
    scale: f64,
    theme: u64,
    schrift: (bool, bool),
    gesperrt: Vec<Box>,
    feld: Option<(ElementId, String)>,
}

/// Stempel des aktiven Geschosses mit Maus- und Tastenzustand.
#[derive(Default)]
pub struct Stempel {
    /// Szene, zu der alles Folgende gehört ([`Scene::kennung`]).
    szene: Option<u64>,
    /// Erkannte Flächen mit Grundfläche (mm²), je Modellstand und Geschoss.
    formen: Option<Formen>,
    /// Wofür `bilder` ausgelegt sind.
    ausgelegt: Option<Anlass>,
    /// Zahl der Auslegungen (Tests).
    gelegt: u64,
    pub bilder: Vec<Bild>,
    /// Stempel unter der Maus (Index in `bilder`).
    pub hover: Option<usize>,
    zug: Option<Zug>,
    pub namen: Option<Namen>,
    /// Vorschlagsliste: Kasten (Pixel der Ansicht, ohne Schatten) und
    /// Raumvorlagen.
    pub liste: Option<(Box, Vec<(Guid, String)>)>,
}

/// Ergebnis eines Ereignisses für die App.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ausgang {
    pub consumed: bool,
    pub redraw: bool,
    /// Das Modell hat sich geändert (Paneel neu).
    pub changed: bool,
    /// Diesen Raum wählen.
    pub gewaehlt: Option<ElementId>,
}

/// Textmaße (Pixel) einer Zeile: fett für den Namen, sonst klein.
fn masse(fonts: &Fonts, text: &str, fett: bool, s: f64, t: &Theme) -> (f64, f64) {
    let (f, px) = schrift(fonts, fett, s, t);
    let tw = f.map_or(text.chars().count() as f32 * px * 0.5, |f| {
        f.width(text, px)
    });
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px));
    ((tw.ceil() + 2.0) as f64, (cap * 2.0).ceil() as f64)
}

fn schrift<'a>(fonts: &'a Fonts, fett: bool, s: f64, t: &Theme) -> (Option<&'a Font>, f32) {
    if fett {
        let f = fonts.bold.as_ref().or(fonts.regular.as_ref());
        (f, t.size.font * s as f32)
    } else {
        (fonts.regular.as_ref(), t.size.font_small * s as f32)
    }
}

/// Raumvorlagen zum Suchtext: erst die, deren Name damit beginnt, dann die,
/// deren Name ihn enthält (ohne Groß- und Kleinschreibung).
pub fn vorschlaege(alle: Vec<(Guid, String)>, such: Option<&str>) -> Vec<(Guid, String)> {
    let Some(q) = such.map(|q| q.trim().to_lowercase()) else {
        return alle;
    };
    let (mut v, rest): (Vec<_>, Vec<_>) = alle
        .into_iter()
        .filter(|(_, n)| n.to_lowercase().contains(&q))
        .partition(|(_, n)| n.to_lowercase().starts_with(&q));
    v.extend(rest);
    v
}

/// Kasten (mm, x und y von–bis) des Bildes in der Ebene der Höhe `z`;
/// `None`, wenn eine Bildecke die Ebene nicht trifft.
fn sichtfeld(c: &Camera, w: f64, h: f64, z: f64) -> Option<[f64; 4]> {
    let mut k: Option<[f64; 4]> = None;
    for (x, y) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
        let p = c.plane_point(x, y, w, h, z)?;
        k = Some(match k {
            None => [p.x, p.y, p.x, p.y],
            Some(k) => [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)],
        });
    }
    k
}

/// Modellpunkt unter dem Bildpunkt auf der Höhe `z`.
fn boden(c: &Camera, x: f64, y: f64, w: f64, h: f64, z: f64) -> Vec3 {
    let (o, d) = c.ray(x, y, w, h);
    let t = if d.z.abs() > 1e-9 {
        (z - o.z) / d.z
    } else {
        0.0
    };
    let p = o + d * t;
    vec3(p.x, p.y, 0.0)
}

impl Stempel {
    /// Steht die Maus auf einem Stempel oder wird einer gezogen?
    pub fn is_busy(&self) -> bool {
        self.hover.is_some() || self.zug.is_some()
    }

    /// Wird ein Stempel gezogen (Schritt offen)? Das Paneel ruht dann wie
    /// beim Ziehen einer Wand (Review #112 B1).
    pub fn zieht(&self) -> bool {
        self.zug.as_ref().is_some_and(|z| z.offen)
    }

    /// Eine andere Szene (Neu, Öffnen, Sicherung) beginnt ohne Flächen,
    /// Auslegung, Zug und Namensfeld der vorigen, auch bei gleichem
    /// Modellstand (Review #87 R4).
    fn szene(&mut self, s: &Scene) {
        if self.szene != Some(s.kennung()) {
            *self = Stempel {
                szene: Some(s.kennung()),
                gelegt: self.gelegt,
                ..Default::default()
            };
        }
    }

    /// Holt die Flächen des aktiven Geschosses, wenn sich Modell oder
    /// Geschoss geändert haben; die Szene rechnet dabei nur Hausgruppen
    /// neu, deren Wände oder Räume sich geändert haben (Review #87 B2, R2).
    fn formen(&mut self, s: &Scene) {
        let (m, g) = (s.model(), s.active_storey());
        let rev = m.revision();
        if self
            .formen
            .as_ref()
            .is_none_or(|f| f.rev != rev || f.raeume.geschoss != g)
        {
            self.formen = Some(Formen {
                rev,
                raeume: s.raumgruppen(g),
                tueren: Default::default(),
            });
        }
    }

    /// Kasten des offenen Namensfelds um den Punkt `c` (Pixel): Feld und
    /// darunter die Fläche.
    fn feld_kasten(n: &Namen, flaeche: &str, c: [f64; 2], fonts: &Fonts, s: f64, t: &Theme) -> Box {
        let pad = PAD * s;
        let (nw, _) = masse(fonts, &n.feld.text, false, s, t);
        let fw = (FELD * s).max(nw + 2.0 * t.size.field_pad as f64 * s + 8.0 * s);
        let fh = t.size.field_height as f64 * s;
        let (aw, ah) = masse(fonts, flaeche, false, s, t);
        Box::around(c, (fw.max(aw) + 2.0 * pad, fh + ah + 2.0 * pad))
    }

    /// Legt die Stempel des aktiven Geschosses aus (siehe Modulkopf);
    /// `gesperrt`: Paneele über dem Plan.
    #[allow(clippy::too_many_arguments)]
    pub fn auslegen(
        &mut self,
        s: &Scene,
        c: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        fonts: &Fonts,
        t: &Theme,
        gesperrt: &[Box],
    ) {
        self.szene(s);
        // Nach Rückgängig gibt es den Raum im Namensfeld vielleicht nicht mehr
        if self
            .namen
            .as_ref()
            .is_some_and(|n| s.model().space(n.raum).is_none())
        {
            self.namen = None;
        }
        self.formen(s);
        let anlass = Anlass {
            rev: s.model().revision(),
            geschoss: s.active_storey(),
            kamera: c.clone(),
            w,
            h,
            scale,
            theme: t.rev,
            schrift: (fonts.regular.is_some(), fonts.bold.is_some()),
            gesperrt: gesperrt.to_vec(),
            feld: self.namen.as_ref().map(|n| (n.raum, n.feld.text.clone())),
        };
        if self.ausgelegt.as_ref() != Some(&anlass) {
            self.bilder = self.legen(s, c, w, h, scale, fonts, t, gesperrt, true);
            self.ausgelegt = Some(anlass);
            self.gelegt += 1;
        }
        if self.hover.is_some_and(|i| i >= self.bilder.len()) {
            self.hover = None;
        }
        // Raum des Namensfelds nicht in diesem Geschoss: Feld zu, Name bleibt
        if let Some(n) = &self.namen {
            if !self.bilder.iter().any(|b| b.raum == Some(n.raum)) {
                self.namen = None;
            }
        }
        self.liste = self.liste_auslegen(s, w, h, scale, t);
    }

    /// Die Stempel zu [`Stempel::auslegen`]. Vorab fallen Räume weg, deren
    /// Kasten das Bild nicht berührt oder in denen nicht einmal der kleinste
    /// Stempel Platz hat (nur die Fläche); so kostet die Auslegung je Raum
    /// im Bild, nicht je Raum im Geschoss. `vortest`: aus nur zum Vergleich
    /// in Tests.
    #[allow(clippy::too_many_arguments)]
    fn legen(
        &mut self,
        s: &Scene,
        c: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        fonts: &Fonts,
        t: &Theme,
        gesperrt: &[Box],
        vortest: bool,
    ) -> Vec<Bild> {
        let Some(Formen { raeume, tueren, .. }) = self.formen.as_mut() else {
            return Vec::new();
        };
        let z = s.plan_cut();
        let pt = |p: Vec3| c.project(vec3(p.x, p.y, z), w, h).map(|(x, y)| [x, y]);
        let pad = PAD * scale;
        let feld = self.namen.as_ref().map(|n| n.raum);
        let m = s.model();
        let sicht = sichtfeld(c, w, h, z);
        // Bildpunkte je mm (parallel: überall gleich, keine Richtung mehr)
        let px_mm = c.ortho.map(|half| h / (2.0 * half));
        // Der kleinste Stempel ist mindestens so breit und hoch
        let (_, ah) = masse(fonts, "0", false, scale, t);
        let (aw, _) = masse(fonts, ", m²", false, scale, t);
        let kleinst = ah.min(aw) + 2.0 * pad;
        let passt = |r: &Raumflaeche| {
            let k = &r.kasten;
            !vortest
                || r.form.raum.is_some_and(|id| Some(id) == feld)
                || (sicht
                    .is_none_or(|v| k[0] <= v[2] && v[0] <= k[2] && k[1] <= v[3] && v[1] <= k[3])
                    && px_mm.is_none_or(|p| (k[2] - k[0]).min(k[3] - k[1]) * p >= kleinst))
        };
        let mut folge: Vec<((usize, usize), &Raumflaeche)> = raeume
            .gruppen
            .iter()
            .enumerate()
            .flat_map(|(g, gr)| gr.iter().enumerate().map(move |(i, r)| ((g, i), r)))
            .filter(|(_, r)| passt(r))
            .collect();
        // Das Namensfeld zuerst, dann große Räume vor kleinen
        let rang = |r: &Raumflaeche| {
            (
                r.form.raum.is_none_or(|id| Some(id) != feld),
                -r.grundflaeche,
            )
        };
        folge.sort_by(|a, b| {
            rang(a.1)
                .partial_cmp(&rang(b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let bildflaeche = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        let mut belegt: Vec<Box> = gesperrt.to_vec();
        let mut bilder = Vec::new();
        for (k, r) in folge {
            let (f, a) = (&r.form, r.grundflaeche);
            let name = f
                .raum
                .and_then(|id| m.space(id))
                .map_or_else(|| "Raum".to_string(), |r| r.name.clone());
            let flaeche = format!("{} m²", crate::selection::de(a / 1e6, 2));
            let Some(mitte) = pt(f.stempel) else {
                continue;
            };
            let ring = |kn: &[sk_raum::Kante]| -> Option<Vec<[f64; 2]>> {
                kn.iter().map(|k| pt(k.a)).collect()
            };
            let (Some(umriss), Some(loecher)) = (
                ring(&f.umriss),
                f.loecher
                    .iter()
                    .map(|l| ring(l))
                    .collect::<Option<Vec<_>>>(),
            ) else {
                continue;
            };
            let frei = |b: &Box| {
                inside(&bildflaeche, b)
                    && inside(&umriss, b)
                    && !loecher.iter().any(|l| touches(l, b))
                    && !belegt.iter().any(|o| b.overlaps(o))
            };
            let (nw, nh) = masse(fonts, &name, true, scale, t);
            let (aw, ah) = masse(fonts, &flaeche, false, scale, t);
            let voll = (nw.max(aw) + 2.0 * pad, nh + ah + 2.0 * pad);
            let nur = (aw + 2.0 * pad, ah + 2.0 * pad);
            // Passt nicht einmal die Fläche in den Rahmen des Raums, bleibt er
            // ohne Stempel; Bögen und Ausweichstelle braucht es dann nicht
            let (lo, hi) = umriss
                .iter()
                .fold(([f64::MAX; 2], [f64::MIN; 2]), |(lo, hi), q| {
                    (
                        [lo[0].min(q[0]), lo[1].min(q[1])],
                        [hi[0].max(q[0]), hi[1].max(q[1])],
                    )
                });
            let rahmen = [lo, [hi[0], lo[1]], hi, [lo[0], hi[1]]];
            let rmitte = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
            if f.raum.is_none_or(|id| Some(id) != feld)
                && !inside(&rahmen, &Box::around(rmitte, nur))
            {
                continue;
            }
            // Türbögen nur um einen nicht verschobenen Stempel (G7)
            let auf = if f.stempel == f.mitte {
                tueren.bei(s, k, r)
            } else {
                Vec::new()
            };
            let boegen: Vec<Vec<[f64; 2]>> = auf
                .iter()
                .filter_map(|a| a.umriss().into_iter().map(pt).collect())
                .collect();
            let ohne_bogen = |b: &Box| !boegen.iter().any(|l| touches(l, b));
            let mut stellen = vec![(f.stempel, mitte)];
            if !ohne_bogen(&Box::around(mitte, voll)) {
                let p = tueren.stelle(k, f, &auf);
                stellen.extend(pt(p).map(|q| (p, q)));
            }
            // Frei von den Bögen, sonst an der Mitte wie ohne sie
            let wahl = [(voll, false), (nur, true)]
                .into_iter()
                .find_map(|(g, kurz)| {
                    let da = |&(p, q): &(Vec3, [f64; 2])| (p, q, Box::around(q, g), kurz);
                    let mut v = stellen.iter().map(da);
                    v.find(|w| frei(&w.2) && ohne_bogen(&w.2))
                        .or(Some(da(&stellen[0])).filter(|w| frei(&w.2)))
                });
            let (punkt, kasten, kurz) = match self.namen.as_ref().filter(|n| f.raum == Some(n.raum))
            {
                Some(n) => {
                    let (p, q) = wahl.map_or((f.stempel, mitte), |w| (w.0, w.1));
                    let kasten = Self::feld_kasten(n, &flaeche, q, fonts, scale, t);
                    (p, kasten, false)
                }
                None => match wahl {
                    Some((p, _, kasten, kurz)) => (p, kasten, kurz),
                    None => continue,
                },
            };
            belegt.push(kasten);
            bilder.push(Bild {
                raum: f.raum,
                punkt,
                name,
                flaeche,
                kasten,
                kurz,
                form: k,
            });
        }
        bilder
    }

    /// Vorschlagsliste unter dem Namensfeld (über ihm, wenn unten kein
    /// Platz ist).
    fn liste_auslegen(
        &self,
        sc: &Scene,
        w: f64,
        h: f64,
        s: f64,
        t: &Theme,
    ) -> Option<(Box, Vec<(Guid, String)>)> {
        let n = self.namen.as_ref()?;
        let b = self.bilder.iter().find(|b| b.raum == Some(n.raum))?;
        let v: Vec<(Guid, String)> =
            vorschlaege(sc.model().space_type_choices(), n.such.as_deref())
                .into_iter()
                .take(ZEILEN)
                .collect();
        if v.is_empty() {
            return None;
        }
        let zeile = zeile(s, t);
        let (lw, lh) = (b.kasten.w.max(FELD * s), v.len() as f64 * zeile + 8.0 * s);
        let unten = b.kasten.y + b.kasten.h + 2.0 * s;
        let y = if unten + lh <= h {
            unten
        } else {
            b.kasten.y - 2.0 * s - lh
        };
        let x = b.kasten.x.min(w - lw).max(0.0);
        Some((Box { x, y, w: lw, h: lh }, v))
    }

    /// Stempel unter dem Bildpunkt.
    fn treffer(&self, x: f64, y: f64) -> Option<usize> {
        self.bilder.iter().position(|b| {
            let k = &b.kasten;
            x >= k.x && x < k.x + k.w && y >= k.y && y < k.y + k.h
        })
    }

    /// Zeile der Vorschlagsliste unter dem Bildpunkt.
    fn zeile_bei(&self, x: f64, y: f64, s: f64, t: &Theme) -> Option<usize> {
        let (b, v) = self.liste.as_ref()?;
        if x < b.x || x >= b.x + b.w || y < b.y + 4.0 * s {
            return None;
        }
        let i = ((y - b.y - 4.0 * s) / zeile(s, t)).floor() as usize;
        (i < v.len()).then_some(i)
    }

    /// Namensfeld für den Raum `id` öffnen: Name markiert, Tippen
    /// überschreibt.
    pub fn oeffnen(&mut self, s: &Scene, id: ElementId) {
        self.szene(s);
        if let Some(r) = s.model().space(id) {
            self.namen = Some(Namen {
                raum: id,
                feld: TextEdit::new(&r.name),
                such: None,
                vorlage: None,
                wahl: None,
            });
        }
    }

    /// Namensfeld schließen und den Namen übernehmen; `true`, wenn er sich
    /// geändert hat.
    pub fn ab(&mut self, s: &mut Scene) -> bool {
        self.szene(s);
        match self.namen.take() {
            Some(n) => {
                self.liste = None;
                s.raum_benennen(n.raum, &n.feld.text, n.vorlage)
            }
            None => false,
        }
    }

    /// Esc beim Ziehen: alles wie vorher. `true`, wenn gezogen wurde.
    pub fn escape(&mut self, s: &mut Scene) -> bool {
        self.szene(s);
        match self.zug.take() {
            Some(z) if z.offen => {
                s.rollback();
                true
            }
            _ => false,
        }
    }

    /// Maus im Grundriss. `aktiv`: Grundriss ohne Wandwerkzeug (sonst nimmt
    /// nur ein Klick daneben das Namensfeld zurück).
    #[allow(clippy::too_many_arguments)]
    pub fn handle(
        &mut self,
        e: &Event,
        s: &mut Scene,
        c: &Camera,
        w: f64,
        h: f64,
        scale: f64,
        fonts: &Fonts,
        t: &Theme,
        aktiv: bool,
    ) -> Ausgang {
        self.szene(s);
        let mut out = Ausgang::default();
        if !aktiv && self.is_busy() {
            out.redraw = self.hover.take().is_some();
            if self.escape(s) {
                out.changed = true;
            }
            self.zug = None;
        }
        match *e {
            Event::MouseMove { x, y, .. } => {
                let i = self.zeile_bei(x, y, scale, t);
                if let Some(n) = self.namen.as_mut().filter(|n| i.is_some() && i != n.wahl) {
                    n.wahl = i;
                    out.redraw = true;
                }
                if let Some(z) = self.zug.as_mut() {
                    out.consumed = true;
                    let weg = ((x - z.start.0).powi(2) + (y - z.start.1).powi(2)).sqrt();
                    if !z.offen && weg >= ZIEHEN * scale {
                        let Some(_) = z.raum else {
                            return out;
                        };
                        s.begin("Raumstempel verschoben");
                        z.offen = true;
                    }
                    if let (true, Some(id)) = (z.offen, z.raum) {
                        let p = boden(c, x - z.versatz.0, y - z.versatz.1, w, h, s.plan_cut());
                        let drin = self
                            .formen
                            .as_ref()
                            .and_then(|f| f.flaeche(z.form))
                            .is_some_and(|r| r.form.enthaelt(p));
                        let rev = s.model().revision();
                        if drin && s.raum_stempel(id, p) {
                            z.punkt = p;
                            out.redraw = true;
                            // Nur dieser Stempel rückt, die Flächen und die
                            // übrigen Räume bleiben (Review #87 R2)
                            if let Some(f) = self.formen.as_mut().filter(|f| f.rev == rev) {
                                let (g, i) = z.form;
                                if let Some(gr) = f.raeume.gruppen.get_mut(g) {
                                    let mut v = gr.to_vec();
                                    if let Some(r) = v.get_mut(i) {
                                        r.form.stempel = p;
                                    }
                                    *gr = v.into();
                                }
                                f.rev = s.model().revision();
                            }
                        }
                    }
                } else if aktiv {
                    let hit = self.treffer(x, y);
                    out.redraw |= hit != self.hover;
                    self.hover = hit;
                    out.consumed = hit.is_some();
                }
            }
            Event::MouseLeave => {
                if self.zug.is_none() {
                    out.redraw = self.hover.take().is_some();
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                // Vorschlag angeklickt: übernehmen
                if let Some(i) = self.zeile_bei(x, y, scale, t) {
                    let v = self.liste.as_ref().map(|(_, v)| v[i].clone());
                    if let (Some((g, name)), Some(n)) = (v, self.namen.as_mut()) {
                        n.feld = TextEdit::new(&name);
                        n.vorlage = Some(g);
                    }
                    out.changed = self.ab(s);
                    out.consumed = true;
                    out.redraw = true;
                    return out;
                }
                let hit = self.treffer(x, y).filter(|_| aktiv);
                let im_feld = hit.is_some_and(|i| {
                    let b = &self.bilder[i];
                    self.namen.as_ref().is_some_and(|n| b.raum == Some(n.raum))
                });
                if im_feld {
                    // Klick ins offene Feld setzt die Schreibmarke
                    let r = feld_rect(&self.bilder[hit.unwrap_or_default()].kasten, scale, t);
                    let x0 = widgets::text_field_x(r, scale as f32, t);
                    let (f, px) = schrift(fonts, false, scale, t);
                    if let Some(n) = self.namen.as_mut() {
                        let i = widgets::caret_at(f, &n.feld.text, px, x0, x as f32);
                        n.feld.place(i, false);
                    }
                    out.consumed = true;
                    out.redraw = true;
                    return out;
                }
                if self.namen.is_some() {
                    out.changed = self.ab(s);
                    out.redraw = true;
                }
                if let Some(i) = hit {
                    let b = &self.bilder[i];
                    let mitte = c.project(vec3(b.punkt.x, b.punkt.y, s.plan_cut()), w, h);
                    let (mx, my) = mitte.unwrap_or((x, y));
                    self.zug = Some(Zug {
                        raum: b.raum,
                        punkt: b.punkt,
                        form: b.form,
                        start: (x, y),
                        versatz: (x - mx, y - my),
                        offen: false,
                    });
                    out.consumed = true;
                }
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => {
                if let Some(z) = self.zug.take() {
                    out.consumed = true;
                    out.redraw = true;
                    if z.offen {
                        s.commit();
                        out.changed = true;
                    } else {
                        // Klick: Raum wählen und benennen; eine Fläche ohne
                        // Raum bekommt ihn jetzt
                        let id = match z.raum {
                            Some(id) => id,
                            None => {
                                out.changed = true;
                                s.raum_anlegen(s.active_storey(), z.punkt)
                            }
                        };
                        out.gewaehlt = Some(id);
                        self.oeffnen(s, id);
                    }
                }
            }
            _ => {}
        }
        out
    }
}

/// Ein Kürzel mit **Strg**, das nicht im Namensfeld bleibt: alle Buchstaben
/// außer **Strg+A** (alles wählen) und **Strg+Z** (Getipptes zurück).
pub fn kuerzel(key: Key, mods: Modifiers) -> bool {
    mods.ctrl && matches!(key, Key::Char(c) if c != 'A' && c != 'Z')
}

impl Stempel {
    /// Nimmt das offene Namensfeld (bzw. der gezogene Stempel: Esc) die
    /// Taste? Zeichen kommen als Text ([`Stempel::text`]), ihre Tasten
    /// bleiben im Feld, damit kein Kürzel auslöst (auch die Leertaste nicht,
    /// die an einer Tür den Aufschlag wechselt). Mit **Strg** bleiben nur
    /// **Strg+A** und **Strg+Z** im Feld, die übrigen Kürzel gehen an die App,
    /// die vorher den Namen übernimmt (Bedienbarkeit 39.3, [`kuerzel`]).
    pub fn will(&self, key: Key, mods: Modifiers) -> bool {
        let feld = !kuerzel(key, mods)
            && matches!(
                key,
                Key::Char(_)
                    | Key::Backspace
                    | Key::Delete
                    | Key::Left
                    | Key::Right
                    | Key::Home
                    | Key::End
                    | Key::Escape
                    | Key::Enter
                    | Key::Tab
                    | Key::Other(0x20)
            );
        (key == Key::Escape && self.zug.as_ref().is_some_and(|z| z.offen))
            || (feld && self.namen.is_some())
    }

    /// Taste nach [`Stempel::will`].
    pub fn key(&mut self, key: Key, mods: Modifiers, s: &mut Scene) -> Ausgang {
        self.szene(s);
        let mut out = Ausgang {
            consumed: true,
            redraw: true,
            ..Default::default()
        };
        if key == Key::Escape && self.escape(s) {
            out.changed = true;
            return out;
        }
        let Some(n) = self.namen.as_mut() else {
            return Ausgang::default();
        };
        let getippt = match key {
            Key::Backspace => {
                n.feld.backspace();
                true
            }
            Key::Delete => {
                n.feld.delete();
                true
            }
            Key::Left => {
                n.feld.left(mods.shift);
                false
            }
            Key::Right => {
                n.feld.right(mods.shift);
                false
            }
            Key::Home => {
                n.feld.home(mods.shift);
                false
            }
            Key::End => {
                n.feld.end(mods.shift);
                false
            }
            Key::Char('A') if mods.ctrl => {
                n.feld.select_all();
                false
            }
            Key::Char('Z') if mods.ctrl => n.feld.undo(),
            // Tab: die nächste Vorlage ins Feld
            Key::Tab => {
                let v = vorschlaege(s.model().space_type_choices(), n.such.as_deref());
                let k = v.len().min(ZEILEN);
                if k > 0 {
                    let i = n.wahl.map_or(0, |i| (i + 1) % k);
                    n.wahl = Some(i);
                    n.feld = TextEdit::new(&v[i].1);
                    n.feld.end(false);
                    n.vorlage = Some(v[i].0);
                }
                false
            }
            // Enter: Ist genau eine Vorlage übrig, gilt sie als Nutzung
            // (Bedienbarkeit 40.5); beginnt sie mit dem Getippten, ergänzt
            // sie auch den Namen („Kü“ wird Küche), sonst bleibt er („WC“
            // mit Nutzung Gäste-WC, Review H-176a)
            Key::Enter => {
                if let (None, Some(q)) = (n.vorlage, n.such.as_deref()) {
                    let v = vorschlaege(s.model().space_type_choices(), Some(q));
                    if let [(g, name)] = v.as_slice() {
                        if name.to_lowercase().starts_with(&q.trim().to_lowercase()) {
                            n.feld = TextEdit::new(name);
                        }
                        n.vorlage = Some(*g);
                    }
                }
                out.changed = self.ab(s);
                return out;
            }
            Key::Escape => {
                self.namen = None;
                self.liste = None;
                return out;
            }
            _ => false,
        };
        if getippt {
            n.such = Some(n.feld.text.clone());
            n.wahl = None;
            n.vorlage = None;
        }
        out
    }

    /// Getipptes Zeichen ins offene Namensfeld; `true`, wenn es eins gab.
    pub fn text(&mut self, ch: char) -> bool {
        let Some(n) = self.namen.as_mut().filter(|_| !ch.is_control()) else {
            return false;
        };
        n.feld.insert(&ch.to_string());
        n.such = Some(n.feld.text.clone());
        n.wahl = None;
        n.vorlage = None;
        true
    }

    /// Zeiger: Textcursor über dem offenen Feld, Hand über einem Stempel
    /// und der Vorschlagsliste.
    pub fn cursor(&self, maus: Option<(f64, f64)>) -> Option<sk_platform::Cursor> {
        let (x, y) = maus?;
        if let Some((b, _)) = &self.liste {
            if x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h {
                return Some(sk_platform::Cursor::Hand);
            }
        }
        let i = self.zug.as_ref().map(|_| usize::MAX).or(self.hover)?;
        let feld = self
            .bilder
            .get(i)
            .is_some_and(|b| self.namen.as_ref().is_some_and(|n| b.raum == Some(n.raum)));
        Some(if feld {
            sk_platform::Cursor::IBeam
        } else {
            sk_platform::Cursor::Hand
        })
    }

    /// Was das Bild des Stempels `i` bestimmt und wo es steht (Pixel der
    /// Ansicht, links oben); `gewaehlt`: der gewählte Raum.
    pub fn bild(
        &self,
        i: usize,
        gewaehlt: Option<ElementId>,
        scale: f64,
        rev: u64,
    ) -> Option<(Schluessel, [f64; 2])> {
        let b = self.bilder.get(i)?;
        let feld = self
            .namen
            .as_ref()
            .filter(|n| b.raum == Some(n.raum))
            .map(|n| (n.feld.text.clone(), n.feld.caret, n.feld.anchor));
        let gezogen = self
            .zug
            .as_ref()
            .is_some_and(|z| z.offen && z.raum == b.raum);
        let ton = if b.raum.is_some() && b.raum == gewaehlt {
            2
        } else if self.hover == Some(i) || gezogen {
            1
        } else {
            0
        };
        let k = Schluessel {
            name: b.name.clone(),
            flaeche: b.flaeche.clone(),
            kurz: b.kurz,
            ton,
            feld,
            groesse: (b.kasten.w.ceil() as u32, b.kasten.h.ceil() as u32),
            scale: (scale as f32).to_bits(),
            rev,
        };
        Some((k, [b.kasten.x, b.kasten.y]))
    }

    /// Was das Bild der Vorschlagsliste bestimmt und wo es steht (mit
    /// Schatten).
    pub fn liste_bild(&self, scale: f64, rev: u64) -> Option<(ListenSchluessel, [f64; 2])> {
        let (b, v) = self.liste.as_ref()?;
        let m = SCHATTEN * scale;
        let k = ListenSchluessel {
            eintraege: v.iter().map(|(_, n)| n.clone()).collect(),
            wahl: self.namen.as_ref().and_then(|n| n.wahl),
            groesse: (b.w.ceil() as u32, b.h.ceil() as u32),
            scale: (scale as f32).to_bits(),
            rev,
        };
        Some((k, [b.x - m, b.y - m]))
    }
}

/// Was ein Stempelbild bestimmt: Bleibt er gleich, wird das hochgeladene
/// Bild nur verschoben.
#[derive(Clone, Debug, PartialEq)]
pub struct Schluessel {
    name: String,
    flaeche: String,
    kurz: bool,
    /// 0 ruhig, 1 unter der Maus oder gezogen, 2 Raum gewählt.
    ton: u8,
    /// Offenes Namensfeld: Text, Schreibmarke, anderes Ende der Markierung.
    feld: Option<(String, usize, usize)>,
    groesse: (u32, u32),
    scale: u32,
    rev: u64,
}

/// Was das Bild der Vorschlagsliste bestimmt.
#[derive(Clone, Debug, PartialEq)]
pub struct ListenSchluessel {
    eintraege: Vec<String>,
    wahl: Option<usize>,
    groesse: (u32, u32),
    scale: u32,
    rev: u64,
}

/// Der Stempel als Bild: Name fett in Papiertinte, darunter die Fläche
/// gedimmt, beide mittig; unter der Maus und gewählt zart im Akzent
/// hinterlegt, die Fläche dann in Papiertinte (gedimmt auf dem Akzent unter
/// 4 : 1). Mit offenem Namensfeld steht statt des Namens das Feld.
pub fn malen(k: &Schluessel, fonts: &Fonts, t: &Theme) -> Canvas {
    let s = f32::from_bits(k.scale) as f64;
    let (w, h) = (k.groesse.0 as f64, k.groesse.1 as f64);
    let mut c = Canvas::new(k.groesse.0 as usize, k.groesse.1 as usize);
    if k.ton > 0 {
        let Rgba(r, g, b, _) = t.ui.accent;
        let mut p = Path::new();
        p.rounded_rect(0.0, 0.0, w as f32, h as f32, (4.0 * s) as f32);
        c.fill(&p, Rgba(r, g, b, deckung(k.ton, t)));
    }
    let zeile = |c: &mut Canvas, text: &str, fett: bool, y: f64, farbe: Rgba| -> f64 {
        let (f, px) = schrift(fonts, fett, s, t);
        let (tw, th) = masse(fonts, text, fett, s, t);
        let cap = f.map_or(px * 0.7, |f| f.cap_height(px)) as f64;
        let x = ((w - tw) * 0.5 + 1.0).round() as f32;
        widgets::text(
            c,
            f,
            text,
            px,
            x,
            (y + (th + cap) * 0.5).round() as f32,
            farbe,
        );
        th
    };
    let mut y = PAD * s;
    if let Some((text, caret, anchor)) = &k.feld {
        let r = feld_rect(
            &Box {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            s,
            t,
        );
        let st = FieldState {
            text,
            focus: true,
            caret: Some(*caret),
            select: (caret != anchor).then(|| (*caret.min(anchor), *caret.max(anchor))),
            ..Default::default()
        };
        widgets::text_field(&mut c, fonts, r, &st, s as f32, t);
        y += r.h as f64;
    } else if !k.kurz {
        y += zeile(&mut c, &k.name, true, y, t.ui.paper_text);
    }
    zeile(&mut c, &k.flaeche, false, y, flaechen_farbe(k.ton, t));
    c
}

/// Deckkraft der Akzent-Hinterlegung: 1 unter der Maus, 2 gewählt.
fn deckung(ton: u8, t: &Theme) -> u8 {
    let a = if ton == 2 {
        t.size.stamp_select_alpha
    } else {
        t.size.stamp_hover_alpha
    };
    (a.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Schrift der Fläche: gedimmt auf Papier, auf der Hinterlegung voll.
fn flaechen_farbe(ton: u8, t: &Theme) -> Rgba {
    if ton > 0 {
        t.ui.paper_text
    } else {
        t.ui.paper_text_dim
    }
}

/// Die Vorschlagsliste als Bild: Menüfläche mit Schatten, gewählte Zeile
/// hinterlegt.
pub fn liste_malen(k: &ListenSchluessel, fonts: &Fonts, t: &Theme) -> Canvas {
    let s = f32::from_bits(k.scale) as f64;
    let m = SCHATTEN * s;
    let (w, h) = (k.groesse.0 as f64, k.groesse.1 as f64);
    let mut c = Canvas::new((w + 2.0 * m).ceil() as usize, (h + 2.0 * m).ceil() as usize);
    let r = Rect {
        x: m as f32,
        y: m as f32,
        w: w as f32,
        h: h as f32,
    };
    widgets::panel_filled(&mut c, r, s as f32, t, t.ui.menu_bg);
    let z = zeile(s, t);
    let (f, px) = schrift(fonts, false, s, t);
    let cap = f.map_or(px * 0.7, |f| f.cap_height(px)) as f64;
    for (i, e) in k.eintraege.iter().enumerate() {
        let y = m + 4.0 * s + i as f64 * z;
        if k.wahl == Some(i) {
            c.fill_rect(r.x + 1.0, y as f32, r.w - 2.0, z as f32, t.ui.hover);
        }
        let x = (m + t.size.field_pad as f64 * s).round() as f32;
        widgets::text(
            &mut c,
            f,
            e,
            px,
            x,
            (y + (z + cap) * 0.5).round() as f32,
            t.ui.text,
        );
    }
    c
}

/// Höhe einer Zeile der Vorschlagsliste (Pixel).
fn zeile(s: f64, t: &Theme) -> f64 {
    (t.size.font_small as f64 * 1.9 * s).round()
}

/// Namensfeld im Kasten des offenen Stempels (Pixel).
fn feld_rect(k: &Box, s: f64, t: &Theme) -> Rect {
    let pad = PAD * s;
    Rect {
        x: (k.x + pad) as f32,
        y: (k.y + pad) as f32,
        w: (k.w - 2.0 * pad) as f32,
        h: t.size.field_height * s as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ViewKind;
    use sk_model::{Category, Model, RefSide, DEFAULT_SPACE_GUID};
    use std::f64::consts::FRAC_PI_2;

    /// Guid der Raumvorlage mit dem Namen `name`.
    fn vorlage(name: &str) -> Guid {
        let v = Model::new().space_type_choices();
        v.into_iter().find(|(_, n)| n == name).unwrap().0
    }

    const W: f64 = 1000.0;
    const H: f64 = 700.0;

    /// Kontrast nach WCAG zweier deckender Farben.
    fn kontrast(a: Rgba, b: Rgba) -> f32 {
        let l = |c: Rgba| {
            let k = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * k(c.0) + 0.7152 * k(c.1) + 0.0722 * k(c.2)
        };
        let (x, y) = (l(a), l(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// Name und Fläche bleiben auf Papier und auf der Hinterlegung lesbar:
    /// Papiertinte 4,5 : 1, gedämpft 4 : 1 (wie `blatt_dunkel_papier_hell`).
    #[test]
    fn stempel_lesbar_auf_der_hinterlegung() {
        let t = Theme::dark();
        let paper = t.env.paper_fallback;
        let Rgba(r, g, b, _) = t.ui.accent;
        assert!(kontrast(flaechen_farbe(0, &t), paper) >= 4.0);
        for ton in [1, 2] {
            let a = deckung(ton, &t) as f32 / 255.0;
            let mix = |p: u8, q: u8| (p as f32 * (1.0 - a) + q as f32 * a).round() as u8;
            let grund = Rgba(mix(paper.0, r), mix(paper.1, g), mix(paper.2, b), 255);
            assert!(kontrast(t.ui.paper_text, grund) >= 4.5, "Name, Ton {ton}");
            let k = kontrast(flaechen_farbe(ton, &t), grund);
            assert!(k >= 4.0, "Fläche, Ton {ton}: {k}");
        }
        // Gewählt kräftiger als unter der Maus
        assert!(deckung(2, &t) > deckung(1, &t));
    }

    fn fonts() -> Fonts {
        Fonts {
            regular: None,
            bold: None,
            italic: None,
        }
    }

    /// Haus 10 × 8 m mit Innenwand bei x = 3 m, direkt ins Modell (wie
    /// aus einer Datei vor G3: noch ohne Räume).
    fn szene() -> Scene {
        szene_mit(3000.0)
    }

    /// Wie [`szene`] mit der Innenwand bei `x`.
    fn szene_mit(x: f64) -> Scene {
        let mut m = Model::with_seed(5);
        let st = m.defaults().storey;
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
            .unwrap();
        let pts = [vec3(x, t, 0.0), vec3(x, 8000.0 - t, 0.0)];
        m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
            .unwrap();
        Scene::with_model(m)
    }

    fn kamera(s: &Scene) -> Camera {
        crate::fit_parallel(ViewKind::Plan, s.bounds(), W, H)
    }

    fn auslegen(st: &mut Stempel, s: &Scene, c: &Camera) {
        st.auslegen(s, c, W, H, 1.0, &fonts(), &Theme::dark(), &[]);
    }

    fn mitte(b: &Bild) -> (f64, f64) {
        (b.kasten.x + b.kasten.w * 0.5, b.kasten.y + b.kasten.h * 0.5)
    }

    fn maus(st: &mut Stempel, s: &mut Scene, c: &Camera, e: Event) -> Ausgang {
        st.handle(&e, s, c, W, H, 1.0, &fonts(), &Theme::dark(), true)
    }

    fn klick(st: &mut Stempel, s: &mut Scene, c: &Camera, (x, y): (f64, f64)) -> Ausgang {
        let m = Modifiers::default();
        let b = MouseButton::Left;
        maus(
            st,
            s,
            c,
            Event::MouseDown {
                button: b,
                x,
                y,
                mods: m,
            },
        );
        maus(
            st,
            s,
            c,
            Event::MouseUp {
                button: b,
                x,
                y,
                mods: m,
            },
        )
    }

    /// Die Leertaste bleibt im offenen Namensfeld, sonst wechselte sie an
    /// einer Tür bei der Maus den Aufschlag.
    #[test]
    fn leertaste_bleibt_im_feld() {
        let mut s = szene();
        let c = kamera(&s);
        let mut st = Stempel::default();
        auslegen(&mut st, &s, &c);
        let leer = crate::wall_edit::tuer_edit::LEERTASTE;
        assert!(!st.will(leer, Modifiers::default()));
        let b = mitte(&st.bilder[0]);
        klick(&mut st, &mut s, &c, b);
        assert!(st.namen.is_some());
        assert!(st.will(leer, Modifiers::default()));
    }

    #[test]
    fn vorschlaege_filtern() {
        let alle = Model::new().space_type_choices();
        let namen = |such| -> Vec<String> {
            vorschlaege(alle.clone(), such)
                .into_iter()
                .map(|(_, n)| n)
                .collect()
        };
        assert_eq!(namen(None).len(), 13, "die Werksvorlagen");
        assert_eq!(
            namen(Some("k")),
            ["Küche", "Kinderzimmer", "Technik, Hausanschluss"],
            "erst die Anfänge, dann der Rest"
        );
        assert_eq!(namen(Some(" BA ")), ["Bad"]);
        assert!(namen(Some("xyz")).is_empty());
    }

    /// Eine Datei vor G3: Stempel ohne Raum zeigen „Raum“, ein Klick legt
    /// den Raum an, wählt ihn und öffnet das Namensfeld; Tippen, Tab und
    /// Enter benennen ihn in einem Schritt.
    #[test]
    fn anlegen_benennen_vorschlag() {
        let mut s = szene();
        let c = kamera(&s);
        let mut st = Stempel::default();
        auslegen(&mut st, &s, &c);
        assert_eq!(st.bilder.len(), 2, "{:?}", st.bilder);
        assert!(st
            .bilder
            .iter()
            .all(|b| b.raum.is_none() && b.name == "Raum"));
        assert!(st.bilder.iter().all(|b| !b.kurz));
        // Der große Raum zuerst; Grundfläche mit Putz an jeder Seite
        assert!(
            st.bilder[0].flaeche.ends_with(" m²"),
            "{}",
            st.bilder[0].flaeche
        );
        let gross = st.bilder[0].clone();
        let out = klick(&mut st, &mut s, &c, mitte(&gross));
        let id = out.gewaehlt.expect("gewählt");
        assert!(out.changed && out.consumed);
        assert_eq!(s.model().space(id).unwrap().name, "Raum 1");
        assert_eq!(s.undo_label(), Some("Raum angelegt"));
        let nutzung = |s: &Scene| s.model().space(id).unwrap().nutzung;
        assert_eq!(nutzung(&s), None, "Vorgabe, nicht gewählt");
        assert!(s.model().space_type(DEFAULT_SPACE_GUID).is_some());
        assert_eq!(st.namen.as_ref().map(|n| n.feld.selected()), Some("Raum 1"));
        auslegen(&mut st, &s, &c);
        assert_eq!(st.liste.as_ref().map(|l| l.1.len()), Some(ZEILEN));
        // Tippen überschreibt die Markierung und filtert
        assert!(st.will(Key::Char('K'), Modifiers::default()));
        // Kürzel mit Strg gehen an die App, nur Strg+A und Strg+Z bleiben im
        // Feld (Bedienbarkeit 39.3)
        let strg = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        for (k, bleibt) in [('S', false), ('Y', false), ('A', true), ('Z', true)] {
            assert_eq!(st.will(Key::Char(k), strg), bleibt, "Strg+{k}");
        }
        for ch in "Kü".chars() {
            assert!(st.text(ch));
        }
        auslegen(&mut st, &s, &c);
        assert_eq!(
            st.liste.as_ref().map(|l| l.1.clone()),
            Some(vec![(vorlage("Küche"), "Küche".to_string())])
        );
        let m = Modifiers::default();
        st.key(Key::Tab, m, &mut s);
        assert_eq!(
            st.namen.as_ref().map(|n| n.feld.text.as_str()),
            Some("Küche")
        );
        let out = st.key(Key::Enter, m, &mut s);
        assert!(out.changed && st.namen.is_none());
        assert_eq!(s.model().space(id).unwrap().name, "Küche");
        assert_eq!(s.undo_label(), Some("Raum benannt"));
        assert_eq!(
            nutzung(&s),
            Some(vorlage("Küche")),
            "die Wahl setzt die Nutzung"
        );
        assert!(s.model().space_type(vorlage("Küche")).is_some());
        // Esc lässt den Namen
        auslegen(&mut st, &s, &c);
        let b = st
            .bilder
            .iter()
            .find(|b| b.raum == Some(id))
            .unwrap()
            .clone();
        assert_eq!(b.name, "Küche");
        klick(&mut st, &mut s, &c, mitte(&b));
        st.text('X');
        st.key(Key::Escape, m, &mut s);
        assert!(st.namen.is_none());
        assert_eq!(s.model().space(id).unwrap().name, "Küche");
        assert!(
            !st.will(Key::Char('X'), Modifiers::default()),
            "ohne Feld nimmt der Stempel keine Taste"
        );
        // Ein freier Name lässt die Nutzung stehen, auch nach Tab und Tippen
        let benennen = |st: &mut Stempel, s: &mut Scene, text: &str, tab: bool| {
            auslegen(st, s, &c);
            klick(st, s, &c, mitte(&b));
            if tab {
                st.key(Key::Tab, m, s);
                let strg = Modifiers { ctrl: true, ..m };
                st.key(Key::Char('A'), strg, s);
            }
            for ch in text.chars() {
                st.text(ch);
            }
            st.key(Key::Enter, m, s);
        };
        benennen(&mut st, &mut s, "Speis", true);
        assert_eq!(s.model().space(id).unwrap().name, "Speis");
        assert_eq!(nutzung(&s), Some(vorlage("Küche")));
        // Bleibt beim Tippen genau eine Vorlage, nimmt Enter sie als
        // Nutzung, den Namen nur, wenn sie mit dem Getippten beginnt
        // (Bedienbarkeit 40.5, Review H-176a)
        benennen(&mut st, &mut s, "flur", false);
        assert_eq!(s.model().space(id).unwrap().name, "flur");
        assert_eq!(nutzung(&s), Some(vorlage("Diele, Flur")));
        benennen(&mut st, &mut s, "bad", false);
        assert_eq!(s.model().space(id).unwrap().name, "Bad");
        assert_eq!(nutzung(&s), Some(vorlage("Bad")));
        benennen(&mut st, &mut s, "Kü", false);
        assert_eq!(s.model().space(id).unwrap().name, "Küche");
        assert_eq!(nutzung(&s), Some(vorlage("Küche")));
        // Ein Name, der eine Vorlage als Wort nennt, wählt sie
        benennen(&mut st, &mut s, "Bad OG", false);
        assert_eq!(s.model().space(id).unwrap().name, "Bad OG");
        assert_eq!(nutzung(&s), Some(vorlage("Bad")));
        // Mehrere Vorschläge: Enter lässt den Namen
        benennen(&mut st, &mut s, "Zimmer", false);
        assert_eq!(s.model().space(id).unwrap().name, "Zimmer");
        assert_eq!(nutzung(&s), Some(vorlage("Bad")));
    }

    /// Ziehen verschiebt Stempel und Anker in einem Schritt, nie aus dem
    /// Raum; Esc nimmt es zurück.
    #[test]
    fn ziehen_bleibt_im_raum() {
        let mut s = szene();
        let c = kamera(&s);
        let mut st = Stempel::default();
        auslegen(&mut st, &s, &c);
        let p0 = mitte(&st.bilder[0]);
        let out = klick(&mut st, &mut s, &c, p0);
        let id = out.gewaehlt.unwrap();
        st.key(Key::Escape, Modifiers::default(), &mut s);
        auslegen(&mut st, &s, &c);
        let b = st
            .bilder
            .iter()
            .find(|b| b.raum == Some(id))
            .unwrap()
            .clone();
        let (x, y) = mitte(&b);
        let (mods, button) = (Modifiers::default(), MouseButton::Left);
        maus(&mut st, &mut s, &c, Event::MouseDown { button, x, y, mods });
        assert!(!st.zieht(), "gedrückt, noch nicht gezogen");
        let out = maus(
            &mut st,
            &mut s,
            &c,
            Event::MouseMove {
                x: x + 40.0,
                y,
                mods,
            },
        );
        assert!(out.consumed && out.redraw);
        assert!(st.zieht(), "das Paneel ruht (Review #112 B1)");
        let r = s.model().space(id).unwrap().clone();
        let p = r.stempel.expect("verschoben");
        assert_eq!(r.anker, p, "der Anker geht mit");
        // R2: Nur die Lage dieses Stempels rückt nach, nichts wird neu
        // zugeteilt, und die Auslegung zeigt sie
        let z0 = s.raum_zuteilungen();
        maus(
            &mut st,
            &mut s,
            &c,
            Event::MouseMove {
                x: x + 50.0,
                y,
                mods,
            },
        );
        maus(
            &mut st,
            &mut s,
            &c,
            Event::MouseMove {
                x: x + 40.0,
                y,
                mods,
            },
        );
        auslegen(&mut st, &s, &c);
        assert_eq!(s.raum_zuteilungen(), z0);
        let gezogen = st.bilder.iter().find(|b| b.raum == Some(id)).unwrap();
        assert_eq!(gezogen.punkt, p);
        let neu = s.raumgruppen(s.active_storey());
        let f = neu.flaechen().find(|f| f.form.raum == Some(id)).unwrap();
        assert_eq!(f.form.stempel, p, "wie neu zugeteilt");
        // Weit nach links über die Innenwand: bleibt im Raum
        maus(&mut st, &mut s, &c, Event::MouseMove { x: 5.0, y, mods });
        assert_eq!(s.model().space(id).unwrap().stempel, Some(p));
        maus(
            &mut st,
            &mut s,
            &c,
            Event::MouseUp {
                button,
                x: 5.0,
                y,
                mods,
            },
        );
        assert_eq!(s.undo_label(), Some("Raumstempel verschoben"));
        assert!(!st.zieht());
        auslegen(&mut st, &s, &c);
        let b2 = st.bilder.iter().find(|b| b.raum == Some(id)).unwrap();
        assert!(
            (mitte(b2).0 - (x + 40.0)).abs() < 1.0,
            "{:?} {x}",
            mitte(b2)
        );
        // Esc beim Ziehen: wie vorher
        let (x, y) = mitte(b2);
        maus(&mut st, &mut s, &c, Event::MouseDown { button, x, y, mods });
        maus(
            &mut st,
            &mut s,
            &c,
            Event::MouseMove {
                x: x + 30.0,
                y,
                mods,
            },
        );
        assert!(st.will(Key::Escape, Modifiers::default()));
        assert!(st.key(Key::Escape, mods, &mut s).changed);
        assert_eq!(s.model().space(id).unwrap().stempel, Some(p));
        assert_eq!(s.undo_label(), Some("Raumstempel verschoben"));
    }

    /// Review #87 R2: Ohne Änderung legt `auslegen` nicht neu aus; der
    /// Vortest über die Kästen lässt bei jedem Ausschnitt und Maßstab
    /// dieselben Stempel stehen wie die volle Prüfung.
    /// Review #87 R4: Eine andere Szene mit gleichem Modellstand (etwa eine
    /// ältere Fassung desselben Projekts) zeigt ihre eigenen Stempel; Zug
    /// und Namensfeld der vorigen enden, ohne in der neuen etwas zu ändern.
    #[test]
    fn andere_szene_gleicher_stand() {
        let (mut a, mut b) = (szene_mit(3000.0), szene_mit(7000.0));
        assert_eq!(a.model().revision(), b.model().revision());
        let c = kamera(&a);
        let lage = |st: &Stempel| st.bilder.iter().map(mitte).collect::<Vec<_>>();
        let mut frisch = Stempel::default();
        auslegen(&mut frisch, &b, &c);
        let mut st = Stempel::default();
        auslegen(&mut st, &a, &c);
        assert_eq!(st.bilder.len(), 2);
        assert_ne!(lage(&st), lage(&frisch));
        auslegen(&mut st, &b, &c);
        assert_eq!(lage(&st), lage(&frisch));
        // Namensfeld und angefangener Zug in a
        let p0 = mitte(&st.bilder[0]);
        auslegen(&mut st, &a, &c);
        let p = mitte(&st.bilder[0]);
        assert!(klick(&mut st, &mut a, &c, p).gewaehlt.is_some());
        assert!(st.namen.is_some());
        let (mods, button) = (Modifiers::default(), MouseButton::Left);
        let (x, y) = p;
        maus(&mut st, &mut a, &c, Event::MouseDown { button, x, y, mods });
        maus(
            &mut st,
            &mut a,
            &c,
            Event::MouseMove {
                x: x + 40.0,
                y,
                mods,
            },
        );
        assert!(st.is_busy());
        let rev = b.model().revision();
        assert!(!st.key(Key::Escape, mods, &mut b).changed);
        assert!(!st.ab(&mut b));
        assert!(st.namen.is_none() && !st.is_busy());
        assert_eq!(b.model().revision(), rev, "b bleibt, wie es war");
        auslegen(&mut st, &b, &c);
        assert_eq!(mitte(&st.bilder[0]), p0);
    }

    #[test]
    fn vortest_und_auslegung_im_speicher() {
        let mut m = Model::with_seed(5);
        let g = m.defaults().storey;
        let (aw, iw) = (m.defaults().exterior_wall, m.defaults().interior_wall);
        let t: f64 = m.wall_layers(aw).iter().map(|l| l.thickness).sum();
        for i in 0..9 {
            let (ox, oy) = ((i % 3) as f64 * 15000.0, (i / 3) as f64 * 12000.0);
            let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
            let pts = [
                p(0.0, 0.0),
                p(0.0, 8000.0),
                p(10000.0, 8000.0),
                p(10000.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, g, aw, Category::ExteriorWall)
                .unwrap();
            let x = 2000.0 + 700.0 * i as f64;
            let pts = [p(x, t), p(x, 8000.0 - t)];
            m.add_wall_run(&pts, false, RefSide::Center, g, iw, Category::InteriorWall)
                .unwrap();
        }
        sk_raum::abgleichen(&mut m, &[g]);
        let s = Scene::with_model(m);
        let mut st = Stempel::default();
        let c = kamera(&s);
        auslegen(&mut st, &s, &c);
        let n = st.gelegt;
        auslegen(&mut st, &s, &c);
        assert_eq!(st.gelegt, n, "nichts geändert");
        let mut gezeigt = 0;
        for half in [1500.0, 4000.0, 9000.0, 20000.0, 60000.0, 200000.0] {
            for (x, y) in [
                (0.0, 0.0),
                (5000.0, 4000.0),
                (17000.0, 15000.0),
                (36000.0, 30000.0),
            ] {
                let c = Camera::parallel(vec3(x, y, 0.0), FRAC_PI_2, -FRAC_PI_2, half);
                auslegen(&mut st, &s, &c);
                assert_eq!(st.gelegt, n + 1, "neue Ansicht");
                let ohne = st.legen(&s, &c, W, H, 1.0, &fonts(), &Theme::dark(), &[], false);
                assert_eq!(st.bilder, ohne, "{half} {x} {y}");
                gezeigt += ohne.len();
                st.ausgelegt = None;
                st.gelegt = n;
            }
        }
        assert!(gezeigt > 20, "{gezeigt}");
    }

    /// Ein Schritt mit Wänden legt die Räume an; der größte neue wartet auf
    /// sein Namensfeld, nach Rückgängig nicht mehr.
    #[test]
    fn neuer_raum_fuer_das_namensfeld() {
        let mut s = Scene::with_model(Model::with_seed(3));
        let st = s.model().defaults().storey;
        s.edit_model("Haus", |m| {
            let aw = m.defaults().exterior_wall;
            let pts = [
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ];
            m.add_wall_run(&pts, true, RefSide::Left, st, aw, Category::ExteriorWall)
                .is_some()
        });
        let id = s.neuer_raum().expect("neu");
        assert!(s.neuer_raum().is_none(), "einmal abholen");
        assert_eq!(
            s.model().space(id).unwrap().name,
            "Raum 1",
            "im aktiven Geschoss, nicht das jüngere OG"
        );
        s.edit_model("Haus", |m| {
            let iw = m.defaults().interior_wall;
            let pts = [vec3(3000.0, 300.0, 0.0), vec3(3000.0, 7700.0, 0.0)];
            m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
                .is_some()
        });
        let teil = s.neuer_raum().expect("Teilen legt einen an");
        assert_eq!(s.model().space(teil).unwrap().name, "Raum 3");
        s.edit_model("Haus", |m| {
            let iw = m.defaults().interior_wall;
            let pts = [vec3(6000.0, 300.0, 0.0), vec3(6000.0, 7700.0, 0.0)];
            m.add_wall_run(&pts, false, RefSide::Center, st, iw, Category::InteriorWall)
                .is_some()
        });
        s.undo();
        assert!(s.neuer_raum().is_none(), "zurückgenommen");
    }

    /// Bild: Größe wie der Kasten, gewählt im Akzent hinterlegt, und neu
    /// gemalt wird nur, wenn sich der Schlüssel ändert.
    #[test]
    fn bild_und_schluessel() {
        let mut s = szene();
        let c = kamera(&s);
        let mut st = Stempel::default();
        auslegen(&mut st, &s, &c);
        let (k, at) = st.bild(0, None, 1.0, 0).unwrap();
        assert_eq!(at, [st.bilder[0].kasten.x, st.bilder[0].kasten.y]);
        let bild = malen(&k, &fonts(), &Theme::dark());
        assert_eq!(
            (bild.width as u32, bild.height as u32),
            (
                st.bilder[0].kasten.w.ceil() as u32,
                st.bilder[0].kasten.h.ceil() as u32
            )
        );
        assert!(
            bild.to_rgba8().iter().skip(3).step_by(4).all(|a| *a == 0),
            "ohne Schrift leer"
        );
        let p0 = mitte(&st.bilder[0]);
        let out = klick(&mut st, &mut s, &c, p0);
        st.key(Key::Escape, Modifiers::default(), &mut s);
        auslegen(&mut st, &s, &c);
        let i = st
            .bilder
            .iter()
            .position(|b| b.raum == out.gewaehlt)
            .unwrap();
        let (k2, _) = st.bild(i, out.gewaehlt, 1.0, 0).unwrap();
        assert_ne!(k, k2);
        let bild = malen(&k2, &fonts(), &Theme::dark());
        assert!(
            bild.to_rgba8().iter().skip(3).step_by(4).any(|a| *a > 0),
            "hinterlegt"
        );
        assert_eq!(
            st.bild(i, out.gewaehlt, 1.0, 0).unwrap().0,
            k2,
            "gleich bleibt gleich"
        );
    }
}

#[cfg(test)]
#[path = "stempel_istbilder.rs"]
mod istbilder;

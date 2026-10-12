//! Raumstempel im Grundriss (Grundriss G4, plan.md §4.4): je Raum des
//! aktiven Geschosses der Name und darunter die Grundfläche „12,34 m²“ an
//! der fertigen Oberfläche ([`sk_raum::flaechen::grundflaeche`], ohne
//! Faktor), wie gedruckt auf dem Papier des Grundrisses.
//!
//! Platz: Der Stempel steht an seinem Punkt (verschoben, sonst die Mitte
//! des größten Kreises im Raum), ganz im Raum, nicht unter einem Paneel und
//! nicht über einem anderen Stempel; passt er so nicht, nur die Fläche,
//! sonst entfällt er. Größere Räume gehen vor, das offene Namensfeld vor
//! allen.
//!
//! Bedienung, nicht bei eingeschaltetem Wandwerkzeug: Ziehen verschiebt den
//! Stempel innerhalb des Raums, ein Schritt „Raumstempel verschoben“; ein
//! Klick wählt den Raum und öffnet sein Namensfeld. Einen neu erkannten Raum
//! öffnet die App selbst. Der Name ist markiert, Tippen überschreibt;
//! darunter Vorschläge (feste Liste bis zum Raumkatalog des Raumbuchs),
//! gefiltert nach dem Getippten. Tab übernimmt den nächsten Vorschlag ins
//! Feld, ein Klick einen bestimmten. Enter übernimmt („Raum benannt“), ein
//! Klick daneben ebenso, Esc lässt den Namen.

use crate::camera::Camera;
use crate::scene::Scene;
use crate::terrace_label::{inside, touches, Box, PAD};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, StoreyId};
use sk_paint::{font::Font, Canvas, Path, Rgba};
use sk_platform::{Event, Key, Modifiers, MouseButton};
use sk_raum::RaumForm;
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, FieldState, Fonts, Rect};

/// Vorschläge für den Namen, bis der Raumkatalog des Raumbuchs sie liefert.
pub const VORSCHLAEGE: [&str; 20] = [
    "Wohnen",
    "Essen",
    "Küche",
    "Schlafen",
    "Kind",
    "Bad",
    "WC",
    "Dusche",
    "Diele",
    "Flur",
    "Garderobe",
    "Abstellraum",
    "Speisekammer",
    "Hauswirtschaft",
    "Arbeiten",
    "Gast",
    "Ankleide",
    "Technik",
    "Heizung",
    "Treppe",
];
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
    /// Index der Fläche in der Erkennung.
    form: usize,
}

/// Gedrückter bzw. gezogener Stempel.
struct Zug {
    raum: Option<ElementId>,
    punkt: Vec3,
    form: usize,
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
    /// Vorschlag unter der Maus bzw. mit Tab gewählt (Index in der Liste).
    pub wahl: Option<usize>,
}

/// Erkannte Flächen mit Grundfläche (mm²) zu Modellstand und Geschoss.
type Formen = (u64, StoreyId, Vec<(RaumForm, f64)>);

/// Stempel des aktiven Geschosses mit Maus- und Tastenzustand.
#[derive(Default)]
pub struct Stempel {
    /// Erkannte Flächen mit Grundfläche (mm²), je Modellstand und Geschoss.
    formen: Option<Formen>,
    pub bilder: Vec<Bild>,
    /// Stempel unter der Maus (Index in `bilder`).
    pub hover: Option<usize>,
    zug: Option<Zug>,
    pub namen: Option<Namen>,
    /// Vorschlagsliste: Kasten (Pixel der Ansicht, ohne Schatten) und
    /// Einträge.
    pub liste: Option<(Box, Vec<&'static str>)>,
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

/// Vorschläge zum Suchtext: erst die, die damit beginnen, dann die, die ihn
/// enthalten (ohne Groß- und Kleinschreibung).
pub fn vorschlaege(such: Option<&str>) -> Vec<&'static str> {
    let Some(q) = such.map(|q| q.trim().to_lowercase()) else {
        return VORSCHLAEGE.to_vec();
    };
    let klein = |v: &str| v.to_lowercase();
    let mut v: Vec<&'static str> = VORSCHLAEGE
        .iter()
        .copied()
        .filter(|v| klein(v).starts_with(&q))
        .collect();
    v.extend(
        VORSCHLAEGE
            .iter()
            .copied()
            .filter(|v| !klein(v).starts_with(&q) && klein(v).contains(&q)),
    );
    v
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

    /// Erkennt die Flächen des aktiven Geschosses neu, wenn sich Modell
    /// oder Geschoss geändert haben.
    fn formen(&mut self, s: &Scene) {
        let (m, g) = (s.model(), s.active_storey());
        let rev = m.revision();
        if self
            .formen
            .as_ref()
            .is_none_or(|(r, gg, _)| *r != rev || *gg != g)
        {
            let f = sk_raum::raeume(m, g)
                .into_iter()
                .map(|f| {
                    let a = sk_raum::flaechen::grundflaeche(&f);
                    (f, a)
                })
                .collect();
            self.formen = Some((rev, g, f));
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
        // Nach Rückgängig gibt es den Raum im Namensfeld vielleicht nicht mehr
        if self
            .namen
            .as_ref()
            .is_some_and(|n| s.model().space(n.raum).is_none())
        {
            self.namen = None;
        }
        let z = s.plan_cut();
        let pt = |p: Vec3| c.project(vec3(p.x, p.y, z), w, h).map(|(x, y)| [x, y]);
        let pad = PAD * scale;
        let feld = self.namen.as_ref().map(|n| n.raum);
        self.formen(s);
        let formen = self.formen.as_ref().map_or(&[][..], |f| &f.2[..]);
        let m = s.model();
        // Das Namensfeld zuerst, dann große Räume vor kleinen
        let mut folge: Vec<usize> = (0..formen.len()).collect();
        let rang = |k: usize| {
            (
                formen[k].0.raum.is_none_or(|r| Some(r) != feld),
                -formen[k].1,
            )
        };
        folge.sort_by(|a, b| {
            rang(*a)
                .partial_cmp(&rang(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let bildflaeche = [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]];
        let mut belegt: Vec<Box> = gesperrt.to_vec();
        let mut bilder = Vec::new();
        for k in folge {
            let (f, a) = &formen[k];
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
            let voll = Box::around(mitte, (nw.max(aw) + 2.0 * pad, nh + ah + 2.0 * pad));
            let nur = Box::around(mitte, (aw + 2.0 * pad, ah + 2.0 * pad));
            let (kasten, kurz) = match self.namen.as_ref().filter(|n| f.raum == Some(n.raum)) {
                Some(n) => (
                    Self::feld_kasten(n, &flaeche, mitte, fonts, scale, t),
                    false,
                ),
                None if frei(&voll) => (voll, false),
                None if frei(&nur) => (nur, true),
                None => continue,
            };
            belegt.push(kasten);
            bilder.push(Bild {
                raum: f.raum,
                punkt: f.stempel,
                name,
                flaeche,
                kasten,
                kurz,
                form: k,
            });
        }
        if self.hover.is_some_and(|i| i >= bilder.len()) {
            self.hover = None;
        }
        // Raum des Namensfelds nicht in diesem Geschoss: Feld zu, Name bleibt
        if let Some(n) = &self.namen {
            if !bilder.iter().any(|b| b.raum == Some(n.raum)) {
                self.namen = None;
            }
        }
        self.bilder = bilder;
        self.liste = self.liste_auslegen(w, h, scale, t);
    }

    /// Vorschlagsliste unter dem Namensfeld (über ihm, wenn unten kein
    /// Platz ist).
    fn liste_auslegen(
        &self,
        w: f64,
        h: f64,
        s: f64,
        t: &Theme,
    ) -> Option<(Box, Vec<&'static str>)> {
        let n = self.namen.as_ref()?;
        let b = self.bilder.iter().find(|b| b.raum == Some(n.raum))?;
        let v: Vec<&'static str> = vorschlaege(n.such.as_deref())
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
        if let Some(r) = s.model().space(id) {
            self.namen = Some(Namen {
                raum: id,
                feld: TextEdit::new(&r.name),
                such: None,
                wahl: None,
            });
        }
    }

    /// Namensfeld schließen und den Namen übernehmen; `true`, wenn er sich
    /// geändert hat.
    pub fn ab(&mut self, s: &mut Scene) -> bool {
        match self.namen.take() {
            Some(n) => {
                self.liste = None;
                s.raum_benennen(n.raum, &n.feld.text)
            }
            None => false,
        }
    }

    /// Esc beim Ziehen: alles wie vorher. `true`, wenn gezogen wurde.
    pub fn escape(&mut self, s: &mut Scene) -> bool {
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
                            .and_then(|(_, _, f)| f.get(z.form))
                            .is_some_and(|(f, _)| f.enthaelt(p));
                        if drin && s.raum_stempel(id, p) {
                            z.punkt = p;
                            out.redraw = true;
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
                    let v = self.liste.as_ref().map(|(_, v)| v[i]);
                    if let (Some(v), Some(n)) = (v, self.namen.as_mut()) {
                        n.feld = TextEdit::new(v);
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

impl Stempel {
    /// Nimmt das offene Namensfeld (bzw. der gezogene Stempel: Esc) die
    /// Taste? Zeichen kommen als Text ([`Stempel::text`]), ihre Tasten
    /// bleiben im Feld, damit kein Kürzel auslöst.
    pub fn will(&self, key: Key) -> bool {
        let feld = matches!(
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
        );
        (key == Key::Escape && self.zug.as_ref().is_some_and(|z| z.offen))
            || (feld && self.namen.is_some())
    }

    /// Taste nach [`Stempel::will`].
    pub fn key(&mut self, key: Key, mods: Modifiers, s: &mut Scene) -> Ausgang {
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
            // Tab: der nächste Vorschlag ins Feld
            Key::Tab => {
                let v = vorschlaege(n.such.as_deref());
                let k = v.len().min(ZEILEN);
                if k > 0 {
                    let i = n.wahl.map_or(0, |i| (i + 1) % k);
                    n.wahl = Some(i);
                    n.feld = TextEdit::new(v[i]);
                    n.feld.end(false);
                }
                false
            }
            Key::Enter => {
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
            eintraege: v.clone(),
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
    eintraege: Vec<&'static str>,
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
    use sk_model::{Category, Model, RefSide};

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
        let pts = [vec3(3000.0, t, 0.0), vec3(3000.0, 8000.0 - t, 0.0)];
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

    #[test]
    fn vorschlaege_filtern() {
        assert_eq!(vorschlaege(None).len(), VORSCHLAEGE.len());
        assert_eq!(
            vorschlaege(Some("k")),
            ["Küche", "Kind", "Speisekammer", "Ankleide", "Technik"],
            "erst die Anfänge, dann der Rest"
        );
        assert_eq!(vorschlaege(Some(" BA ")), ["Bad"]);
        assert!(vorschlaege(Some("xyz")).is_empty());
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
        assert_eq!(st.namen.as_ref().map(|n| n.feld.selected()), Some("Raum 1"));
        auslegen(&mut st, &s, &c);
        assert_eq!(st.liste.as_ref().map(|l| l.1.len()), Some(ZEILEN));
        // Tippen überschreibt die Markierung und filtert
        assert!(st.will(Key::Char('K')));
        for ch in "Kü".chars() {
            assert!(st.text(ch));
        }
        auslegen(&mut st, &s, &c);
        assert_eq!(st.liste.as_ref().map(|l| l.1.clone()), Some(vec!["Küche"]));
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
            !st.will(Key::Char('X')),
            "ohne Feld nimmt der Stempel keine Taste"
        );
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
        let r = s.model().space(id).unwrap().clone();
        let p = r.stempel.expect("verschoben");
        assert_eq!(r.anker, p, "der Anker geht mit");
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
        assert!(st.will(Key::Escape));
        assert!(st.key(Key::Escape, mods, &mut s).changed);
        assert_eq!(s.model().space(id).unwrap().stempel, Some(p));
        assert_eq!(s.undo_label(), Some("Raumstempel verschoben"));
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

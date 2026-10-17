//! Geländeplatte in der App (Gelände G0, plan-gelaendemodul §4.6): statt
//! der Geländelinie eine ebene Platte um jedes Gebäude. In 3D ein Körper
//! mit Rasen oben und Erdreich an den Seiten, der auf dem Boden steht; im
//! Schnitt ihre Schnittfläche mit Erdreich-Schraffur; in den Ansichten ihr
//! Band (Oberkante mit dem Stift „Gelände“, Unterkante und Enden fein). Sie
//! ist kein Bauteil: kein Hover und im Grundriss keine Auswahl (sie läge
//! sonst unter jeder Maßkette); gewählt wird sie im Objektbaum oder mit
//! einem Klick in 3D (Review #212 B1), im Paneel stehen dann Dicke und
//! Abstand ums Haus. Mit Höhenaufmaß steht statt der Platte das echte
//! Gelände da ([`crate::gelaende_netz`], G1).

pub(crate) mod bodenaufbau;

use crate::draw_table::{fallback_look, DrawTable, MatLook, Stroke};
use crate::gelaende_app::unterlage::{Schluessel, Striche};
use crate::gelaende_netz::{self, Gelaende};
use crate::selection::de;
use crate::ui::{kote_text, Field, FieldRow, Props, Section, ViewKind};
use crate::App;
use bodenaufbau::Bodenaufbau;
use sk_math::{polygon, vec3, Vec3};
use sk_model::solid::{right_of, SectionFrame};
use sk_model::{edge_kind, LevelKind, Model, PlateInput, Solid, TerrainPlate};
use sk_model::{PLATE_MARGIN, PLATE_THICK};
use sk_render::{Helper, MeshData};
use sk_ui::theme::Theme;
use std::collections::HashMap;
use std::sync::{mpsc, Arc};

/// Platz der Platte im Renderer (neben Modell, Vorschau, Live und Geist).
pub(crate) const MESH_PLATE: usize = 4;

/// Rand der Gelände-Rechtecke um die Wandzüge eines Gebäudes (mm), für
/// „unter Gelände“ in den Ansichten (S11): deckt Dämmung, Schürze und
/// Sockel.
pub(crate) const TERRAIN_MARGIN: f64 = 1500.0;

/// Höchstens so lange je Durchlauf der Hauptschleife folgen nach einem
/// Gelände-Zug die übrigen Gebäude (Review #123 F1; Loslassen 8 ms).
pub(crate) const NACHZUG: std::time::Duration = std::time::Duration::from_millis(6);

/// Schlüssel der Kacheln des Lageplans im Renderer zu ihren Strecken
/// ([`crate::gelaende_netz::Kachel::key`]), Zeichentabelle, Farbschema
/// und Maßstab (`stil`, [`stil_key`]).
pub(crate) fn unterlage_keys(g: &Gelaende, stil: u64) -> Vec<u64> {
    g.unterlage
        .iter()
        .map(|k| k.key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ stil)
        .collect()
}

/// Prüfsumme von Zeichentabelle, Farbschema und Maßstab.
pub(crate) fn stil_key(t: &DrawTable, scale: f32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (t.rev, t.theme_rev, scale.to_bits()).hash(&mut h);
    h.finish()
}

/// Gelände-Rechtecke für den Renderer.
pub(crate) fn terrain_boxes(patches: &[([f64; 4], f64)]) -> Vec<([f32; 4], f32)> {
    patches
        .iter()
        .map(|(b, z)| (b.map(|v| v as f32), *z as f32))
        .collect()
}

/// Was das Netz außer den Platten bestimmt: Ansicht, Schnittebene,
/// Schlüssel von Rasen und Erdreich, ausgeblendet, Hilfsnetz in 3D
/// (Jörns §4.3a).
pub(crate) type Sicht = (ViewKind, Option<(Vec3, Vec3)>, [u16; 3], bool, bool);

/// Was im Renderer liegt: Fassung der Platten und die [`Sicht`], die
/// Schnittebene als Bits.
type NetzKey = (u64, ViewKind, Option<[u64; 6]>, [u16; 3], bool, bool);

/// Fassung, Ziffernhöhe, Zeichentabelle und Farben der Höhenzahlen.
type ZahlenKey = (u64, u64, u64, u64);

fn netz_key(fassung: u64, s: &Sicht) -> NetzKey {
    (fassung, s.0, s.1.map(bits), s.2, s.3, s.4)
}

/// Neue Platten, ihre Eingaben, das Gelände und ihr Netz zur Sicht beim
/// Start, dazu die im Faden abgebildeten Linien der Unterlage.
type Neues = (
    Arc<PlateInput>,
    Vec<TerrainPlate>,
    Option<Gelaende>,
    MeshData,
    Option<(Schluessel, Arc<Striche>)>,
    Arc<Bodenaufbau>,
);

/// Was der Faden zurückgibt.
enum Ergebnis {
    /// Die Eingaben sind dieselben wie die der stehenden Platten (etwa
    /// nach Esc oder einer Änderung, die die Platte nicht berührt).
    Gleich,
    /// Neue Platten (in einer Kiste, das Gelände ist groß).
    Neu(Box<Neues>),
}

/// Ein Nachzug im eigenen Faden: Modellstand und Sicht beim Start.
struct Job {
    rev: u64,
    sicht: Sicht,
    rx: mpsc::Receiver<Ergebnis>,
}

/// Ecken der Kacheln des Lageplans zu ihrem Schlüssel, wie der Renderer
/// sie hochlädt ([`sk_render::helper_ecken`]).
pub(crate) type Ecken = HashMap<u64, Vec<[f32; 23]>>;

/// Höchstens so viel lädt ein Bild von der Unterlage hoch (Bytes, mindestens
/// eine Kachel); der Rest folgt in den nächsten Bildern (Review #139 N3:
/// alle Kacheln in einem Bild 22 ms unter llvmpipe).
pub(crate) const HOCHLADEN_JE_BILD: usize = 4 << 20;

/// Was der Renderer in diesem Bild mit der Unterlage tun soll
/// ([`Platte::unterlage_im_bild`]): Kacheln in den Vorrat laden
/// ([`sk_render::Renderer::unterlage_laden`]) und, wenn alle da sind, die
/// gezeigten Kacheln tauschen ([`sk_render::Renderer::set_unterlage`]).
#[derive(Debug, Default)]
pub(crate) struct Hochladen {
    pub(crate) laden: Vec<(u64, Vec<[f32; 23]>)>,
    pub(crate) keys: Option<Vec<u64>>,
}

/// Gelände-Rechtecke (x0, y0, x1, y1) mit Höhe, wie der Renderer sie nimmt.
type Kisten = Vec<([f32; 4], f32)>;

/// Wovon die Linien abhängen: Fassung der Platten, Ansicht, Schnittebene
/// und Modellgrenzen (Bits), Maßstab (Bits), Stand der Zeichentabelle und
/// des Farbschemas. Der Zoom (mm je Bildpunkt) zählt nur für die
/// Punktmarken (Review #129 B3).
type LinienKey = (
    u64,
    ViewKind,
    Option<[u64; 6]>,
    Option<[u64; 6]>,
    u32,
    u64,
    u64,
);

/// Linien zu ihrem Schlüssel; ab der Stelle `marken.0` die Punktmarken
/// zum Zoom `marken.1` (mm je Bildpunkt als Bits).
struct Linien {
    key: LinienKey,
    helfer: Vec<Helper>,
    marken: (usize, Option<u64>),
}

/// Platten des Modells und was davon gezeigt wird.
#[derive(Default)]
pub(crate) struct Platte {
    /// Modellstand, zu dem `platten` gehört.
    stand: Option<u64>,
    /// Zählt, wenn `platten` wirklich neu sind (Schlüssel des Netzes).
    fassung: u64,
    platten: Vec<TerrainPlate>,
    /// Eingaben zu `platten`, mit denen der Faden vergleicht.
    eingabe: Option<Arc<PlateInput>>,
    /// Gelände aus dem Höhenaufmaß; statt der Platten, wenn da.
    gelaende: Option<Arc<Gelaende>>,
    /// Bodenaufbau unter den Platten zum Modellstand.
    aufbau: Arc<Bodenaufbau>,
    netz: Option<NetzKey>,
    /// Netz aus dem Faden zu seinem Schlüssel, bis es hochgeladen ist.
    fertig: Option<(NetzKey, MeshData)>,
    job: Option<Job>,
    /// Mit Aufmaß: Netz zur Sicht im eigenen Faden (Körper, Schnitt), bis
    /// es fertig ist (Review #129 B4).
    netz_job: Option<(NetzKey, mpsc::Receiver<MeshData>)>,
    /// Der Körper in 3D zu seinem Schlüssel, gemerkt für den Rückweg nach
    /// einem Sichtwechsel.
    koerper: Option<(NetzKey, Arc<MeshData>)>,
    /// Das Paneel „Eigenschaften“ zeigt die Platte (zum Stand der Platten).
    props: Option<u64>,
    /// Gelände-Rechtecke für „unter Gelände“ zum Modellstand.
    kisten: Option<(u64, Kisten)>,
    /// Leiste „Gelände“ (G1).
    pub(crate) leiste: crate::gelaende_app::Zustand,
    /// Griffe der Grenze bei offener Leiste (I3).
    pub(crate) griffe: crate::gelaende_flaeche_app::griffe::Griffe,
    /// Höhenpunkte im Grundriss (I4).
    pub(crate) punkte: crate::gelaende_flaeche_app::punkte::Punkte,
    /// Der Lageplan der Unterlage, gelesen (G1c).
    pub(crate) unterlage: crate::gelaende_app::Unterlage,
    /// Linien in Ansicht und Schnitt.
    linien: Option<Linien>,
    /// Höhenzahlen im Grundriss als Tinte, zu Gelände, Ziffernhöhe und
    /// Zeichentabelle ([`App::gelaende_zahlen`]), dazu ihr Inhalt
    /// ([`gelaende_netz::zahlen_inhalt`]).
    zahlen: Option<(ZahlenKey, u64, crate::bemassung::Teil)>,
    /// Der Lageplan kam nach dem letzten Nachzug: das Gelände zieht mit
    /// der Unterlage nach, auch beim selben Modellstand.
    mit_plan: bool,
    /// Schlüssel der Kacheln, die der Renderer zeigt, und die im Vorrat;
    /// die Ecken neuer Kacheln aus einem eigenen Faden, bis sie fertig sind
    /// (Review #139 N2), dann je Bild ein Teil davon (N3).
    im_renderer: Vec<u64>,
    im_vorrat: std::collections::HashSet<u64>,
    ecken_job: Option<mpsc::Receiver<Ecken>>,
    wartend: Option<(Vec<u64>, Ecken)>,
    /// Der Renderer hat, was `im_renderer` sagt; eine neue Platte (Neu,
    /// Öffnen) weiß es nicht und leert ihn im ersten Bild (Review #139 N3).
    renderer_bekannt: bool,
    /// Zuletzt gemeldete Nummer von [`sk_model::StripNote`].
    pub(crate) streifen_gemeldet: u64,
}

impl Platte {
    /// Die Platten zum Modell; gerechnet nur, wenn es sich geändert hat.
    /// Solange ein Schritt offen ist (`halten`: jeder Zug, auch Gelände,
    /// Höhen und Öffnungen ohne Live-Netz, Review W34 B1) bleiben sie stehen
    /// und folgen beim Loslassen, statt je Mausbewegung alle Häuser neu zu
    /// rechnen.
    pub(crate) fn platten(&mut self, m: &Model, halten: bool) -> &[TerrainPlate] {
        self.abgleich(m, halten);
        &self.platten
    }

    /// Das echte Gelände zum Modell, wenn ein Aufmaß da ist; im Zug wie
    /// [`Platte::platten`].
    pub(crate) fn gelaende(&mut self, m: &Model, halten: bool) -> Option<&Gelaende> {
        self.abgleich(m, halten);
        self.gelaende.as_deref()
    }

    fn abgleich(&mut self, m: &Model, halten: bool) {
        let rev = m.revision();
        if self.stand != Some(rev) && !(halten && self.stand.is_some()) {
            // ein laufender Nachzug brächte einen älteren Stand
            self.job = None;
            self.netz_job = None;
            self.stand = Some(rev);
            let e = m.terrain_plate_input();
            // ohne gelesenen Plan ohne Unterlage; sie folgt im Nachzug
            let u = &mut self.unterlage;
            let g = Gelaende::aus(m).map(|g| g.mit_unterlage(&u.linien(m)));
            let neu = self.eingabe.as_deref() != Some(&e);
            if neu {
                self.platten = e.build();
                self.eingabe = Some(Arc::new(e));
            }
            let ba = Bodenaufbau::aus(m);
            let anders = *self.aufbau != ba;
            if anders {
                self.aufbau = Arc::new(ba);
            }
            // mit Aufmaß zählt jede Änderung (das Gelände spart die Häuser aus)
            if neu || anders || g.is_some() || self.gelaende.is_some() {
                self.fassung += 1;
            }
            self.gelaende = g.map(Arc::new);
        }
    }

    /// Für das Bild: die Platten, wie sie stehen; gerechnet nur, wenn es
    /// noch keine gibt. Einer Änderung folgen sie im Leerlauf
    /// ([`Platte::nachziehen`]), nicht im Loslassen selbst: Bei 1000
    /// Häusern kostete das Loslassen sonst 13 bis 29 ms statt höchstens 8
    /// (Review #119 B4).
    pub(crate) fn im_bild(&mut self, m: &Model) -> &[TerrainPlate] {
        self.platten(m, true)
    }

    /// Das Gelände für das Bild, wie [`Platte::im_bild`].
    pub(crate) fn gelaende_im_bild(&mut self, m: &Model) -> Option<&Gelaende> {
        self.gelaende(m, true)
    }

    /// Je Durchlauf der Hauptschleife: einen fertigen Nachzug abholen,
    /// sonst nach einer Änderung einen beginnen, nicht in einem offenen
    /// Schritt. Der UI-Faden kopiert dafür nur das Modell; Eingaben,
    /// Zerlegen, Gelände und Netz rechnet ein eigener Faden, das Bild nimmt dann das
    /// fertige Netz (Review #119 W34 B6: synchron 20 ms bei 1000 Häusern,
    /// 53 ms bei 4000). `true`, wenn der Stand neu ist (dann folgt ein
    /// Bild).
    pub(crate) fn nachziehen(&mut self, m: &Model, sicht: Sicht) -> bool {
        if self.unterlage.abholen() {
            // ein laufender Nachzug brächte das Gelände ohne den Plan
            self.job = None;
            self.mit_plan = true;
        }
        let netz = self.netz_abholen();
        if self.abholen() || netz {
            return true;
        }
        let neu = self.stand != Some(m.revision()) || self.mit_plan;
        if self.job.is_some() || m.in_step() || !neu {
            return false;
        }
        self.mit_plan = false;
        let kopie = m.clone();
        let alt = self.eingabe.clone();
        let alt_ba = self.aufbau.clone();
        let ohne = self.gelaende.is_none();
        // Den Lageplan liest ein eigener Faden; die Linien bildet dieser
        // Faden ab, wenn sie nicht schon gemerkt sind (Review #139 B2, B3)
        let abbild = self.unterlage.abbild(m);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let e = kopie.terrain_plate_input();
            let u = abbild.map(|a| (a.key, a.linien(&kopie)));
            let l = u.as_ref().map_or(&[][..], |u| &u.1[..]);
            let g = Gelaende::aus(&kopie).map(|g| g.mit_unterlage(l));
            let ba = Bodenaufbau::aus(&kopie);
            let r = if alt.as_deref() == Some(&e) && ohne && g.is_none() && *alt_ba == ba {
                Ergebnis::Gleich
            } else {
                let p = e.build();
                let n = netz_der_sicht(&p, g.as_ref(), sicht, &ba);
                Ergebnis::Neu(Box::new((Arc::new(e), p, g, n, u, Arc::new(ba))))
            };
            // fällt weg, wenn die Platte inzwischen neu begonnen hat
            let _ = tx.send(r);
        });
        self.job = Some(Job {
            rev: m.revision(),
            sicht,
            rx,
        });
        false
    }

    /// Linien der Platten in Ansicht und Schnitt ([`linien`]), mit Aufmaß
    /// die des Geländes ([`gelaende_netz::grundlinien`]), gemerkt: neu nur,
    /// wenn sich Platten, Ansicht, Ebene, Modellgrenzen, Maßstab oder
    /// Zeichentabelle ändern, nicht je Bild (Review W34 B7: 0,35 ms bei
    /// 1000, 1,46 ms bei 4000 Häusern). Mit dem Zoom im Grundriss (`mm_px`)
    /// folgen nur die Punktmarken ([`gelaende_netz::marken`]; Review #129
    /// B3: sonst 233 ms je Zoomstufe bei 1000 Häusern).
    pub(crate) fn linien_im_bild(
        &mut self,
        m: &Model,
        (view, plane): (ViewKind, Option<(Vec3, Vec3)>),
        bounds: Option<(Vec3, Vec3)>,
        t: &DrawTable,
        (scale, mm_px): (f32, f64),
    ) -> &[Helper] {
        self.im_bild(m);
        // im Grundriss hängt das Gelände nicht an den Modellgrenzen: kein
        // Neubau je Mausbewegung, wenn sie sich im Zug ändern (Review #139)
        let grenzen = if self.gelaende.is_some() && view == ViewKind::Plan {
            None
        } else {
            bounds.map(bits)
        };
        let key = (
            self.fassung,
            view,
            plane.map(bits),
            grenzen,
            scale.to_bits(),
            t.rev,
            t.theme_rev,
        );
        if self.linien.as_ref().is_none_or(|l| l.key != key) {
            let helfer = match self.gelaende.as_deref() {
                Some(g) => {
                    let f = gelaende_netz::fassade(g, view, bounds);
                    gelaende_netz::grundlinien(g, view, plane, f, t, scale)
                }
                None => linien(&self.platten, &self.aufbau, view, plane, bounds, t, scale),
            };
            let n = helfer.len();
            self.linien = Some(Linien {
                key,
                helfer,
                marken: (n, None),
            });
        }
        // je Zoomstufe nur die Punktmarken neu
        if let (Some(l), Some(g)) = (self.linien.as_mut(), self.gelaende.as_deref()) {
            let px = Some(mm_px.to_bits());
            if l.marken.1 != px {
                l.helfer.truncate(l.marken.0);
                l.helfer
                    .extend(gelaende_netz::marken(g, view, t, scale, mm_px));
                l.marken.1 = px;
            }
        }
        self.linien.as_ref().map_or(&[], |l| &l.helfer)
    }

    /// Was der Renderer von der Unterlage hochladen soll, wenn sie zu
    /// sehen ist (Review #139 B1, N1 und N2). Je Bild vergleicht der
    /// UI-Faden nur die Schlüssel der Kacheln ([`unterlage_keys`]) mit
    /// denen im Renderer, wie viele Striche der Plan auch hat. Fehlen
    /// Kacheln, baut ein eigener Faden ihre Ecken; bis er fertig ist, bleibt
    /// stehen, was der Renderer hat, danach lädt der UI-Faden nur hoch.
    /// Nicht zu sehen (`zeigen`: Grundriss des Erdgeschosses, nicht
    /// ausgeblendet) bleibt alles stehen, und der Rückweg kostet nichts.
    pub(crate) fn unterlage_im_bild(
        &mut self,
        m: &Model,
        zeigen: bool,
        t: &DrawTable,
        scale: f32,
    ) -> Option<Hochladen> {
        if !zeigen {
            return None;
        }
        let keys = match self.gelaende_im_bild(m) {
            Some(g) => unterlage_keys(g, stil_key(t, scale)),
            None => Vec::new(),
        };
        if keys == self.im_renderer && self.renderer_bekannt {
            self.ecken_job = None;
            self.wartend = None;
            // übrige Kacheln im Vorrat fallen weg
            return (!self.im_vorrat.is_empty()).then(|| self.tauschen(keys));
        }
        if self.wartend.as_ref().is_some_and(|w| w.0 != keys) {
            self.wartend = None;
        }
        if self.wartend.is_none() {
            if let Some(rx) = &self.ecken_job {
                match rx.try_recv() {
                    Ok(ecken) => {
                        self.ecken_job = None;
                        self.wartend = Some((keys.clone(), ecken));
                    }
                    Err(mpsc::TryRecvError::Empty) => return None,
                    Err(mpsc::TryRecvError::Disconnected) => self.ecken_job = None,
                }
            }
        }
        if let Some((ziel, mut rest)) = self.wartend.take() {
            // je Bild höchstens HOCHLADEN_JE_BILD, mindestens eine Kachel
            let mut folge: Vec<u64> = rest.keys().copied().collect();
            folge.sort_unstable();
            let (mut laden, mut bytes) = (Vec::new(), 0);
            for k in folge {
                if bytes >= HOCHLADEN_JE_BILD {
                    break;
                }
                if let Some(e) = rest.remove(&k) {
                    bytes += std::mem::size_of_val(&e[..]).max(1);
                    laden.push((k, e));
                }
            }
            self.im_vorrat.extend(laden.iter().map(|l| l.0));
            if !rest.is_empty() {
                self.wartend = Some((ziel, rest));
                return Some(Hochladen { laden, keys: None });
            }
            let mut h = self.tauschen(keys);
            h.laden = laden;
            return Some(h);
        }
        let fehlt: Vec<(usize, u64)> = keys
            .iter()
            .enumerate()
            .filter(|(_, k)| !self.im_renderer.contains(k) && !self.im_vorrat.contains(k))
            .map(|(i, k)| (i, *k))
            .collect();
        let g = self.gelaende.clone().filter(|_| !fehlt.is_empty());
        let Some(g) = g else {
            // alles da: nur tauschen (oder leeren)
            return Some(self.tauschen(keys));
        };
        let stil = gelaende_netz::unterlage_stil(t, scale);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let ecken: Ecken = fehlt
                .into_iter()
                .map(|(i, k)| {
                    let l = gelaende_netz::unterlage_kachel(&g, i, stil);
                    (k, sk_render::helper_ecken(&l))
                })
                .collect();
            // fällt weg, wenn inzwischen andere Kacheln gefragt sind
            let _ = tx.send(ecken);
        });
        self.ecken_job = Some(rx);
        None
    }

    /// Die Kacheln `keys` zeigen, aus den gezeigten und dem Vorrat; was
    /// fehlt, fällt weg, der übrige Vorrat auch.
    fn tauschen(&mut self, keys: Vec<u64>) -> Hochladen {
        let keys: Vec<u64> = keys
            .into_iter()
            .filter(|k| self.im_renderer.contains(k) || self.im_vorrat.contains(k))
            .collect();
        self.im_renderer.clone_from(&keys);
        self.im_vorrat.clear();
        self.renderer_bekannt = true;
        Hochladen {
            laden: Vec::new(),
            keys: Some(keys),
        }
    }

    /// Ein Nachzug oder ein Netz zur Sicht rechnet (die Hauptschleife fragt
    /// dann je Bild nach).
    pub(crate) fn rechnet(&self) -> bool {
        self.job.is_some()
            || self.netz_job.is_some()
            || self.unterlage.liest()
            || self.leiste.liest()
            || self.ecken_job.is_some()
    }

    /// Netz zum Schlüssel `key`: fertig aus einem Faden, der gemerkte
    /// Körper, ohne Aufmaß das der Platten. Körper und Schnitt des Geländes
    /// rechnet ein eigener Faden (Review #129 B4: synchron 1,1 s bei 1000
    /// Häusern, 18 s bei 4000, bei jedem Sichtwechsel); bis er fertig ist
    /// `None`, und das alte Netz bleibt stehen.
    fn netz_zur_sicht(&mut self, key: NetzKey, sicht: Sicht) -> Option<Arc<MeshData>> {
        // ein fertiger Körper bleibt gemerkt, auch wenn die Sicht inzwischen
        // eine andere ist
        if let Some((k, n)) = self.fertig.take() {
            let n = Arc::new(n);
            if k.1 == ViewKind::Persp {
                self.koerper = Some((k, n.clone()));
            }
            if k == key {
                return Some(n);
            }
        }
        if let Some((k, n)) = &self.koerper {
            if *k == key {
                return Some(n.clone());
            }
        }
        let (view, _, _, hidden, _) = sicht;
        let g = match &self.gelaende {
            Some(g) if !hidden && matches!(view, ViewKind::Persp | ViewKind::Section) => g.clone(),
            g => {
                let n = netz_der_sicht(&self.platten, g.as_deref(), sicht, &self.aufbau);
                return Some(Arc::new(n));
            }
        };
        if self.netz_job.is_none() {
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(netz_der_sicht(
                    &[],
                    Some(&g),
                    sicht,
                    &Bodenaufbau::default(),
                ));
            });
            self.netz_job = Some((key, rx));
        }
        None
    }

    /// Ein fertiges Netz zur Sicht übernehmen; `true`, wenn eins kam (dann
    /// folgt ein Bild). Bricht der Faden ab, steht ein leeres Netz da,
    /// statt dass er je Bild neu beginnt.
    fn netz_abholen(&mut self) -> bool {
        let Some((key, rx)) = &self.netz_job else {
            return false;
        };
        let n = match rx.try_recv() {
            Ok(n) => n,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => MeshData::default(),
        };
        self.fertig = Some((*key, n));
        self.netz_job = None;
        true
    }

    /// Den Nachzug übernehmen, wenn der Faden fertig ist.
    fn abholen(&mut self) -> bool {
        let Some(job) = &self.job else {
            return false;
        };
        let r = match job.rx.try_recv() {
            Ok(r) => Ok(r),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err(()),
        };
        let job = self.job.take().expect("Nachzug");
        self.einsetzen(job, r)
    }

    /// Stand und, wenn neu, Platten und Netz des Nachzugs übernehmen. Ist
    /// der Faden ohne Ergebnis abgebrochen, bleiben die Platten, ohne dass
    /// derselbe Stand je Bild neu begonnen wird.
    fn einsetzen(&mut self, job: Job, r: Result<Ergebnis, ()>) -> bool {
        self.stand = Some(job.rev);
        let Ok(Ergebnis::Neu(neu)) = r else {
            return true;
        };
        let (e, p, g, n, u, ba) = *neu;
        if let Some((key, l)) = u {
            self.unterlage.merke_linien(key, l);
        }
        self.fassung += 1;
        self.platten = p;
        self.eingabe = Some(e);
        self.gelaende = g.map(Arc::new);
        self.aufbau = ba;
        self.netz_job = None;
        self.fertig = Some((netz_key(self.fassung, &job.sicht), n));
        true
    }

    /// Für Tests: auf den laufenden Nachzug warten und ihn übernehmen.
    #[cfg(test)]
    fn warten(&mut self) -> bool {
        let Some(job) = self.job.take() else {
            return false;
        };
        let r = job.rx.recv().map_err(|_| ());
        self.einsetzen(job, r)
    }
}

/// Aussehen von Rasen, Erdreich und blassem Rasen (Schlüssel hinter den
/// Baustoffen der Zeichentabelle): Rasen ohne Schnitt; Erdreich im Schnitt
/// auf Papier mit dem Zeichen „gewachsener Boden“ nach DIN 1356-1 (je
/// Zelle drei kurze Striche, Nachbarn um 90° gedreht, unter 45°) in der
/// Farbe des Stifts „Gelände“; blasser Rasen außerhalb der Grenze bei
/// „Umgebung blass“ (G1).
pub(crate) fn looks(theme: &Theme, paper: [f32; 3], ink: [f32; 3]) -> [MatLook; 3] {
    let rgb = |c: sk_paint::Rgba| {
        let [r, g, b, _] = c.to_f32();
        [r, g, b]
    };
    let (grass, soil) = (rgb(theme.env.terrain_grass), rgb(theme.env.terrain_soil));
    let fade = rgb(theme.env.terrain_fade);
    let base = MatLook {
        cut: soil,
        cut_bg: paper,
        cut_fg: ink,
        ..fallback_look(theme)
    };
    [
        MatLook {
            face: grass,
            ..base
        },
        bodenaufbau::erde(soil, base, theme.px_per_mm),
        MatLook { face: fade, ..base },
    ]
}

/// Abschnitte der Platte längs der Schnittebene (sortiert, vereinigt).
fn schnitt(p: &TerrainPlate, f: &SectionFrame, p0: Vec3) -> Vec<(f64, f64)> {
    let mut iv: Vec<(f64, f64)> = p
        .pieces
        .iter()
        .flat_map(|q| polygon::plane_intervals(q, p0, f.n, f.along))
        .collect();
    vereinigt(&mut iv)
}

/// Die Abschnitte sortiert, Berührende und Überlappende zu einem.
fn vereinigt(iv: &mut [(f64, f64)]) -> Vec<(f64, f64)> {
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for &(a, b) in iv.iter() {
        match out.last_mut() {
            Some(l) if a <= l.1 + 1e-3 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Band der Platte in einer Ansicht: Abschnitte längs `x` (vorn, hinten)
/// oder `y` (Seiten), vereinigt.
fn band(p: &TerrainPlate, side: bool) -> Vec<(f64, f64)> {
    let u = |v: &Vec3| if side { v.y } else { v.x };
    let mut iv: Vec<(f64, f64)> = p
        .pieces
        .iter()
        .map(|q| {
            let lo = q.iter().map(u).fold(f64::INFINITY, f64::min);
            let hi = q.iter().map(u).fold(f64::NEG_INFINITY, f64::max);
            (lo, hi)
        })
        .filter(|(a, b)| b > a)
        .collect();
    vereinigt(&mut iv)
}

fn at(q: Vec3, z: f64) -> Vec3 {
    vec3(q.x, q.y, z)
}

/// Körper einer Platte in 3D bis hinunter auf `bot` (den Boden): Rasen
/// oben, Erdreich unten und an den Seiten; Kanten am oberen und unteren
/// Rand und senkrecht dort, wo der Rand die Richtung wechselt.
fn koerper(p: &TerrainPlate, bot: f64, keys: [u16; 3], ba: &Bodenaufbau) -> Solid {
    let top = p.top;
    let up = vec3(0.0, 0.0, 1.0);
    let mut s = Solid {
        mat: keys[0],
        edge_kind: edge_kind::VIEW,
        ..Solid::default()
    };
    for q in &p.pieces {
        s.quad(
            at(q[0], top),
            at(q[1], top),
            at(q[2], top),
            at(q[3], top),
            up,
        );
    }
    s.mat = keys[1];
    for q in &p.pieces {
        s.quad(
            at(q[0], bot),
            at(q[3], bot),
            at(q[2], bot),
            at(q[1], bot),
            up * -1.0,
        );
    }
    let gleich = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
    // Kanten nach ihrem Endpunkt (Raster 1 mm, Nachbarzellen mit), statt
    // je Kante alle zu durchsuchen
    let zelle = |v: Vec3| (v.x.round() as i64, v.y.round() as i64);
    let mut enden: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, e) in p.edges.iter().enumerate() {
        enden.entry(zelle(e.1)).or_default().push(i);
    }
    let davor = |a: Vec3| {
        let (x, y) = zelle(a);
        let mut best: Option<usize> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &i in enden.get(&(x + dx, y + dy)).into_iter().flatten() {
                    if gleich(p.edges[i].1, a) && best.is_none_or(|b| i < b) {
                        best = Some(i);
                    }
                }
            }
        }
        best.map(|i| p.edges[i])
    };
    for &(a, b) in &p.edges {
        // am Haus steht die Aussparung des Bodenaufbaus
        let m = (a + b) * 0.5;
        if ba.am_haus([m.x, m.y]) {
            continue;
        }
        let d = (b - a).normalized();
        s.quad(at(a, bot), at(b, bot), at(b, top), at(a, top), right_of(d));
        s.edge(at(a, top), at(b, top));
        s.edge(at(a, bot), at(b, bot));
        // Ecke am Anfang: die Kante davor läuft in eine andere Richtung
        let davor = davor(a);
        let ecke = davor.is_none_or(|e| {
            let c = (e.1 - e.0).normalized();
            (c.x * d.y - c.y * d.x).abs() > 1e-6 || c.dot(d) < 0.0
        });
        if ecke {
            s.edge(at(a, bot), at(a, top));
        }
    }
    s
}

/// Strecken der Geländelinie einer Platte im Schnitt (Lauflänge, Höhe).
fn oben(p: &TerrainPlate, f: &SectionFrame, p0: Vec3) -> Vec<([f64; 2], [f64; 2])> {
    schnitt(p, f, p0)
        .into_iter()
        .map(|(a, b)| ([a, p.top], [b, p.top]))
        .collect()
}

/// Unterkante der Platten in Körper und Schnitt: die tiefste, mindestens
/// unter der tiefsten Gründung ([`Bodenaufbau::unten`]).
fn boden(plates: &[TerrainPlate], ba: &Bodenaufbau) -> f64 {
    ba.unten(
        plates
            .iter()
            .map(|p| p.bottom)
            .fold(f64::INFINITY, f64::min),
    )
}

/// Netz der Platten: in 3D die Körper, alle bis auf die tiefste
/// Unterkante (dort liegt der Boden, keine Platte schwebt), am Haus die
/// Aussparung im Bodenaufbau; im Schnitt die Schichten des Bodenaufbaus
/// (ihr Umriss kommt aus [`linien`]); sonst leer.
pub(crate) fn netz(
    plates: &[TerrainPlate],
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    keys: [u16; 3],
    ba: &Bodenaufbau,
) -> MeshData {
    let mut s = Solid::default();
    let unten = boden(plates, ba);
    let schichten = bodenaufbau::schluessel(keys);
    match (view, plane) {
        (ViewKind::Persp, _) if !plates.is_empty() => {
            for p in plates {
                s.append(&koerper(p, unten, keys, ba));
            }
            let teilung = |_: [f64; 2], _: [f64; 2]| Vec::new();
            s.append(&bodenaufbau::grube(
                ba,
                &|_| None,
                &teilung,
                unten,
                schichten,
            ));
        }
        (ViewKind::Section, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            let oben: Vec<_> = plates.iter().flat_map(|p| oben(p, &f, p0)).collect();
            s.append(&bodenaufbau::schnitt(ba, &f, &oben, unten, schichten));
        }
        _ => {}
    }
    crate::scene::mesh_of(&s)
}

/// Netz zur Sicht: mit Aufmaß das Gelände (Körper in 3D, Schnittflächen im
/// Schnitt, sonst keins), ohne die Platten; ausgeblendet leer.
fn netz_der_sicht(
    p: &[TerrainPlate],
    g: Option<&Gelaende>,
    s: Sicht,
    ba: &Bodenaufbau,
) -> MeshData {
    let (view, plane, keys, hidden, hilfsnetz) = s;
    match (g, view, plane) {
        _ if hidden => MeshData::default(),
        (Some(g), ViewKind::Persp, _) => {
            crate::scene::mesh_of(&gelaende_netz::koerper_mit_netz(g, keys, hilfsnetz))
        }
        (Some(g), ViewKind::Section, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            crate::scene::mesh_of(&gelaende_netz::kappen(g, &f, p0, keys))
        }
        (Some(_), ..) => MeshData::default(),
        (None, ..) => netz(p, view, plane, keys, ba),
    }
}

/// Linien der Platten in Ansicht und Schnitt (ersetzen die Geländelinie):
/// Oberkante mit dem Stift „Gelände“, Unterkante und Enden mit der feinen
/// Linie. In den Ansichten in der Mitte des Modells (`bounds`), verdeckt
/// werden sie nicht: Was unter dem Gelände liegt, regelt „unter Gelände“
/// (S11).
pub(crate) fn linien(
    plates: &[TerrainPlate],
    ba: &Bodenaufbau,
    view: ViewKind,
    plane: Option<(Vec3, Vec3)>,
    bounds: Option<(Vec3, Vec3)>,
    t: &DrawTable,
    scale: f32,
) -> Vec<Helper> {
    let fine = t.drawing_edges[edge_kind::FINE as usize];
    let line = |a: Vec3, b: Vec3, (w, c): Stroke| Helper {
        a: a.to_f32(),
        b: b.to_f32(),
        color: c,
        width: w * scale,
        dash: 0.0,
        pattern: sk_render::SOLID,
        occlude: false,
        round: false,
    };
    let mut out = Vec::new();
    // im Schnitt reicht die Platte unter die Gründung (Bodenaufbau)
    let unten = boden(plates, ba);
    let mut rahmen =
        |pt: &dyn Fn(f64, f64) -> Vec3, p: &TerrainPlate, iv: Vec<(f64, f64)>, bottom: f64| {
            for (a, b) in iv {
                out.push(line(pt(a, p.top), pt(b, p.top), t.ground));
                out.push(line(pt(a, bottom), pt(b, bottom), fine));
                out.push(line(pt(a, bottom), pt(a, p.top), fine));
                out.push(line(pt(b, bottom), pt(b, p.top), fine));
            }
        };
    match (view, plane) {
        (ViewKind::Section, Some((p0, n))) => {
            let f = SectionFrame::new(p0, n);
            for p in plates {
                rahmen(&|u, z| f.pt(u, z), p, schnitt(p, &f, p0), unten);
            }
        }
        (ViewKind::Front | ViewKind::Back | ViewKind::Left | ViewKind::Right, _) => {
            let side = matches!(view, ViewKind::Left | ViewKind::Right);
            let mitte = bounds.map_or(vec3(0.0, 0.0, 0.0), |(lo, hi)| (lo + hi) * 0.5);
            let pt = |u: f64, z: f64| {
                if side {
                    vec3(mitte.x, u, z)
                } else {
                    vec3(u, mitte.y, z)
                }
            };
            for p in plates {
                rahmen(&pt, p, band(p, side), p.bottom);
            }
        }
        _ => {}
    }
    out
}

/// Zahlenfeld ohne Sonderwerte.
fn feld(field: Field, label: &'static str, value: f64, r: (f64, f64, f64)) -> FieldRow {
    FieldRow {
        field,
        label,
        value,
        min: r.1,
        max: r.2,
        zero: false,
        einheit: None,
    }
}

/// Inhalt des Paneels „Eigenschaften“ des Geländes (I1: eine Zeile im
/// Baum, ein Paneel): ohne Punkte die Höhenlage aus dem Geschossmanager und
/// die Fläche der Platten, mit Punkten ihre Herkunft; immer Dicke und
/// Abstand ums Haus.
pub(crate) fn props(m: &Model, plates: &[TerrainPlate]) -> Props {
    let s = m.plate_settings();
    let mut values = vec![("Kategorie", "Gelände".to_string())];
    let hint = match m.terrain().filter(|t| !t.points.is_empty()) {
        None => {
            let ok = plates.first().map_or(m.terrain_z(), |p| p.top);
            let flaeche: f64 = plates.iter().map(TerrainPlate::area).sum();
            values.extend([
                ("Form", "eben".to_string()),
                ("OK Gelände", format!("{} m", kote_text(ok))),
                ("UK Gelände", format!("{} m", kote_text(ok - s.thick))),
                ("Fläche", format!("{} m²", de(flaeche / 1e6, 2))),
            ]);
            "Höhe aus dem Geschossmanager; zählt in keiner Menge. Bearbeiten: Knopf „Gelände“."
        }
        Some(t) => {
            let herkunft = if t.assumed() {
                "angenommen, nicht vermessen".to_string()
            } else {
                format!("Aufmaß {}", t.src)
            };
            values.extend([
                ("Herkunft", herkunft),
                ("Punkte", t.points.len().to_string()),
            ]);
            "Bearbeiten: Knopf „Gelände“."
        }
    };
    Props {
        values,
        sections: vec![Section {
            title: "Gelände",
            fields: vec![
                feld(Field::PlateThick, "Dicke", s.thick, PLATE_THICK),
                // „Außenoffset“ sagte nicht, wovon (Bedienbarkeit, später zu
                // Durchgang 41)
                feld(
                    Field::PlateMargin,
                    "Abstand ums Haus",
                    s.margin,
                    PLATE_MARGIN,
                ),
            ],
            hint,
            buttons: Vec::new(),
        }],
        ..Default::default()
    }
}

/// Bits einer Schnittebene (Schlüssel).
fn bits(p: (Vec3, Vec3)) -> [u64; 6] {
    let (a, n) = p;
    [a.x, a.y, a.z, n.x, n.y, n.z].map(f64::to_bits)
}

impl App {
    /// Was das Netz der Platte außer den Platten bestimmt.
    fn platte_sicht(&self) -> Sicht {
        let keys = self.scene.table().terrain;
        (
            self.ui.view,
            self.plane(),
            keys,
            self.scene.terrain_hidden(),
            self.ui.view == ViewKind::Persp && self.netz_an(),
        )
    }

    /// Netz der Platte hochladen, wenn sich die Platten, Ansicht, Schnitt,
    /// Aussehen oder Sichtbarkeit geändert haben (aus `build_mesh`, je
    /// Bild); nach einem Nachzug das Netz aus dem Faden. Gerechnet wird
    /// hier nur das Netz der Platten, wenn die Sicht sich ändert, und beim
    /// ersten Bild; Körper und Schnitt des Geländes kommen aus einem Faden
    /// ([`Platte::netz_zur_sicht`]).
    pub(crate) fn platte_hochladen(&mut self) {
        let sicht = self.platte_sicht();
        let m = self.scene.model();
        self.platte.im_bild(m);
        let key = netz_key(self.platte.fassung, &sicht);
        if self.platte.netz == Some(key) {
            return;
        }
        let Some(netz) = self.platte.netz_zur_sicht(key, sicht) else {
            return;
        };
        self.platte.netz = Some(key);
        self.renderer.set_mesh(MESH_PLATE, &netz);
        self.renderer.set_grass(sicht.2[0]);
    }

    /// Linien der Platte in Ansicht und Schnitt, mit Aufmaß die des
    /// Geländes, im Grundriss nur im Erdgeschoss; gemerkt
    /// ([`Platte::linien_im_bild`]), ausgeblendet keine. `vh`: Höhe der
    /// Ansicht in Bildpunkten.
    pub(crate) fn platte_linien(&mut self, vh: f64, scale: f32) -> &[Helper] {
        self.platte_unterlage(scale);
        if self.scene.terrain_hidden() {
            return &[];
        }
        let view = self.ui.view;
        let m = self.scene.model();
        let aufmass = self.platte.gelaende_im_bild(m).is_some();
        // ohne Aufmaß zeigen nur Ansichten und Schnitt Linien der Platte
        let keine = if aufmass {
            view == ViewKind::Plan && !self.scene.ground_active()
        } else {
            matches!(view, ViewKind::Plan | ViewKind::Persp)
        };
        if keine {
            return &[];
        }
        let mm_px = self.cam.ortho.map_or(10.0, |half| 2.0 * half / vh.max(1.0));
        let sicht = (view, self.plane());
        let (b, t) = (self.scene.bounds(), self.scene.table());
        self.platte.linien_im_bild(m, sicht, b, t, (scale, mm_px))
    }

    /// Höhenzahlen an Höhenlinien und ebenen Geländeflächen im Grundriss
    /// des Erdgeschosses als Tinte, in der Ziffernhöhe der Maßzahlen
    /// (Bedienbarkeit, Ist-Bilder zu #202); gemerkt, neu nur mit neuem
    /// Gelände, Maßstil oder neuer Zeichentabelle, nicht je Zoom.
    /// Ausgeblendet und ohne Aufmaß keine.
    pub(crate) fn gelaende_zahlen(&mut self) -> Option<crate::bemassung::Teil> {
        if self.ui.view != ViewKind::Plan
            || self.scene.terrain_hidden()
            || !self.scene.ground_active()
        {
            return None;
        }
        let m = self.scene.model();
        let stil = m.massstil();
        let h = stil.am_bau(stil.schrift_mm);
        let t = self.scene.table();
        let g = self.platte.gelaende_im_bild(m);
        if g.is_none_or(|g| g.zahlen.is_empty() && g.koten.is_empty()) {
            return None;
        }
        let key = (self.platte.fassung, h.to_bits(), t.rev, t.theme_rev);
        if let Some((k, _, teil)) = &self.platte.zahlen {
            if *k == key {
                return Some(teil.clone());
            }
        }
        // Mit Aufmaß bringt jede Änderung eine neue Fassung; gleiche Zahlen
        // behalten ihre Tinte, sonst lüde jeder Schritt bis zu 2.000 Koten
        // neu hoch (Review H-223b: 3,4 ms und 354.000 Dreiecke)
        let g = self.platte.gelaende_im_bild(m)?;
        let inhalt = gelaende_netz::zahlen_inhalt(g);
        if let Some((k, i, _)) = &mut self.platte.zahlen {
            if *i == inhalt && (k.1, k.2, k.3) == (key.1, key.2, key.3) {
                *k = key;
                return self.platte.zahlen.as_ref().map(|z| z.2.clone());
            }
        }
        let ui = self.ui.fonts.regular.as_ref();
        let schrift = self.massketten.schrift(&stil.schrift, ui)?;
        let g = self.platte.gelaende_im_bild(m)?;
        let ink = gelaende_netz::zahlen_tinte(g, &schrift, h, t);
        let teil: crate::bemassung::Teil = (crate::scene::neuer_stand() << 1, ink.into());
        self.platte.zahlen = Some((key, inhalt, teil.clone()));
        Some(teil)
    }

    /// Der Lageplan unter dem Gelände in eigenen Puffern des Renderers
    /// ([`Platte::unterlage_im_bild`]): je Bild nur der Vergleich der
    /// Schlüssel; neu sind nur Kacheln, deren Strecken sich ändern, alle
    /// bei neuer Zeichentabelle oder neuem Maßstab, und ihre Ecken baut ein
    /// eigener Faden (Review #139 B1, N1 und N2). Gezeigt im Grundriss des
    /// Erdgeschosses, nicht ausgeblendet; sonst bleiben die Puffer stehen,
    /// und der Rückweg kostet nichts.
    fn platte_unterlage(&mut self, scale: f32) {
        let zeigen = !self.scene.terrain_hidden()
            && self.ui.view == ViewKind::Plan
            && self.scene.ground_active();
        self.renderer.set_unterlage_sichtbar(zeigen);
        let (m, t) = (self.scene.model(), self.scene.table());
        if let Some(h) = self.platte.unterlage_im_bild(m, zeigen, t, scale) {
            for (k, e) in &h.laden {
                self.renderer.unterlage_laden(*k, e);
            }
            if let Some(keys) = &h.keys {
                self.renderer.set_unterlage(keys);
            }
        }
    }

    /// Hüllquader einer Ansicht: ohne Teile unter dem Gelände, wenn sie
    /// dort ausblendet (S11). Aus `main.rs`.
    pub(crate) fn ansicht_huelle(&self, v: ViewKind) -> Option<(Vec3, Vec3)> {
        let (lo, hi) = self.scene.bounds()?;
        let m = self.scene.model();
        let ganz = crate::ansicht_schatten::platz(v).is_none_or(|i| m.view_below(i));
        // Das tiefste Gelände: über keinem Gebäude wird etwas abgeschnitten;
        // mit Aufmaß zählt je Gründung ihr tiefster Punkt am Plattenrand
        let gruendungen = m
            .storeys()
            .iter()
            .filter(|(_, s)| s.kind == LevelKind::Foundation);
        let g = m
            .terrain_patches(TERRAIN_MARGIN)
            .iter()
            .map(|p| p.1)
            .chain(gruendungen.map(|(id, _)| m.terrain_z_of_level(id)))
            .fold(m.terrain_z(), f64::min);
        Some(if ganz || hi.z <= g {
            (lo, hi)
        } else {
            (vec3(lo.x, lo.y, lo.z.max(g)), hi)
        })
    }

    /// Gelände im Renderer: OK Gelände für „unter Gelände“, mit Aufmaß
    /// aus seinem Raster, sonst je Gebäude, und der Boden in 3D auf der
    /// Unterkante der Platte, mindestens unter der Gründung (die Platte
    /// steht auf ihm); ausgeblendet auf
    /// OK Gelände.
    pub(crate) fn gelaende_renderer(&mut self) {
        let raster = self.scene.terrain_grid();
        self.renderer.set_terrain_grid(raster.as_deref());
        let m = self.scene.model();
        self.renderer.set_terrain(m.terrain_z() as f32);
        // die Rechtecke je Modellstand, nicht je Bild
        let rev = m.revision();
        if self.platte.kisten.as_ref().is_none_or(|k| k.0 != rev) {
            let k = terrain_boxes(&m.terrain_patches(TERRAIN_MARGIN));
            self.platte.kisten = Some((rev, k));
        }
        if let Some((_, k)) = &self.platte.kisten {
            self.renderer.set_terrain_boxes(k);
        }
        let unten = if self.scene.terrain_hidden() {
            None
        } else if let Some(g) = self.platte.gelaende_im_bild(m) {
            Some(g.unten as f32)
        } else {
            let p = self.platte.im_bild(m);
            let b = p.iter().map(|p| p.bottom).reduce(f64::min);
            b.map(|b| self.platte.aufbau.unten(b) as f32)
        };
        self.renderer.set_ground_z(unten);
    }

    /// Eigenschaften des Geländes, solange es gewählt ist (Baum oder Klick
    /// in 3D); `true`, wenn das Paneel es zeigt. Wird es abgewählt, leert
    /// sich das Paneel für die übliche Auswahl.
    pub(crate) fn platte_props(&mut self) -> bool {
        let m = self.scene.model();
        // am Stand der Platten, nicht an der Revision: im Zug rechnet das
        // Paneel nicht je Mausbewegung neu (Review #119)
        let soll = if self.picking.plate && self.sel.id.is_none() {
            self.platte.im_bild(m);
            self.platte.stand
        } else {
            None
        };
        if soll == self.platte.props {
            return soll.is_some();
        }
        let war = self.platte.props.is_some();
        self.platte.props = soll;
        if soll.is_some() {
            if !war {
                self.ui.reset_props_scroll();
            }
            let p = props(m, &self.platte.platten);
            self.ui.set_props(Some(p));
        } else {
            self.ui.set_props(None);
            self.props_key = None;
        }
        self.props_dirty = true;
        soll.is_some()
    }

    /// Je Durchlauf, auch während Animationen (Review W34 H3): die
    /// Platten einer Änderung nachziehen ([`Platte::nachziehen`]); `true`,
    /// wenn ein Bild folgen soll. Davor folgen nach einem Gelände-Zug die
    /// Körper der übrigen Gebäude, je Durchlauf höchstens [`NACHZUG`] lang
    /// ([`Scene::nachziehen`](crate::scene::Scene::nachziehen), Review #123
    /// F1); sind alle neu, folgen Bild, Mengen und Paneel.
    pub(crate) fn platte_nachziehen(&mut self) -> bool {
        // das Geländenetz aus seinem Faden (Review #234 B-234b)
        if (self.scene.nachziehen(NACHZUG) == Some(true)) | self.scene.netz_abholen() {
            self.upload_model();
            self.quantity.dirty = true;
            self.props_key = None;
        }
        // Lageplan und Unterlage aus ihren Fäden (Review #139 B2)
        let ordner = self.doc.path.as_deref().and_then(std::path::Path::parent);
        if self.platte.unterlage.ordner(ordner) {
            self.platte.mit_plan = true;
        }
        let gelesen = self.lageplan_abholen() | self.hoehen_abholen() | self.baugruben_pruefen();
        let sicht = self.platte_sicht();
        let neu = self.platte.nachziehen(self.scene.model(), sicht);
        self.unterlage_stand();
        self.streifen_pruefen();
        self.nadeln_merken();
        neu || gelesen
    }

    /// Etwas rechnet im Hintergrund (Mengen, Verbände, die Platte, der
    /// Nachzug eines Gelände-Zugs): das Warten der Hauptschleife fragt dann
    /// je Bild nach.
    pub(crate) fn rechnet_noch(&self) -> bool {
        self.quantity_busy
            || !self.bond_wait.is_empty()
            || self.platte.rechnet()
            || self.baugruben_rechnen()
            || self.scene.zieht_nach()
            || self.scene.netz_rechnet()
    }

    /// Neues Projekt (Neu, Öffnen, Sicherung): Objektbaum zu, Platte
    /// abgewählt und neu zu rechnen (geladene Dateien beginnen alle bei
    /// Revision 0, sonst zeigte B die Platte von A).
    pub(crate) fn baum_neu(&mut self) {
        self.tree.reset();
        self.platte = Platte::default();
        // die Kacheln des alten Projekts (Review #139 N3)
        self.renderer.set_unterlage(&[]);
        self.picking.plate = false;
        self.picking.flaeche = None;
    }

    /// Zeile „Gelände“ im Objektbaum: das Gelände wählen, die Bauteile
    /// abwählen.
    pub(crate) fn platte_waehlen(&mut self) {
        self.picking.selected.clear();
        self.picking.plate = true;
        self.picking.flaeche = None;
        self.quantity.dirty = true;
        self.redraw = true;
    }

    /// Feld der Platte im Paneel übernommen (ein Schritt); `false`, wenn
    /// das Feld nicht zur Platte gehört.
    /// Felder der Leiste „Gelände“ gehen weiter an [`App::gelaende_feld`].
    pub(crate) fn platte_feld(&mut self, submit: Option<(Field, f64)>) -> bool {
        if let Some((Field::Gelaende(g), v)) = submit {
            self.gelaende_feld(g, v);
            return true;
        }
        let Some((f @ (Field::PlateThick | Field::PlateMargin), mm)) = submit else {
            return false;
        };
        let mut s = self.scene.model().plate_settings();
        match f {
            Field::PlateThick => s.thick = mm,
            _ => s.margin = mm,
        }
        if self
            .scene
            .edit_model("Gelände", |m| m.set_plate_settings(s))
        {
            self.upload_model();
        }
        true
    }
}

/// Das Gelände als Körper für den 3D-Export (Schnittstelle zum
/// Export-Thread, `sk-export`): Weltkoordinaten in mm, z zu ±0,00, jedes
/// Dreieck nach außen gerichtet; oben der Belag (`keys[0]`), Seiten, Boden
/// und Aussparung am Haus Erdreich (`keys[1]`). Mit Aufmaß das Netz, sonst
/// die Platten bis auf die tiefste Unterkante wie in 3D; `None` ohne
/// Gelände. Ist das Gelände geglättet, liefert diese Stelle das feine Netz.
pub(crate) fn export_koerper(m: &Model, keys: [u16; 2]) -> Option<Solid> {
    if let Some(g) = Gelaende::aus(m) {
        return Some(gelaende_netz::export_koerper(g, keys));
    }
    let plates = m.terrain_plates();
    let boden = plates
        .iter()
        .map(|p| p.bottom)
        .fold(f64::INFINITY, f64::min);
    let mut s = Solid::default();
    for p in &plates {
        s.append(&koerper(p, boden, [keys[0], keys[1], keys[0]]));
    }
    (!s.triangles.is_empty()).then_some(s)
}

#[cfg(test)]
mod tempo;

#[cfg(test)]
#[path = "gelaende_istbilder.rs"]
mod istbilder;

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::material;

    fn haus() -> Model {
        let mut m = Model::with_seed(3);
        let b = m.add_building(1);
        let pts = [
            vec3(0.0, 0.0, 0.0),
            vec3(0.0, 8000.0, 0.0),
            vec3(10000.0, 8000.0, 0.0),
            vec3(10000.0, 0.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m
    }

    /// Sicht der Tests: 3D, Rasen, Erdreich und blasser Rasen 40 bis 42.
    const SICHT: Sicht = (ViewKind::Persp, None, [40, 41, 42], false, false);

    /// Erste Wand des Modells um `d` verschieben, ein geschlossener Schritt.
    fn wand_schieben(s: &mut crate::scene::Scene, d: f64) {
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, d).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        s.commit();
    }

    /// Nach dem Loslassen rechnet das Bild die Platten nicht, sondern ein
    /// eigener Faden, den die Hauptschleife beginnt und abholt; im offenen
    /// Schritt beginnt sie keinen (Review #119 B4, W34 B6).
    #[test]
    fn leerlauf_zieht_nach() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        let a0 = pl.im_bild(s.model())[0].area();
        assert!(!pl.nachziehen(s.model(), SICHT), "aktuell");
        assert!(!pl.rechnet());
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 1000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        assert!(!pl.nachziehen(s.model(), SICHT), "im Schritt nicht");
        assert!(!pl.rechnet());
        s.commit();
        assert_eq!(
            pl.im_bild(s.model())[0].area(),
            a0,
            "das Bild rechnet nicht"
        );
        assert!(!pl.nachziehen(s.model(), SICHT), "beginnt den Faden");
        assert!(pl.rechnet());
        assert_eq!(pl.im_bild(s.model())[0].area(), a0, "steht bis dahin");
        assert!(pl.warten());
        let a1 = pl.im_bild(s.model())[0].area();
        assert!((a1 - a0).abs() > 1e6, "{a0} {a1}");
        assert!(!pl.nachziehen(s.model(), SICHT));
        assert!(!pl.rechnet());
        // die Hauptschleife ruft es je Durchlauf, vor dem Leerlauf, der
        // während Animationen ruht (Review W34 H3); im Bild nur `im_bild`
        let main = include_str!("main.rs");
        assert!(main.contains("a.redraw |= a.platte_nachziehen();"));
        let schleife = &main[main.find("a.redraw |= a.tool.pulsiert();").unwrap()..];
        assert!(
            schleife.find("a.platte_nachziehen()").unwrap()
                < schleife.find("if !a.redraw").unwrap()
        );
        assert!(main.contains("let wait = match (wait, a.rechnet_noch()) {"));
        let quelle = include_str!("gelaende_platte.rs");
        let app = &quelle[quelle.find("impl App {").unwrap()..quelle.find("mod tests {").unwrap()];
        assert!(!app.contains(concat!("platte.", "platten(")));
        assert!(!app.contains(concat!("platte.", "gelaende(")));
    }

    /// Während eines Zugs bleiben die Platten stehen und folgen beim
    /// Loslassen (Review W34 B1); ein neues Projekt rechnet neu, auch wenn
    /// beide Dateien bei Revision 0 beginnen (B3).
    #[test]
    fn zug_und_neues_projekt() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        let a0 = pl.platten(s.model(), false)[0].area();
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 1000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        let halten = |s: &crate::scene::Scene| s.model().in_step();
        assert_eq!(pl.platten(s.model(), halten(&s))[0].area(), a0, "steht");
        s.commit();
        assert!(!halten(&s));
        let a1 = pl.platten(s.model(), halten(&s))[0].area();
        assert!((a1 - a0).abs() > 1e6, "folgt beim Loslassen: {a0} {a1}");
        // ein Zug ohne Live-Netz (Geländehöhe ziehen): steht ebenso
        let top = pl.platten(s.model(), false)[0].top;
        let b = s.model().buildings().ids().next().unwrap();
        s.begin("Gelände");
        assert!(s.edit_attr(|m| m.set_terrain_offset_of(b, 400.0)));
        assert!(halten(&s));
        assert_eq!(
            pl.platten(s.model(), halten(&s))[0].top,
            top,
            "steht im Schritt"
        );
        s.commit();
        assert_eq!(pl.platten(s.model(), halten(&s))[0].top, top - 400.0);
        // zwei Dateien nacheinander: beide Revision 0
        let laden = |m: &Model| {
            let t = sk_model::szo::write(m);
            sk_model::szo::read(&t, sk_model::GuidGen::with_seed(1))
                .unwrap()
                .model
        };
        let (a, b) = (laden(&Model::with_seed(4)), laden(&haus()));
        assert_eq!(a.revision(), b.revision());
        let mut pl = Platte::default();
        assert!(pl.platten(&a, false).is_empty());
        assert!(pl.platten(&b, false).is_empty(), "ohne Zurücksetzen");
        let mut pl = Platte::default();
        assert_eq!(pl.platten(&b, false).len(), 1);
        let main = include_str!("main.rs");
        let install = &main[main.find("fn install_scene").unwrap()..];
        assert!(install[..install.find("\n    }\n").unwrap()].contains("self.baum_neu();"));
    }

    /// Das Netz kommt fertig aus dem Faden, zur Sicht beim Start. Esc
    /// ändert die Revision, aber nicht die Platten: Der Faden meldet
    /// „gleich“, Fassung und Netz bleiben (Review W34 H2). Ändert sich das
    /// Modell, während der Faden rechnet, kommt erst sein Stand, dann ein
    /// neuer Nachzug; das Rechnen im UI-Faden verwirft einen laufenden.
    #[test]
    fn nachzug_im_faden() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        pl.im_bild(s.model());
        let f0 = pl.fassung;
        wand_schieben(&mut s, 1000.0);
        pl.nachziehen(s.model(), SICHT);
        assert!(pl.warten());
        assert_eq!(pl.fassung, f0 + 1);
        let (k, n) = pl.fertig.take().expect("Netz aus dem Faden");
        assert_eq!(k, netz_key(pl.fassung, &SICHT));
        let soll = netz(&pl.platten, SICHT.0, SICHT.1, SICHT.2, &pl.aufbau);
        assert_eq!(n.faces, soll.faces);
        assert_eq!(n.edges, soll.edges);
        // ausgeblendet: leeres Netz
        wand_schieben(&mut s, 500.0);
        pl.nachziehen(
            s.model(),
            (ViewKind::Persp, None, [40, 41, 42], true, false),
        );
        assert!(pl.warten());
        assert!(pl.fertig.take().unwrap().1.faces.is_empty());
        // Esc: neue Revision, dieselben Platten
        let (f1, rev) = (pl.fassung, s.model().revision());
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 2000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        s.rollback();
        assert_ne!(s.model().revision(), rev);
        assert!(!pl.nachziehen(s.model(), SICHT));
        assert!(pl.warten());
        assert_eq!(pl.fassung, f1, "gleich");
        assert!(pl.fertig.is_none());
        assert_eq!(pl.stand, Some(s.model().revision()));
        // Änderung während des Fadens
        wand_schieben(&mut s, 1000.0);
        let r1 = s.model().revision();
        pl.nachziehen(s.model(), SICHT);
        wand_schieben(&mut s, -1000.0);
        assert!(pl.warten());
        assert_eq!(pl.stand, Some(r1));
        assert!(!pl.nachziehen(s.model(), SICHT));
        assert!(pl.rechnet(), "neuer Nachzug");
        assert!(pl.warten());
        assert_eq!(pl.stand, Some(s.model().revision()));
        // im UI-Faden gerechnet: der laufende Nachzug entfällt
        wand_schieben(&mut s, 1000.0);
        pl.nachziehen(s.model(), SICHT);
        assert!(pl.rechnet());
        pl.platten(s.model(), false);
        assert!(!pl.rechnet() && !pl.warten());
        // neues Projekt, während ein Faden rechnet
        wand_schieben(&mut s, 1000.0);
        pl.nachziehen(s.model(), SICHT);
        pl = Platte::default();
        assert!(!pl.rechnet());
    }

    /// Mit Aufmaß rechnet der Faden auch das Gelände und sein Netz; jede
    /// Änderung zählt, auch Esc, weil das Gelände die Häuser ausspart.
    #[test]
    fn gelaende_im_faden() {
        const CSV: &[u8] =
            include_bytes!("../../crates/sk-gelaende/tests/daten/hoehenaufmass-lindenhof-7.csv");
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            vec3(7000.0, 6000.0, 0.0),
            vec3(7000.0, 14000.0, 0.0),
            vec3(17000.0, 14000.0, 0.0),
            vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let (t, ..) = sk_model::Terrain::lesen(CSV, "a.csv", "2026-10-12", 32).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(t));
        m.commit();
        let mut s = crate::scene::Scene::with_model(m);
        let mut pl = Platte::default();
        pl.im_bild(s.model());
        assert!(pl.gelaende.is_some());
        let f0 = pl.fassung;
        wand_schieben(&mut s, 1000.0);
        pl.nachziehen(s.model(), SICHT);
        assert!(pl.warten());
        assert_eq!(pl.fassung, f0 + 1);
        let (k, n) = pl.fertig.take().unwrap();
        assert_eq!(k, netz_key(pl.fassung, &SICHT));
        let g = Gelaende::aus(s.model());
        assert!(!n.faces.is_empty());
        assert_eq!(
            n.faces,
            netz_der_sicht(&[], g.as_ref(), SICHT, &Bodenaufbau::default()).faces
        );
        // Esc
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        s.begin("Wand verschieben");
        let moved = orig.with_segment_moved(0, 2000.0).unwrap();
        assert!(s.set_run_points(run, &moved.points));
        s.rollback();
        pl.nachziehen(s.model(), SICHT);
        assert!(pl.warten());
        assert_eq!(pl.fassung, f0 + 2);
        assert!(pl.gelaende.is_some() && pl.fertig.is_some());
        // die Linien des Geländes gemerkt (Review W34 B7); mit dem Zoom
        // nur die Punktmarken neu, in doppelter Größe (#129 B3)
        let t = DrawTable::resolve(s.model(), &Theme::dark());
        let plan = (ViewKind::Plan, None);
        let l = pl.linien_im_bild(s.model(), plan, None, &t, (1.0, 10.0));
        let (a, n) = (l.as_ptr(), l.len());
        assert!(n > 0);
        let l = pl.linien_im_bild(s.model(), plan, None, &t, (1.0, 10.0));
        assert_eq!((l.as_ptr(), l.len()), (a, n));
        let vorher = l.to_vec();
        let g = pl.gelaende.as_deref().unwrap();
        let marken = gelaende_netz::marken(g, ViewKind::Plan, &t, 1.0, 10.0).len();
        assert!(marken > 0 && marken < n);
        let l = pl.linien_im_bild(s.model(), plan, None, &t, (1.0, 20.0));
        assert_eq!(l.len(), n);
        let k = n - marken;
        let gleich =
            |a: &Helper, b: &Helper| (a.a, a.b, a.color, a.width) == (b.a, b.b, b.color, b.width);
        assert!(l[..k].iter().zip(&vorher[..k]).all(|(a, b)| gleich(a, b)));
        let breit = |h: &Helper| h.b[0] - h.a[0];
        assert!((breit(&l[k]) - 2.0 * breit(&vorher[k])).abs() < 1e-3);
        // Körper und Schnitt kommen aus einem Faden; der Körper bleibt für
        // den Rückweg gemerkt (#129 B4)
        let persp: Sicht = (ViewKind::Persp, None, [1, 2, 3], false, false);
        let key = netz_key(pl.fassung, &persp);
        pl.fertig = None;
        assert!(pl.netz_zur_sicht(key, persp).is_none());
        assert!(pl.rechnet());
        assert!(
            pl.netz_zur_sicht(key, persp).is_none(),
            "kein zweiter Faden"
        );
        let n = loop {
            if pl.nachziehen(s.model(), persp) {
                break pl.netz_zur_sicht(key, persp).expect("fertig");
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert!(!pl.rechnet());
        let g = Gelaende::aus(s.model());
        assert_eq!(
            n.faces,
            netz_der_sicht(&[], g.as_ref(), persp, &Bodenaufbau::default()).faces
        );
        let grund: Sicht = (ViewKind::Plan, None, [1, 2, 3], false, false);
        let leer = pl.netz_zur_sicht(netz_key(pl.fassung, &grund), grund);
        assert!(leer.unwrap().faces.is_empty(), "Grundriss ohne Faden");
        let wieder = pl.netz_zur_sicht(key, persp).expect("gemerkt");
        assert!(Arc::ptr_eq(&n, &wieder) && !pl.rechnet());
    }

    /// Die Linien in Ansicht und Schnitt entstehen je Schlüssel einmal,
    /// nicht je Bild (Review W34 B7); neu mit Maßstab, Ansicht, Ebene,
    /// Modellgrenzen, Zeichentabelle und neuen Platten.
    #[test]
    fn linien_gemerkt() {
        let mut s = crate::scene::Scene::with_model(haus());
        let mut pl = Platte::default();
        let t = DrawTable::resolve(s.model(), &Theme::dark());
        let vorn = (ViewKind::Front, None);
        let l = pl.linien_im_bild(s.model(), vorn, None, &t, (1.0, 10.0));
        let (a, w) = (l.as_ptr(), l[0].width);
        assert_eq!(l.len(), 4);
        assert_eq!(
            pl.linien_im_bild(s.model(), vorn, None, &t, (1.0, 10.0))
                .as_ptr(),
            a
        );
        let l = pl.linien_im_bild(s.model(), vorn, None, &t, (2.0, 10.0));
        assert_eq!(l[0].width, 2.0 * w, "Maßstab");
        let l = pl.linien_im_bild(s.model(), (ViewKind::Left, None), None, &t, (2.0, 10.0));
        assert!(l.iter().all(|h| h.a[0] == 0.0), "Seite");
        let plane = (vec3(5000.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
        let schnitt = (ViewKind::Section, Some(plane));
        assert_eq!(
            pl.linien_im_bild(s.model(), schnitt, None, &t, (2.0, 10.0))
                .len(),
            8
        );
        let b = Some((vec3(0.0, -2000.0, 0.0), vec3(10000.0, 8000.0, 3000.0)));
        let l = pl.linien_im_bild(s.model(), vorn, b, &t, (1.0, 10.0));
        assert!(l.iter().all(|h| h.a[1] == 3000.0), "Mitte der Grenzen");
        let mut t2 = t.clone();
        t2.theme_rev += 1;
        let a = pl
            .linien_im_bild(s.model(), vorn, b, &t, (1.0, 10.0))
            .as_ptr();
        assert_ne!(
            pl.linien_im_bild(s.model(), vorn, b, &t2, (1.0, 10.0))
                .as_ptr(),
            a
        );
        // neue Platten: das Band ändert sich um 1 m
        let breite = |l: &[Helper]| {
            let xs = l.iter().flat_map(|h| [h.a[0], h.b[0]]);
            let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |a, x| (a.0.min(x), a.1.max(x)));
            hi - lo
        };
        let b0 = breite(pl.linien_im_bild(s.model(), vorn, b, &t2, (1.0, 10.0)));
        wand_schieben(&mut s, 1000.0);
        assert_eq!(
            breite(pl.linien_im_bild(s.model(), vorn, b, &t2, (1.0, 10.0))),
            b0
        );
        pl.nachziehen(s.model(), SICHT);
        assert!(pl.warten());
        let b1 = breite(pl.linien_im_bild(s.model(), vorn, b, &t2, (1.0, 10.0)));
        assert!(((b1 - b0).abs() - 1000.0).abs() < 1e-3, "{b0} {b1}");
    }

    /// Budget (Review W34 B6, 15 ms je Bild): Der UI-Faden kopiert für den
    /// Nachzug nur das Modell und holt das Ergebnis ab; Platten und Netz
    /// rechnet der Faden; die Linien je Bild sind gemerkt (B7). 1000 und
    /// 4000 Häuser in einem Gebäude (synchron waren es 20 und 53 ms); im
    /// Debug nur 1000, das Bauen von 4000 dauerte dort Minuten.
    #[test]
    fn nachzug_im_budget() {
        let budget = |ms: f64| {
            if cfg!(debug_assertions) {
                ms * 40.0
            } else {
                ms
            }
        };
        let faelle: &[usize] = if cfg!(debug_assertions) {
            &[1000]
        } else {
            &[1000, 4000]
        };
        for &n in faelle {
            let mut m = Model::with_seed(5);
            let b = m.add_building(1);
            for i in 0..n {
                let (ox, oy) = ((i % 64) as f64 * 15000.0, (i / 64) as f64 * 15000.0);
                let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
                let pts = [p(0.0, 0.0), p(0.0, 8000.0), p(1e4, 8000.0), p(1e4, 0.0)];
                m.build_from_polygon(b, &pts).unwrap();
            }
            let mut pl = Platte::default();
            pl.im_bild(&m);
            let run = m.runs().ids().next().unwrap();
            let orig = m.runs().get(run).unwrap().points.clone();
            // das Beste aus drei Nachzügen (andere Tests laufen nebenher)
            let (mut beginnen, mut abholen) = (f64::INFINITY, f64::INFINITY);
            for d in [1000.0, 0.0, 1000.0] {
                let moved: Vec<Vec3> = orig.iter().map(|q| *q + vec3(d, 0.0, 0.0)).collect();
                m.begin("Haus verschieben");
                assert!(m.set_run_points(run, &moved).is_some());
                m.commit();
                let t = std::time::Instant::now();
                assert!(!pl.nachziehen(&m, SICHT));
                beginnen = beginnen.min(t.elapsed().as_secs_f64() * 1000.0);
                let ms = loop {
                    let t = std::time::Instant::now();
                    if pl.nachziehen(&m, SICHT) {
                        break t.elapsed().as_secs_f64() * 1000.0;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                };
                abholen = abholen.min(ms);
                assert!(pl.fertig.take().is_some(), "neue Platten");
            }
            // die Linien je Bild (Review W34 B7): gemerkt
            let t = DrawTable::resolve(&m, &Theme::dark());
            let vorn = (ViewKind::Front, None);
            pl.linien_im_bild(&m, vorn, None, &t, (1.0, 10.0));
            let z = std::time::Instant::now();
            for _ in 0..100 {
                std::hint::black_box(pl.linien_im_bild(&m, vorn, None, &t, (1.0, 10.0)).len());
            }
            let linien = z.elapsed().as_secs_f64() * 10.0;
            eprintln!(
                "{n} Häuser: beginnen {beginnen:.2} ms, abholen {abholen:.3} ms, \
                 Linien {linien:.4} ms"
            );
            assert!(beginnen < budget(15.0), "{n}: {beginnen:.1} ms");
            assert!(abholen < budget(1.0), "{n}: {abholen:.2} ms");
            assert!(linien < budget(0.05), "{n}: {linien:.3} ms");
        }
    }

    /// In 3D ein geschlossener Körper: Rasen oben und Erdreich unten mit
    /// gleicher Fläche, vier Außen- und vier Lochecken als senkrechte
    /// Kanten; in Grundriss und Ansicht kein Netz.
    #[test]
    fn koerper_in_3d() {
        let m = haus();
        let p = m.terrain_plates();
        let keys = [40, 41, 42];
        let s = koerper(&p[0], p[0].bottom, keys, &Bodenaufbau::default());
        let flaeche = |k: u16| -> f64 {
            s.triangles
                .iter()
                .filter(|t| t.mat == k && t.n.z.abs() > 0.5)
                .map(|t| (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length() * 0.5)
                .sum()
        };
        assert!((flaeche(40) - p[0].area()).abs() < 1.0);
        assert!((flaeche(41) - p[0].area()).abs() < 1.0);
        let senkrecht = s
            .edges
            .iter()
            .filter(|e| (e.a.z - e.b.z).abs() > 1.0)
            .count();
        assert_eq!(senkrecht, 8);
        for v in [ViewKind::Plan, ViewKind::Front] {
            assert!(netz(&p, v, None, keys, &Bodenaufbau::aus(&m))
                .faces
                .is_empty());
        }
    }

    /// Schnitt quer durch die Mitte: links und rechts vom Haus je 3 m
    /// Erdreich in voller Dicke, geschnitten; die Linien rahmen beide
    /// Stücke, die Oberkante im Stift „Gelände“.
    #[test]
    fn schnitt_durch_die_mitte() {
        let m = haus();
        let p = m.terrain_plates();
        let plane = (vec3(5000.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0));
        let f = SectionFrame::new(plane.0, plane.1);
        let iv = schnitt(&p[0], &f, plane.0);
        assert_eq!(iv.len(), 2, "{iv:?}");
        for (a, b) in &iv {
            assert!((b - a - 3000.0).abs() < 1e-6, "{a} {b}");
        }
        // ohne Gründung: Oberboden über gewachsenem Boden, je Stück zwei
        // Flächen
        let mut leer = Bodenaufbau::aus(&m);
        leer.haeuser.clear();
        let n = netz(&p, ViewKind::Section, Some(plane), [40, 41, 42], &leer);
        assert_eq!(n.faces.len(), 2 * 2 * 6);
        let oberboden = bodenaufbau::schluessel([40, 41, 42])[bodenaufbau::OBERBODEN];
        assert!(n.faces.iter().all(|v| {
            let k = v[6] as u32 & !(material::CUT as u32);
            v[6] as u32 & material::CUT as u32 != 0 && (k == 41 || k == oberboden as u32)
        }));
        // mit der Gründung des Hauses auch die Schichten darunter
        let ba = Bodenaufbau::aus(&m);
        let mit = netz(&p, ViewKind::Section, Some(plane), [40, 41, 42], &ba);
        assert!(mit.faces.len() > n.faces.len());
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let l = linien(&p, &ba, ViewKind::Section, Some(plane), None, &t, 1.0);
        assert_eq!(l.len(), 8);
        let oben: Vec<_> = l
            .iter()
            .filter(|h| h.a[2] == 0.0 && h.b[2] == 0.0)
            .collect();
        assert_eq!(oben.len(), 2);
        assert!(oben.iter().all(|h| h.color == t.ground.1));
    }

    /// Ansicht von vorn: ein Band über die ganze Platte (16 m), von OK
    /// Gelände bis zur Unterkante; Dicke und Abstand ums Haus folgen den
    /// Einstellungen.
    #[test]
    fn band_in_der_ansicht() {
        let mut m = haus();
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let l = linien(
            &m.terrain_plates(),
            &Bodenaufbau::default(),
            ViewKind::Front,
            None,
            None,
            &t,
            1.0,
        );
        assert_eq!(l.len(), 4);
        let xs: Vec<f32> = l.iter().flat_map(|h| [h.a[0], h.b[0]]).collect();
        let (lo, hi) = xs
            .iter()
            .fold((f32::MAX, f32::MIN), |a, &x| (a.0.min(x), a.1.max(x)));
        assert_eq!((lo, hi), (-3000.0, 13000.0));
        m.begin("Platte");
        assert!(m.set_plate_settings(sk_model::PlateSettings {
            thick: 800.0,
            margin: 1500.0
        }));
        m.commit();
        let l = linien(
            &m.terrain_plates(),
            &Bodenaufbau::default(),
            ViewKind::Left,
            None,
            None,
            &t,
            1.0,
        );
        let zs: Vec<f32> = l.iter().flat_map(|h| [h.a[2], h.b[2]]).collect();
        assert!(zs.contains(&-800.0) && zs.contains(&0.0));
        let ys: Vec<f32> = l.iter().flat_map(|h| [h.a[1], h.b[1]]).collect();
        assert!(ys.contains(&-1500.0) && ys.contains(&9500.0));
    }

    /// Dicke und Abstand ums Haus sind ein Schritt: Rückgängig stellt die
    /// Vorgabe wieder her, Wiederholen den neuen Wert; das Paneel zeigt
    /// die Werte und die Höhe aus dem Geschossmanager.
    #[test]
    fn einstellungen_rueckgaengig() {
        let mut s = crate::scene::Scene::with_model(haus());
        let neu = sk_model::PlateSettings {
            thick: 700.0,
            margin: 2000.0,
        };
        assert!(s.edit_model("Gelände", |m| m.set_plate_settings(neu)));
        assert_eq!(s.model().plate_settings(), neu);
        assert!(s.undo());
        assert_eq!(s.model().plate_settings(), Default::default());
        assert!(s.redo());
        assert_eq!(s.model().plate_settings(), neu);
        let p = props(s.model(), &s.model().terrain_plates());
        let f = &p.sections[0].fields;
        assert_eq!((f[0].field, f[0].value), (Field::PlateThick, 700.0));
        assert_eq!((f[1].field, f[1].value), (Field::PlateMargin, 2000.0));
        assert_eq!((f[1].min, f[1].max), (1500.0, 3000.0));
        assert!(
            p.values.contains(&("UK Gelände", "−0,70 m".to_string())),
            "{:?}",
            p.values
        );
        assert_eq!(p.sections[0].title, "Gelände");
        // mit Punkten von Hand (I1): Herkunft statt Höhe, Felder bleiben
        assert!(s.edit_model("Fläche", |m| m.seed_flat_terrain(&[])));
        let p = props(s.model(), &s.model().terrain_plates());
        assert!(p
            .values
            .contains(&("Herkunft", "angenommen, nicht vermessen".to_string())));
        assert!(p.values.contains(&("Punkte", "4".to_string())));
        assert_eq!(p.sections[0].fields.len(), 2);
    }

    /// Rasen und Erdreich liegen hinter den Baustoffen der Tabelle; das
    /// Erdreich ist im Schnitt schraffiert.
    #[test]
    fn rasen_und_erdreich_in_der_tabelle() {
        let m = haus();
        let theme = Theme::dark();
        let t = DrawTable::resolve(&m, &theme);
        let [g, s, blass] = t.terrain;
        assert_eq!((s, blass), (g + 1, g + 2));
        assert_eq!(blass as usize + 1 + bodenaufbau::LOOKS, t.mats.len());
        let [r, gr, b, _] = theme.env.terrain_fade.to_f32();
        assert_eq!(t.mats[blass as usize].face, [r, gr, b]);
        assert!(m
            .materials()
            .iter()
            .all(|(id, _)| sk_model::material_key(id) < g));
        assert_eq!(t.mats[s as usize].kind, crate::draw_table::fill_kind::ERDE);
        let [r, gr, b, _] = theme.env.terrain_grass.to_f32();
        assert_eq!(t.mats[g as usize].face, [r, gr, b]);
    }
}

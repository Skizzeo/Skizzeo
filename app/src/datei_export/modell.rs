//! 3D-Export aus dem Dateimenü (Export-Thread, Jörn 17.10.): GLB und OBJ
//! für Twinmotion, Lumion und andere, STL für den 3D-Druck. Die Szene
//! übernimmt die Gliederung der Karte „Baum“ (Projekt › Gelände, Gebäude ›
//! Geschoss › Bauteilart › Bauteil), damit sich jede Ebene im Zielprogramm
//! ein- und ausblenden lässt wie hier. Exportiert wird, was deckend zu
//! sehen ist: Ausgeblendetes und ein ausgeblendetes Gelände bleiben
//! draußen, beim Isolieren nur das Isolierte.
//!
//! Für die Visualisierung kommen die Körper so, wie die Ansicht sie
//! zeichnet; für den Druck je Bauteil und Baustoff geschlossen wie im IFC
//! ([`sk_ifc::geometrie::netze`]), das Gelände als ein Körper. Maßstab,
//! Achse und Lage wählt die Zeile „Dateityp“ im Speichern-Dialog.
//!
//! Der Export liest nur; Modell, Mengen und Kosten bleiben unberührt.

use super::kachel;
use crate::menu::Format3d;
use crate::{document, meldung, App, NOTICE_TIME};
use sk_export::{Dreieck, Knoten, Lage, Lagesystem, Material, Netz, Oben, Szene};
use sk_model::proctex::Pattern;
use sk_model::tree::{self, NodeKey, ProjectTree, Tab};
use sk_model::view::Masks;
use sk_model::{material, ElementId, Model, Solid};
use sk_platform::Surface;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Wofür die Szene ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Zweck {
    /// Wie die Ansicht, mit Baustoffen (GLB, OBJ).
    Bild,
    /// Geschlossene Körper je Bauteil und Baustoff, das Gelände als einer
    /// (STL).
    Druck,
}

/// Zeilen „Dateityp“ für GLB: Lage.
const GLB_WAHL: [(&str, bool); 2] = [
    ("GLB, Ursprung Projektnullpunkt (*.glb)", false),
    ("GLB, im Lagesystem ETRS89/UTM (*.glb)", true),
];

/// Zeilen „Dateityp“ für OBJ: Achse nach oben und Lage.
const OBJ_WAHL: [(&str, Oben, bool); 4] = [
    ("OBJ, y oben, Projektnullpunkt (*.obj)", Oben::Y, false),
    ("OBJ, z oben, Projektnullpunkt (*.obj)", Oben::Z, false),
    ("OBJ, y oben, Lagesystem ETRS89/UTM (*.obj)", Oben::Y, true),
    ("OBJ, z oben, Lagesystem ETRS89/UTM (*.obj)", Oben::Z, true),
];

/// Zeilen „Dateityp“ für STL: Maßstab (Nenner) und ob als Text.
const STL_WAHL: [(&str, f64, bool); 5] = [
    ("STL, Maßstab 1:50 (*.stl)", 50.0, false),
    ("STL, Maßstab 1:100 (*.stl)", 100.0, false),
    ("STL, Maßstab 1:200 (*.stl)", 200.0, false),
    ("STL, Maßstab 1:500 (*.stl)", 500.0, false),
    ("STL als Text, Maßstab 1:100 (*.stl)", 100.0, true),
];

/// Zuletzt gewählte Zeile je Format, für die Sitzung (STL: 1:100).
static WAHL: [AtomicUsize; 3] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(1),
];

/// So viele dünne Teile nennt die Statuszeile mit Namen.
const DUENN_NAMEN: usize = 3;

impl App {
    /// Befehl aus dem Dateimenü: IFC oder ein 3D-Format.
    pub(crate) fn exportieren(&mut self, c: crate::menu::Command, surface: &Surface) {
        match c {
            crate::menu::Command::Export3d(f) => self.export_3d(f, surface),
            _ => self.export_ifc(surface),
        }
    }

    /// „GLB …“, „OBJ …“, „STL …“: Speichern-Dialog mit der Wahl im
    /// Dateityp, dann die Datei und in der Statuszeile, was darin steht.
    fn export_3d(&mut self, f: Format3d, surface: &Surface) {
        let (titel, ext, zeilen): (&str, &str, Vec<&str>) = match f {
            Format3d::Glb => (
                "GLB exportieren",
                "glb",
                GLB_WAHL.iter().map(|w| w.0).collect(),
            ),
            Format3d::Obj => (
                "OBJ exportieren",
                "obj",
                OBJ_WAHL.iter().map(|w| w.0).collect(),
            ),
            Format3d::Stl => (
                "STL für 3D-Druck",
                "stl",
                STL_WAHL.iter().map(|w| w.0).collect(),
            ),
        };
        let muster = format!("*.{ext}");
        let filters: Vec<(&str, &str)> = zeilen.iter().map(|z| (*z, muster.as_str())).collect();
        let slot = &WAHL[f as usize];
        let name = format!("{}.{ext}", self.datei_stamm());
        let Some((path, wahl)) =
            surface.save_dialog_wahl(titel, &filters, ext, &name, slot.load(Ordering::Relaxed))
        else {
            return;
        };
        let wahl = wahl.min(filters.len() - 1);
        slot.store(wahl, Ordering::Relaxed);
        let farben = gelaende_farben(self.scene.table());
        self.scene.model_fertig();
        let name = self.datei_stamm_von(self.scene.model());
        let model = self.scene.model();
        let zweck = if f == Format3d::Stl {
            Zweck::Druck
        } else {
            Zweck::Bild
        };
        let (szene, offen) = szene(model, zweck, &farben, &name);
        let lage = |oben: Oben, utm: bool| Lage {
            oben,
            lagesystem: utm.then(|| lagesystem(model)).flatten(),
        };
        let ohne_utm = |utm: bool| utm && model.location().normalized().utm.is_none();
        let (dateien, satz) = match f {
            Format3d::Glb => {
                let (_, utm) = GLB_WAHL[wahl];
                let b = sk_export::glb::schreiben(&szene, &lage(Oben::Y, utm));
                (
                    vec![(path.clone(), b)],
                    bild_satz("GLB", &szene, ohne_utm(utm)),
                )
            }
            Format3d::Obj => {
                let (_, oben, utm) = OBJ_WAHL[wahl];
                let stamm = path
                    .file_stem()
                    .map_or("modell".into(), |n| n.to_string_lossy().into_owned());
                let r = sk_export::obj::schreiben(&szene, &lage(oben, utm), &stamm);
                let mut d = vec![
                    (path.clone(), r.obj.into_bytes()),
                    (path.with_file_name(&r.mtl_datei), r.mtl.into_bytes()),
                ];
                for (name, png) in r.bilder {
                    d.push((path.with_file_name(name), png));
                }
                (d, bild_satz("OBJ", &szene, ohne_utm(utm)))
            }
            Format3d::Stl => {
                let (_, massstab, text) = STL_WAHL[wahl];
                let (b, bericht) = if text {
                    let (t, b) = sk_export::stl::text(&szene, massstab);
                    (t.into_bytes(), b)
                } else {
                    sk_export::stl::binaer(&szene, massstab)
                };
                (
                    vec![(path.clone(), b)],
                    druck_satz(massstab, &bericht, offen),
                )
            }
        };
        for (p, bytes) in &dateien {
            if let Err(e) = document::tabelle_schreiben(p, bytes) {
                let m = meldung::Meldung::aus_io("3D-Modell nicht gespeichert", titel, p, &e);
                surface.message(&m, true);
                return;
            }
        }
        self.status(meldung::Meldung::mit("{}", &[&satz]), NOTICE_TIME);
    }

    /// Name der Szene: Projektname, sonst der Dateiname.
    fn datei_stamm_von(&self, m: &Model) -> String {
        let n = m.project().name.trim();
        if n.is_empty() {
            self.datei_stamm()
        } else {
            n.to_string()
        }
    }
}

/// Farben von Belag und Erdreich des Geländes wie in 3D.
pub(crate) fn gelaende_farben(t: &crate::draw_table::DrawTable) -> [[u8; 3]; 2] {
    let f = |k: u16| {
        t.mats.get(k as usize).map_or([128, 128, 128], |l| {
            l.face.map(|c| (c * 255.0).round() as u8)
        })
    };
    [f(t.terrain[0]), f(t.terrain[1])]
}

/// Lage im Lagesystem aus der Georeferenz des Projekts wie im IFC
/// (`IfcMapConversion`); `None` ohne Ost- und Nordwert.
fn lagesystem(m: &Model) -> Option<Lagesystem> {
    let loc = m.location().normalized();
    let u = loc.utm?;
    let (b, l) = sk_math::utm::von_utm(u.e, u.n, u.zone);
    let gitter = (loc.north_deg() + sk_math::utm::konvergenz(b, l, u.zone)).to_radians();
    Some(Lagesystem {
        ost: u.e,
        nord: u.n,
        hoehe: loc.nhn.unwrap_or(0.0),
        drehung: gitter,
    })
}

/// Satz der Statuszeile nach GLB und OBJ.
fn bild_satz(format: &str, s: &Szene, ohne_utm: bool) -> String {
    let mut teile = 0;
    s.je_knoten(|_, _| teile += 1);
    let mut satz = format!(
        "{format} gespeichert: {} Teile, {} Dreiecke",
        teile,
        tausender(s.dreiecke())
    );
    let bilder = s.materialien.iter().filter(|m| m.textur.is_some()).count();
    if bilder > 0 {
        satz += &format!(
            ", {}",
            if bilder == 1 {
                "1 Muster als Bild".to_string()
            } else {
                format!("{bilder} Muster als Bild")
            }
        );
    }
    if ohne_utm {
        satz += " · ohne Lage im Lagesystem (Projektdaten), Ursprung am Projektnullpunkt";
    }
    satz
}

/// Satz der Statuszeile nach STL: Größe im Druck, dünne Teile, offene
/// Körper.
fn druck_satz(massstab: f64, b: &sk_export::stl::DruckBericht, offen: usize) -> String {
    let cm = |mm: f64| format!("{:.1}", mm / 10.0).replace('.', ",");
    let [x, y, z] = b.groesse;
    let mut satz = format!(
        "STL 1:{massstab} gespeichert: {} × {} × {} cm im Druck",
        cm(x),
        cm(y),
        cm(z)
    );
    if !b.duenn.is_empty() {
        let namen: Vec<&str> = b
            .duenn
            .iter()
            .take(DUENN_NAMEN)
            .map(|d| d.0.rsplit(" / ").next().unwrap_or(&d.0))
            .collect();
        let mehr = if b.duenn.len() > DUENN_NAMEN {
            " …"
        } else {
            ""
        };
        satz += &format!(
            " · {} dünner als 0,8 mm: {}{mehr}",
            if b.duenn.len() == 1 {
                "1 Teil".to_string()
            } else {
                format!("{} Teile", b.duenn.len())
            },
            namen.join(", ")
        );
    }
    if offen > 0 {
        satz += &format!(
            " · {} nicht geschlossen",
            if offen == 1 {
                "1 Körper".to_string()
            } else {
                format!("{offen} Körper")
            }
        );
    }
    satz
}

/// 52000 → „52.000“.
fn tausender(n: usize) -> String {
    let s = n.to_string();
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            o.push('.');
        }
        o.push(c);
    }
    o
}

/// Schlüssel von Belag und Erdreich im Geländekörper; sie stehen nur dort,
/// nie in den Körpern der Bauteile.
const BELAG: u16 = 1;
const ERDREICH: u16 = 2;

/// Die Szene zum Modell: Gliederung der Karte „Baum“ mit den sichtbaren
/// Körpern; dazu die Zahl der Körper, die sich für den Druck nicht
/// schließen ließen.
pub(crate) fn szene(m: &Model, zweck: Zweck, farben: &[[u8; 3]; 2], name: &str) -> (Szene, usize) {
    m.lesend(|| {
        let gelaende = if m.visibility().terrain_hidden {
            None
        } else {
            crate::gelaende_platte::export_koerper(m, [BELAG, ERDREICH])
        };
        let koerper: HashMap<ElementId, Solid> = m.element_solids().into_iter().collect();
        let baum = tree::build(m);
        let mut b = Bau {
            m,
            zweck,
            farben,
            koerper,
            gelaende,
            materialien: Vec::new(),
            stoffe: HashMap::new(),
            offen: 0,
        };
        let nodes = baum.tab(Tab::Tree);
        let mut knoten = Vec::new();
        // Zeile 0 ist das Projekt, darunter die Äste
        if let Some(p) = nodes.first() {
            let mut j = 1;
            while j < p.end.min(nodes.len()) {
                knoten.push(b.ast(&baum, j));
                j = nodes[j].end.max(j + 1);
            }
        }
        let mut s = Szene {
            name: name.into(),
            materialien: b.materialien,
            knoten,
        };
        s.ausduennen();
        (s, b.offen)
    })
}

/// Zustand beim Bau der Szene.
struct Bau<'a> {
    m: &'a Model,
    zweck: Zweck,
    farben: &'a [[u8; 3]; 2],
    koerper: HashMap<ElementId, Solid>,
    gelaende: Option<Solid>,
    materialien: Vec<Material>,
    /// Baustoffschlüssel bzw. Geländeschlüssel (über `u16::MAX`) → Platz
    /// in `materialien`.
    stoffe: HashMap<u32, usize>,
    offen: usize,
}

impl Bau<'_> {
    /// Zeile `i` der Karte „Baum“ mit ihrem Ast.
    fn ast(&mut self, baum: &ProjectTree, i: usize) -> Knoten {
        let nodes = baum.tab(Tab::Tree);
        let n = &nodes[i];
        let mut k = Knoten::neu(ohne_zahl(&n.label));
        match &n.key {
            NodeKey::Terrain => {
                if let Some(s) = self.gelaende.take() {
                    k.netze = self.netze(s, true, |_| true);
                }
            }
            NodeKey::Element(_) => {
                for &id in baum.elements(Tab::Tree, i) {
                    if let Some(s) = self.koerper.remove(&id) {
                        let maske = self.m.masks(id).solid;
                        let sichtbar = move |layer: u8| maske & Masks::bit(layer) != 0;
                        let netze = self.netze(s, false, sichtbar);
                        k.netze.extend(netze);
                    }
                }
            }
            _ => {}
        }
        let mut j = i + 1;
        while j < n.end.min(nodes.len()) {
            k.kinder.push(self.ast(baum, j));
            j = nodes[j].end.max(j + 1);
        }
        k
    }

    /// Netze eines Körpers je Material, nur die sichtbaren Schichten.
    fn netze(&mut self, mut s: Solid, gelaende: bool, sichtbar: impl Fn(u8) -> bool) -> Vec<Netz> {
        s.triangles.retain(|t| sichtbar(t.layer));
        s.edges.clear();
        let mut out: Vec<Netz> = Vec::new();
        match self.zweck {
            Zweck::Bild => {
                for t in &s.triangles {
                    let mat = self.material(t.mat, gelaende, t.n.z.abs() >= 0.5 * t.n.length());
                    let d = Dreieck {
                        p: t.p.map(|p| [p.x, p.y, p.z]),
                        n: [t.n.x, t.n.y, t.n.z],
                    };
                    match out.iter_mut().find(|n| n.material == mat) {
                        Some(n) => n.dreiecke.push(d),
                        None => out.push(Netz {
                            material: mat,
                            dreiecke: vec![d],
                        }),
                    }
                }
            }
            Zweck::Druck => {
                // Das Gelände ist ein Körper, nicht Belag und Erdreich
                if gelaende {
                    for t in &mut s.triangles {
                        t.mat = BELAG;
                    }
                }
                for n in sk_ifc::geometrie::netze(&s, sk_math::Vec3::default()) {
                    if !sk_ifc::geometrie::geschlossen(&n) {
                        self.offen += 1;
                    }
                    let mat = self.material(n.mat, gelaende, false);
                    let pt = |i: u32| {
                        let p = n.punkte[i as usize];
                        [p.x, p.y, p.z]
                    };
                    out.push(Netz {
                        material: mat,
                        dreiecke: n
                            .dreiecke
                            .iter()
                            .filter_map(|t| Dreieck::aus_umlauf(t.map(pt)))
                            .collect(),
                    });
                }
            }
        }
        out
    }

    /// Platz des Materials zum Schlüssel; Baustoffe mit Namen und Farbe
    /// der Oberfläche (ein Muster mit seiner Mischfarbe aus der Ferne, für
    /// die Visualisierung dazu als Bild), das Gelände mit Belag und
    /// Erdreich. `eben`: das Dreieck liegt flach; Muster, die die Ansicht
    /// dort nicht zeigt, bekommen dann ein eigenes Material ohne Bild.
    fn material(&mut self, key: u16, gelaende: bool, eben: bool) -> usize {
        let key = key & !material::CUT;
        let mat = (!gelaende).then(|| self.m.material_by_key(key)).flatten();
        let sf = mat.and_then(|x| self.m.attr().surface(x.surface));
        let muster = sf
            .and_then(|f| f.pattern.as_ref())
            .filter(|p| !matches!(p, Pattern::Foreign(_)));
        let bild = self.zweck == Zweck::Bild && muster.is_some();
        let flach = bild && eben && muster.is_some_and(|p| !kachel::auch_eben(p));
        let slot = if gelaende {
            u32::from(u16::MAX) + u32::from(key)
        } else {
            u32::from(key) | u32::from(flach) << 17
        };
        if let Some(&i) = self.stoffe.get(&slot) {
            return i;
        }
        let neu = if gelaende {
            let erd = key == ERDREICH;
            Material {
                name: if erd { "Erdreich" } else { "Geländebelag" }.into(),
                farbe: self.farben[usize::from(erd)],
                textur: None,
            }
        } else {
            let farbe = sf.map_or([200, 200, 200], |f| match &f.pattern {
                Some(p) => sk_model::proctex::mix(p, f.color),
                None => f.color,
            });
            let name = mat.map_or("Ohne Baustoff", |x| x.name.as_str());
            let textur = match (sf, muster) {
                (Some(f), Some(p)) if bild && !flach => kachel::kachel(p, f.color),
                _ => None,
            };
            Material {
                name: if flach {
                    format!("{name} (Oberseite)")
                } else {
                    name.into()
                },
                farbe,
                textur,
            }
        };
        self.materialien.push(neu);
        let i = self.materialien.len() - 1;
        self.stoffe.insert(slot, i);
        i
    }
}

/// „Außenwände (4)“ → „Außenwände“: die Zahl gehört zum Baum, nicht in den
/// Namen der Ebene.
fn ohne_zahl(label: &str) -> String {
    if let Some(rest) = label.strip_suffix(')') {
        if let Some(i) = rest.rfind(" (") {
            if !rest[i + 2..].is_empty() && rest[i + 2..].chars().all(|c| c.is_ascii_digit()) {
                return rest[..i].to_string();
            }
        }
    }
    label.to_string()
}

#[cfg(test)]
mod tests;

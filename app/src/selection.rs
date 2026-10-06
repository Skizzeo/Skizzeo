//! Auswahl eines Bauteils per Klick: Treffer, Hervorhebung und Inhalt des
//! Paneels „Eigenschaften“. Die Auswahl gehört der App, nicht dem Modell, und
//! steht nicht im Rückgängig-Verlauf.

use crate::camera::Camera;
use crate::scene::Scene;
use crate::ui::{Field, FieldRow, Props, ViewKind};
use sk_math::{vec3, Vec3};
use sk_model::{ElementId, ElementKind, FootingShape, FoundationError, MaterialId, Model};
use sk_paint::Rgba;
use sk_render::Helper;
use sk_ui::theme::Theme;

/// Bis zu so vielen Pixeln Bewegung zwischen Drücken und Loslassen gilt als Klick.
const CLICK_PX: f64 = 4.0;

#[derive(Default)]
pub struct Selection {
    pub id: Option<ElementId>,
    /// Wo die linke Taste in der Ansicht gedrückt wurde.
    press: Option<(f64, f64)>,
}

impl Selection {
    pub fn press(&mut self, x: f64, y: f64) {
        self.press = Some((x, y));
    }

    /// Loslassen: `true`, wenn es ein Klick war (kaum bewegt seit dem Drücken).
    pub fn release(&mut self, x: f64, y: f64, scale: f64) -> bool {
        self.press
            .take()
            .is_some_and(|(px, py)| (x - px).hypot(y - py) <= CLICK_PX * scale)
    }

    /// Wählt `id` (oder nichts). `true`, wenn sich die Auswahl geändert hat.
    pub fn set(&mut self, id: Option<ElementId>) -> bool {
        let changed = id != self.id;
        self.id = id;
        changed
    }

    /// Hebt die Auswahl auf, wenn es das Bauteil nicht mehr gibt (etwa nach
    /// Rückgängig). `true`, wenn sie aufgehoben wurde.
    #[cfg(test)]
    pub fn validate(&mut self, scene: &Scene) -> bool {
        match self.id {
            Some(id) if scene.model().element(id).is_none() => {
                self.id = None;
                true
            }
            _ => false,
        }
    }
}

/// Bauteil unter dem Bildpunkt `(x, y)` der Ansicht.
#[allow(clippy::too_many_arguments)]
pub fn pick_at(
    scene: &mut Scene,
    cam: &Camera,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Option<ElementId> {
    let (o, d) = cam.ray(x, y, w, h);
    scene.pick(view, section, o, d)
}

/// Zahl mit `dec` Nachkommastellen im deutschen Format, z. B. 1.608,25.
/// Länge in m aus mm: zwei Nachkommastellen, eine dritte nur, wenn es
/// Millimeter gibt (2,855 m, 2,98 m).
pub fn de_m(mm: f64) -> String {
    let cm = mm / 10.0;
    let dec = if (cm - cm.round()).abs() < 1e-6 { 2 } else { 3 };
    de(mm / 1e3, dec)
}

pub fn de(v: f64, dec: usize) -> String {
    let s = format!("{:.*}", dec, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut out = String::new();
    if v < 0.0 && s.bytes().any(|b| (b'1'..=b'9').contains(&b)) {
        out.push('-');
    }
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push(',');
        out.push_str(frac);
    }
    out
}

/// Zentimeter ohne überflüssige Nachkommastelle: „14“, „17,5“.
pub fn cm(mm: f64) -> String {
    let c = mm / 10.0;
    if (c - c.round()).abs() < 1e-9 {
        de(c, 0)
    } else {
        de(c, 1)
    }
}

/// Farbfeld, Name und Menge einer massiven Schicht aus einem Baustoff.
fn solid_layer(
    m: &Model,
    mat: MaterialId,
    t: f64,
    volume: Option<f64>,
) -> Option<(Rgba, String, String)> {
    let x = m.material(mat)?;
    let rgb = m.attr().surface(x.surface)?.cut_color;
    let amount = volume.map_or(String::new(), |v| {
        format!("{} m³ · {} kg", de(v / 1e9, 3), de(v / 1e9 * x.density, 0))
    });
    Some((
        Rgba::from_rgb8(rgb),
        format!("{} cm {}", cm(t), x.name),
        amount,
    ))
}

/// Feld „Sockelrücksprung“, wenn das Bauteil zu einem Zug mit Sohlplatte
/// gehört: 0 (bündig) oder 2 bis 50 cm.
fn recess_field(m: &Model, id: ElementId) -> Option<FieldRow> {
    let (slab, _) = m.foundation_of(m.run_of(id)?)?;
    let ElementKind::GroundSlab(s) = m.element(slab)?.kind else {
        return None;
    };
    Some(FieldRow {
        field: Field::Recess,
        label: "Sockelrücksprung",
        value: s.recess,
        min: sk_model::MIN_RECESS,
        max: 500.0,
        zero: true,
    })
}

/// Zahlenfeld ohne Sonderwert 0; Bereich in mm.
fn field(field: Field, label: &'static str, value: f64, min: f64, max: f64) -> FieldRow {
    FieldRow {
        field,
        label,
        value,
        min,
        max,
        zero: false,
    }
}

/// Paneel für die Erdgeschossdecke: Hauptmenge Fläche zuerst.
fn floor_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let ElementKind::Floor(f) = e.kind else {
        return None;
    };
    let q = scene.floor_qto(f.run);
    if let Some(q) = q {
        values.extend([
            ("Fläche", format!("{} m²", de(q.area / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
            ("Umfang", format!("{} m", de(q.perimeter / 1e3, 2))),
        ]);
    }
    let top = m.level_z(f.top).unwrap_or(0.0);
    values.push(("Oberkante", format!("+{} m", de(top / 1e3, 3))));
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mut notes = m.warnings(id);
    if let Some(Err(_)) = m.floor(f.run) {
        notes.push("Kein Körper: Lage oder Umriss ungültig".into());
    }
    Some(Props {
        values,
        layer_set: m
            .material(f.material)
            .map_or(String::new(), |x| x.name.clone()),
        layers: solid_layer(m, f.material, f.thickness, q.map(|q| q.volume))
            .into_iter()
            .collect(),
        set_label: "Baustoff",
        fields: vec![field(
            Field::FloorThickness,
            "Dicke",
            f.thickness,
            100.0,
            600.0,
        )],
        notes,
    })
}

/// Paneel für Sohlplatte und Frostschürze: Hauptmenge zuerst.
fn foundation_props(
    scene: &Scene,
    id: ElementId,
    mut values: Vec<(&'static str, String)>,
) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let run = m.run_of(id)?;
    let q = scene.foundation_qto(run);
    let mut fields = Vec::new();
    let (mat, t, volume) = match e.kind {
        ElementKind::GroundSlab(s) => {
            if let Some((sq, _)) = q {
                values.extend([
                    ("Fläche", format!("{} m²", de(sq.area / 1e6, 2))),
                    ("Volumen", format!("{} m³", de(sq.volume / 1e9, 3))),
                    ("Umfang", format!("{} m", de(sq.perimeter / 1e3, 2))),
                ]);
            }
            fields.push(field(
                Field::SlabThickness,
                "Dicke",
                s.thickness,
                100.0,
                1000.0,
            ));
            fields.extend(recess_field(m, id));
            (s.material, s.thickness, q.map(|q| q.0.volume))
        }
        ElementKind::StripFooting(f) => {
            if let Some((_, fq)) = q {
                values.extend([
                    ("Länge (Achse)", format!("{} m", de(fq.length / 1e3, 2))),
                    ("Volumen", format!("{} m³", de(fq.volume / 1e9, 3))),
                ]);
            }
            fields.extend([
                field(Field::FootingWidth, "Breite", f.width, 200.0, 1500.0),
                field(
                    Field::FootingDepth,
                    "Tiefe",
                    m.footing_depth(id).unwrap_or(0.0),
                    sk_model::MIN_FOOTING,
                    3000.0,
                ),
            ]);
            (f.material, f.width, q.map(|q| q.1.volume))
        }
        ElementKind::Wall(_) | ElementKind::Floor(_) => return None,
    };
    values.push(("Bauabschnitt", e.seq.to_string()));
    let mut notes = m.warnings(id);
    if let Some(Err(err)) = m.foundation(run) {
        notes.push(match err {
            FoundationError::RecessTooLarge => "Kein Körper: Rücksprung zu groß".into(),
            _ => "Kein Körper: Umriss ungültig".into(),
        });
    }
    Some(Props {
        values,
        layer_set: m.material(mat).map_or(String::new(), |x| x.name.clone()),
        layers: solid_layer(m, mat, t, volume).into_iter().collect(),
        set_label: "Baustoff",
        fields,
        notes,
    })
}

/// Inhalt des Paneels „Eigenschaften“ für ein Bauteil.
pub fn props(scene: &Scene, id: ElementId) -> Option<Props> {
    let m = scene.model();
    let e = m.element(id)?;
    let q = scene.wall_qto(id);
    let mut values = vec![
        ("Nummer", e.number.clone()),
        ("Kategorie", e.category.name().to_string()),
        (
            "Geschoss",
            m.storey(e.storey).map_or("–".into(), |s| s.short.clone()),
        ),
        (
            "Gebäude",
            m.building_of(e.storey)
                .and_then(|b| m.building(b))
                .map_or("–".into(), |b| b.number.clone()),
        ),
    ];
    match e.kind {
        ElementKind::Wall(_) => {}
        ElementKind::Floor(_) => return floor_props(scene, id, values),
        _ => return foundation_props(scene, id, values),
    }
    if let Some(q) = q {
        values.extend([
            ("Länge", format!("{} m", de(q.length / 1e3, 2))),
            ("Dicke", format!("{} cm", cm(q.width))),
            ("Höhe", format!("{} m", de_m(q.height))),
            ("Fläche außen", format!("{} m²", de(q.side_outer / 1e6, 2))),
            ("Fläche innen", format!("{} m²", de(q.side_inner / 1e6, 2))),
            ("Volumen", format!("{} m³", de(q.volume / 1e9, 3))),
        ]);
    }
    let set = e.layer_set.and_then(|s| m.layer_set(s));
    let layers = set.map_or(Vec::new(), |s| {
        s.layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let mat = m.material(l.material)?;
                let rgb = m.attr().surface(mat.surface)?.cut_color;
                let amount = q.and_then(|q| q.layers.get(i)).map_or(String::new(), |lq| {
                    format!("{} m³ · {} kg", de(lq.volume / 1e9, 3), de(lq.mass, 0))
                });
                Some((
                    Rgba::from_rgb8(rgb),
                    format!("{} cm {}", cm(l.thickness), mat.name),
                    amount,
                ))
            })
            .collect()
    });
    Some(Props {
        values,
        layer_set: set.map_or(String::new(), |s| s.name.clone()),
        layers,
        fields: recess_field(m, id).into_iter().collect(),
        ..Default::default()
    })
}

/// Teil der Strecke hinter der Ebene `(p0, n)` (Seite gegen `n`).
fn behind(a: Vec3, b: Vec3, (p0, n): (Vec3, Vec3)) -> Option<(Vec3, Vec3)> {
    let (da, db) = ((a - p0).dot(n), (b - p0).dot(n));
    if da > 0.0 && db > 0.0 {
        return None;
    }
    let cut = || a + (b - a) * (da / (da - db));
    Some((
        if da > 0.0 { cut() } else { a },
        if db > 0.0 { cut() } else { b },
    ))
}

/// Umriss des gewählten Bauteils in Akzentfarbe. Im Grundriss die Schnittfläche,
/// im Schnitt nur der Teil hinter der Ebene.
pub fn helpers(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
) -> Vec<Helper> {
    outline(
        scene,
        id,
        view,
        section,
        scale,
        theme,
        theme.interact.select,
    )
}

/// Umriss des Bauteils unter der Maus (auch vom Mengenfenster aus, F2) in
/// der Rolle `interact.hover_element`, wie [`helpers`] gezeichnet.
pub fn hover_helpers(
    scene: &Scene,
    hover: Option<ElementId>,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
) -> Vec<Helper> {
    hover.map_or_else(Vec::new, |id| {
        outline(
            scene,
            id,
            view,
            section,
            scale,
            theme,
            theme.interact.hover_element,
        )
    })
}

fn outline(
    scene: &Scene,
    id: ElementId,
    view: ViewKind,
    section: Option<(Vec3, Vec3)>,
    scale: f32,
    theme: &Theme,
    color: [f32; 4],
) -> Vec<Helper> {
    let at = |p: Vec3, z: f64| vec3(p.x, p.y, z);
    let mut lines = Vec::new();
    // Umriss `f` von z0 bis z1 als Kanten (im Grundriss nur oben)
    let mut prism = |f: &[Vec3], z0: f64, z1: f64| {
        let n = f.len();
        for i in 0..n {
            let (a, b) = (f[i], f[(i + 1) % n]);
            if view == ViewKind::Plan {
                // Von oben fallen Fuß, Kopf und Kanten zusammen
                lines.push((at(a, z1), at(b, z1)));
            } else {
                lines.push((at(a, z0), at(b, z0)));
                lines.push((at(a, z1), at(b, z1)));
                lines.push((at(a, z0), at(a, z1)));
            }
        }
    };
    let m = scene.model();
    match (m.segment_of(id), m.element(id).map(|e| &e.kind)) {
        (Some((run, seg)), _) => {
            let Some(f) = scene.chain(run).and_then(|c| c.segment_footprint(seg)) else {
                return Vec::new();
            };
            let height = scene.chain(run).map_or(0.0, |c| c.height);
            let top = if view == ViewKind::Plan {
                scene.plan_cut().min(height)
            } else {
                height
            };
            prism(&f, 0.0, top);
        }
        (None, Some(ElementKind::Floor(f))) => {
            // Über der Schnittebene des Grundrisses (EG): dort nicht hervorgehoben
            let Some(slab) = scene.floor(f.run) else {
                return Vec::new();
            };
            let (b, t) = slab.band();
            if view == ViewKind::Plan && b >= scene.plan_cut() {
                return Vec::new();
            }
            prism(&slab.outline, b, t.min(scene.plan_cut()));
        }
        (None, Some(kind)) => {
            let Some(found) = m.run_of(id).and_then(|r| scene.foundation(r)) else {
                return Vec::new();
            };
            let p = found.params;
            let t = p.slab_thickness;
            match kind {
                ElementKind::GroundSlab(_) => prism(&found.outline, -t, 0.0),
                _ => {
                    let bottom = -t - p.footing_depth;
                    prism(&found.outline, bottom, -t);
                    if let FootingShape::Ring(inset) = &found.footing {
                        prism(&inset.pts, bottom, -t);
                    }
                }
            }
        }
        _ => return Vec::new(),
    }
    if view == ViewKind::Section {
        let Some(pl) = section else {
            return Vec::new();
        };
        lines = lines
            .into_iter()
            .filter_map(|(a, b)| behind(a, b, pl))
            .collect();
    }
    lines
        .into_iter()
        .map(|(a, b)| Helper {
            a: a.to_f32(),
            b: b.to_f32(),
            color,
            width: theme.size.outline * scale,
            dash: 0.0,
            pattern: sk_render::SOLID,
            occlude: view == ViewKind::Persp,
            round: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sk_model::{Model, RefSide, WallChain};

    #[test]
    fn deutsches_zahlenformat() {
        assert_eq!(de(1608.4, 0), "1.608");
        assert_eq!(de(8.3887, 3), "8,389");
        assert_eq!(de(27.5, 2), "27,50");
        assert_eq!(de(1234567.891, 2), "1.234.567,89");
        assert_eq!(de(-0.0001, 2), "0,00");
        assert_eq!(de(-12.5, 1), "-12,5");
        assert_eq!(
            (de_m(2855.0), de_m(2980.0), de_m(3500.0)),
            ("2,855".into(), "2,98".into(), "3,50".into())
        );
        assert_eq!(
            (cm(140.0), cm(175.0), cm(315.0)),
            ("14".into(), "17,5".into(), "31,5".into())
        );
    }

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Außenkante auf der Bezugslinie.
    fn rechteck() -> WallChain {
        WallChain {
            base: 0.0,
            points: vec![
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 8000.0, 0.0),
                vec3(10000.0, 8000.0, 0.0),
                vec3(10000.0, 0.0, 0.0),
            ],
            closed: true,
            ref_side: RefSide::Left,
            layers: Vec::new(),
            height: 2750.0,
            joints: Default::default(),
        }
    }

    /// Das Rechteck mit OK EG +2,75: EG-Wände 2,75 hoch (B12: Wände reichen
    /// von UK bis OK ihres Geschosses).
    fn haus(s: &mut Scene) -> sk_model::RunId {
        let run = s.add_wall(&rechteck()).unwrap();
        assert!(s.edit_model("OK EG", |m| {
            let eg = m.defaults().storey;
            m.set_storey_top(eg, 2750.0)
        }));
        run
    }

    fn value(p: &Props, k: &str) -> String {
        p.values.iter().find(|v| v.0 == k).unwrap().1.clone()
    }

    #[test]
    fn klick_waehlt_wand_und_rueckgaengig_hebt_auf() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let run = haus(&mut s);
        let mut sel = Selection::default();
        // Segment 2 läuft bei x = 10 m von y = 8 m nach 0; Strahl von außen (+x)
        let o = vec3(20000.0, 4000.0, 1000.0);
        let hit = s.pick(ViewKind::Persp, None, o, vec3(-1.0, 0.0, 0.0));
        assert_eq!(hit, s.model().wall_at(run, 2));
        assert!(sel.set(hit));
        let p = props(&s, sel.id.unwrap()).unwrap();
        assert_eq!(
            value(&p, "Nummer"),
            s.model().element(hit.unwrap()).unwrap().number
        );
        assert_eq!(value(&p, "Kategorie"), "Außenwand");
        assert_eq!(value(&p, "Geschoss"), "EG");
        assert_eq!(value(&p, "Gebäude"), "GB-01");
        assert_eq!(value(&p, "Länge"), "8,00 m");
        assert_eq!(value(&p, "Dicke"), "31,5 cm");
        assert_eq!(value(&p, "Höhe"), "2,75 m");
        assert_eq!(p.layer_set, "AW 31,5 Gasbeton + WDVS");
        assert_eq!(p.layers.len(), 2);
        assert_eq!(p.layers[0].1, "14 cm Dämmung (WDVS)");
        assert_eq!(p.layers[1].1, "17,5 cm Gasbeton");

        // Grundriss: von oben auf die Schnittfläche
        let down = vec3(0.0, 0.0, -1.0);
        let top = vec3(9900.0, 4000.0, 9000.0);
        assert_eq!(s.pick(ViewKind::Plan, None, top, down), hit);
        // Schnitt bei y = 4 m, Blick nach +y: Die Wand liegt in der Ebene und bleibt treffbar
        let pl = Some((vec3(0.0, 4000.0, 0.0), vec3(0.0, -1.0, 0.0)));
        let o = vec3(9900.0, -5000.0, 1000.0);
        assert_eq!(s.pick(ViewKind::Section, pl, o, vec3(0.0, 1.0, 0.0)), hit);
        assert!(!helpers(&s, hit.unwrap(), ViewKind::Section, pl, 1.0, &Theme::dark()).is_empty());
        assert_eq!(
            helpers(&s, hit.unwrap(), ViewKind::Persp, None, 1.0, &Theme::dark()).len(),
            12
        );

        // Daneben: nichts
        assert_eq!(s.pick(ViewKind::Persp, None, o, vec3(0.0, 0.0, 1.0)), None);

        // Rückgängig des Anlegens: Die Auswahl gilt nicht mehr
        assert!(!sel.validate(&s));
        assert!(s.undo(), "OK EG");
        assert!(s.undo(), "Anlegen");
        assert!(sel.validate(&s));
        assert_eq!(sel.id, None);
    }

    #[test]
    fn mengen_im_paneel() {
        let mut s = Scene::with_model(Model::with_seed(6));
        let run = haus(&mut s);
        // Obere Wand: 10 m außen, Gehrung an beiden Enden
        let p = props(&s, s.model().wall_at(run, 1).unwrap()).unwrap();
        assert_eq!(value(&p, "Länge"), "10,00 m");
        assert_eq!(value(&p, "Fläche außen"), "27,50 m²");
        assert_eq!(value(&p, "Fläche innen"), "25,77 m²");
        let q = s.wall_qto(s.model().wall_at(run, 1).unwrap()).unwrap();
        assert_eq!(
            value(&p, "Volumen"),
            format!("{} m³", de(q.volume / 1e9, 3))
        );
        assert!(p.layers[1].2.ends_with(" kg"), "{}", p.layers[1].2);
    }

    #[test]
    fn mengen_erst_nach_dem_loslassen() {
        let mut s = Scene::with_model(Model::with_seed(7));
        let run = s.add_wall(&rechteck()).unwrap();
        // Linke Wand: wird länger, wenn die obere nach außen rückt
        let wall = s.model().wall_at(run, 0).unwrap();
        let vol = s.wall_qto(wall).unwrap().volume;
        s.begin("Wand verschieben");
        // Live-Ziehen der oberen Wand: keine Mengen
        let moved = s
            .chain(run)
            .unwrap()
            .with_segment_moved(1, -1000.0)
            .unwrap();
        s.set_run_points(run, &moved.points);
        assert!(s.wall_qto(wall).is_none());
        // Loslassen: Mengen für den neuen Stand
        s.commit();
        let q = s.wall_qto(wall).unwrap();
        assert!(q.volume > vol);
        assert_eq!(Some(q), sk_model::wall_qto(s.model(), wall).as_ref());
    }

    #[test]
    fn klick_erkennt_kleine_bewegung() {
        let mut sel = Selection::default();
        sel.press(100.0, 100.0);
        assert!(sel.release(102.0, 101.0, 1.0));
        sel.press(100.0, 100.0);
        assert!(!sel.release(120.0, 100.0, 1.0));
        assert!(!sel.release(100.0, 100.0, 1.0));
    }

    #[test]
    fn umriss_in_der_auswahlfarbe_des_schemas() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let run = s.add_wall(&rechteck()).unwrap();
        let id = s.model().wall_at(run, 0).unwrap();
        let mut th = Theme::dark();
        let colors = |th: &Theme| -> Vec<[f32; 4]> {
            helpers(&s, id, ViewKind::Persp, None, 1.0, th)
                .iter()
                .map(|h| h.color)
                .collect()
        };
        assert!(colors(&th).iter().all(|&c| c == th.ui.accent.to_f32()));
        th.set_accent(Rgba::rgb(40, 120, 220));
        assert!(colors(&th)
            .iter()
            .all(|&c| c == [40.0 / 255.0, 120.0 / 255.0, 220.0 / 255.0, 1.0]));
    }
}

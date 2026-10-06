//! Auswahl eines Bauteils per Klick: Treffer, Hervorhebung und Inhalt des
//! Paneels „Eigenschaften“. Die Auswahl gehört der App, nicht dem Modell, und
//! steht nicht im Rückgängig-Verlauf.

use crate::camera::Camera;
use crate::scene::{Scene, PLAN_CUT};
use crate::ui::{Props, ViewKind};
use sk_math::{vec3, Vec3};
use sk_model::ElementId;
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
            m.storey(e.storey).map_or("–".into(), |s| s.name.clone()),
        ),
    ];
    if let Some(q) = q {
        values.extend([
            ("Länge", format!("{} m", de(q.length / 1e3, 2))),
            ("Dicke", format!("{} cm", cm(q.width))),
            ("Höhe", format!("{} m", de(q.height / 1e3, 2))),
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
    let Some((run, seg)) = scene.model().segment_of(id) else {
        return Vec::new();
    };
    let Some(f) = scene.chain(run).and_then(|c| c.segment_footprint(seg)) else {
        return Vec::new();
    };
    let height = scene.chain(run).map_or(0.0, |c| c.height);
    let at = |p: Vec3, z: f64| vec3(p.x, p.y, z);
    let mut lines = Vec::new();
    for i in 0..4 {
        let (a, b) = (f[i], f[(i + 1) % 4]);
        if view == ViewKind::Plan {
            // Von oben fallen Fuß, Kopf und Kanten zusammen
            let z = PLAN_CUT.min(height);
            lines.push((at(a, z), at(b, z)));
        } else {
            lines.push((a, b));
            lines.push((at(a, height), at(b, height)));
            lines.push((a, at(a, height)));
        }
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
    let color = theme.interact.select;
    lines
        .into_iter()
        .map(|(a, b)| Helper {
            a: a.to_f32(),
            b: b.to_f32(),
            color,
            width: theme.size.outline * scale,
            dash: 0.0,
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
            (cm(140.0), cm(175.0), cm(315.0)),
            ("14".into(), "17,5".into(), "31,5".into())
        );
    }

    /// Rechteck 10 × 8 m im Uhrzeigersinn, Außenkante auf der Bezugslinie.
    fn rechteck() -> WallChain {
        WallChain {
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
        }
    }

    fn value(p: &Props, k: &str) -> String {
        p.values.iter().find(|v| v.0 == k).unwrap().1.clone()
    }

    #[test]
    fn klick_waehlt_wand_und_rueckgaengig_hebt_auf() {
        let mut s = Scene::with_model(Model::with_seed(5));
        let run = s.add_wall(&rechteck()).unwrap();
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
        assert!(s.undo());
        assert!(sel.validate(&s));
        assert_eq!(sel.id, None);
    }

    #[test]
    fn mengen_im_paneel() {
        let mut s = Scene::with_model(Model::with_seed(6));
        let run = s.add_wall(&rechteck()).unwrap();
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
        let before = s.snapshot();
        let vol = s.wall_qto(wall).unwrap().volume;
        // Live-Ziehen der oberen Wand: keine Mengen
        let moved = s
            .chain(run)
            .unwrap()
            .with_segment_moved(1, -1000.0)
            .unwrap();
        s.set_run_points(run, &moved.points);
        assert!(s.wall_qto(wall).is_none());
        // Loslassen: Mengen für den neuen Stand
        s.record(before);
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

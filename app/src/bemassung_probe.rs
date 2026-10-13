//! Tests und Probe zu Review W30, W33, L24, W36, W39 und L29: nur geänderte
//! Geschosse rechnen neu, ein Ebenenwechsel rechnet nichts, Geschosse
//! außerhalb des Bildes fehlen; die Probe misst das erste Bild,
//! Ebenenwechsel, Maus im Freien, Ziehen, Loslassen, Neuzeichnen, Ketten
//! ziehen und loslassen, große Wahl, Tippen und Enter bei 1, 10, 100, 1000,
//! 2000 und 4000 Häusern (je ein Gebäude 10 × 8 m mit einer
//! angeschlossenen Innenwand und einem Fenster, alle EG auf ±0,00), dazu
//! 1000 dicht stehende.
//!
//! Probe: `cargo test --release -p skizzeo probe_bemassung -- --ignored --nocapture`

use super::*;
use crate::camera::Camera;
use crate::wall_edit::mass_treffer::{finde, treffer, Treffer};
use crate::wall_edit::WallEdit;
use sk_math::vec3;
use sk_model::oeffnung::{HeightLine, HeightRef, Opening, OpeningKind};
use sk_model::{Category, RefSide};
use sk_plan::bild::Zahlart;
use sk_platform::{Event, Key, Modifiers, MouseButton};
use std::collections::HashSet;
use std::time::Instant;

fn siedlung(n: usize) -> Scene {
    siedlung_raster(n, 20_000.0, 20_000.0)
}

/// `n` Häuser im Raster `dx` × `dy` (mm).
fn siedlung_raster(n: usize, dx: f64, dy: f64) -> Scene {
    let mut m = Model::new();
    m.begin("Siedlung");
    let seite = (n as f64).sqrt().ceil() as usize;
    for i in 0..n {
        let (ox, oy) = ((i % seite) as f64 * dx, (i / seite) as f64 * dy);
        let p = |x: f64, y: f64| vec3(ox + x, oy + y, 0.0);
        let b = m.add_building(1);
        let pts = [
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10_000.0, 8000.0),
            p(10_000.0, 0.0),
        ];
        let eg = m.build_from_polygon(b, &pts).expect("Haus");
        let st = m.run(eg).unwrap().storey;
        let set = m.defaults().interior_wall;
        m.add_wall_run(
            &[p(4000.0, 0.0), p(4000.0, 5000.0)],
            false,
            RefSide::Center,
            st,
            set,
            Category::InteriorWall,
        )
        .expect("Innenwand");
        m.add_opening(Opening {
            kind: OpeningKind::Window,
            path: vec![p(2510.0, 0.0), p(1500.0, 0.0)],
            bottom: HeightRef::Line(st, HeightLine::Sill),
            top: HeightRef::Line(st, HeightLine::Lintel),
            reveal: None,
            install: Default::default(),
            swing: None,
            lock: Vec::new(),
        })
        .expect("Fenster");
    }
    m.commit();
    Scene::with_model(m)
}

const W: f64 = 1600.0;
const H: f64 = 900.0;

fn schrift() -> Option<Font> {
    std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
        .ok()
        .and_then(Font::parse)
}

/// Außenwandzug im Geschoss `st`.
fn zug(s: &Scene, st: StoreyId) -> sk_model::RunId {
    s.model()
        .runs()
        .iter()
        .find(|(_, r)| r.storey == st && r.closed)
        .map(|(id, _)| id)
        .unwrap()
}

fn werte(g: &Geschoss) -> Vec<f64> {
    g.b.zahlen.iter().map(|z| z.wert).collect()
}

/// Beim Ziehen rechnet nur das Geschoss des gezogenen Zugs neu, Abbrechen
/// und Rückgängig stellen es wieder her, ein anderer Stil rechnet alle.
#[test]
fn nur_das_geaenderte_geschoss_rechnet_neu() {
    let Some(f) = schrift() else {
        eprintln!("Liberation Sans fehlt, Test übersprungen");
        return;
    };
    let mut s = siedlung(3);
    let mut mk = Massketten::default();
    let st = geschosse(&s, ViewKind::Plan, &[]);
    assert_eq!(st.len(), 3);
    let a = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert_eq!(a.geschosse.len(), 3);
    // Ohne Änderung derselbe Stand
    assert!(Rc::ptr_eq(&a, &mk.stand(&s, st.clone(), Some(&f)).unwrap()));
    let run = zug(&s, st[0]);
    let c = s.chain(run).unwrap().clone();
    s.begin("Wand verschieben");
    s.set_run_points(run, &c.with_segment_moved(1, 500.0).unwrap().points);
    let b = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!(!Rc::ptr_eq(&a.geschosse[0], &b.geschosse[0]));
    assert_ne!(werte(&a.geschosse[0]), werte(&b.geschosse[0]));
    assert!(Rc::ptr_eq(&a.geschosse[1], &b.geschosse[1]));
    assert!(Rc::ptr_eq(&a.geschosse[2], &b.geschosse[2]));
    // Abbrechen: wieder die alten Maße
    s.rollback();
    let r = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert_eq!(werte(&a.geschosse[0]), werte(&r.geschosse[0]));
    // Loslassen, dann Rückgängig
    s.begin("Wand verschieben");
    s.set_run_points(run, &c.with_segment_moved(1, 500.0).unwrap().points);
    s.commit();
    let l = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert_eq!(werte(&b.geschosse[0]), werte(&l.geschosse[0]));
    assert!(s.undo());
    let u = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert_eq!(werte(&a.geschosse[0]), werte(&u.geschosse[0]));
    assert!(Rc::ptr_eq(&a.geschosse[2], &u.geschosse[2]));
    // Anderer Stil: alle neu
    s.begin_settings();
    let mut stil = s.model().massstil().clone();
    stil.rundung = 5;
    assert!(s.edit_attr(|m| m.set_massstil(stil)));
    let n = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!((0..3).all(|i| !Rc::ptr_eq(&u.geschosse[i], &n.geschosse[i])));
}

/// Auf eine andere Ebene und zurück rechnet und zeichnet nichts neu (Review
/// W33 zu #116); gemerkt bleiben nur Geschosse, die es noch gibt.
#[test]
fn ebenenwechsel_rechnet_nichts_neu() {
    let Some(f) = schrift() else {
        eprintln!("Liberation Sans fehlt, Test übersprungen");
        return;
    };
    let mut s = siedlung(3);
    let mut mk = Massketten::default();
    let st = geschosse(&s, ViewKind::Plan, &[]);
    let a = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    // Eine Ebene ohne Ketten, dann zurück
    let leer = mk.stand(&s, Vec::new(), Some(&f)).unwrap();
    assert!(leer.geschosse.is_empty());
    let b = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!((0..3).all(|i| Rc::ptr_eq(&a.geschosse[i], &b.geschosse[i])));
    // Ein Gebäude gelöscht: sein Geschoss fällt aus dem Gemerkten
    let haus = s.model().storey(st[2]).and_then(|g| g.building).unwrap();
    assert!(s.remove_building(haus));
    let rest = geschosse(&s, ViewKind::Plan, &[]);
    assert_eq!(rest, st[..2]);
    mk.stand(&s, rest, Some(&f)).unwrap();
    assert_eq!(mk.ketten.len(), 2);
    assert!(!mk.ketten.contains_key(&st[2]) && !mk.geschosse.contains_key(&st[2]));
}

/// Ohne Änderung kommt der gemerkte Stand, ohne die Geschosse einzeln zu
/// prüfen (Review L24). Eine andere Szene mit denselben Geschossnummern,
/// ein Versatz und sein Rückgängig gelten als Änderung.
#[test]
fn ohne_aenderung_schnell_zurueck() {
    let Some(f) = schrift() else {
        return;
    };
    let mut s = siedlung(3);
    let t = siedlung_raster(3, 30_000.0, 30_000.0);
    let st = geschosse(&s, ViewKind::Plan, &[]);
    assert_eq!(geschosse(&t, ViewKind::Plan, &[]), st);
    let rahmen = |x: &Stand| x.geschosse.iter().map(|g| g.rahmen).collect::<Vec<_>>();
    let mut mk = Massketten::default();
    let a = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!(Rc::ptr_eq(&a, &mk.stand(&s, st.clone(), Some(&f)).unwrap()));
    // Andere Szene: ihre Häuser, nicht die gemerkten
    let b = mk.stand(&t, st.clone(), Some(&f)).unwrap();
    let frisch = Massketten::default()
        .stand(&t, st.clone(), Some(&f))
        .unwrap();
    assert_eq!(rahmen(&b), rahmen(&frisch));
    assert_ne!(rahmen(&a), rahmen(&b));
    assert_eq!(
        rahmen(&a),
        rahmen(&mk.stand(&s, st.clone(), Some(&f)).unwrap())
    );
    // Kette nach außen, dann Rückgängig
    s.begin("Kette verschieben");
    assert!(s.set_massversatz(st[0], 270, Kettenart::Gesamt, 500.0));
    s.commit();
    let v = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!(v.geschosse[0].rahmen[0].y < a.geschosse[0].rahmen[0].y - 400.0);
    assert!(Rc::ptr_eq(
        &v.geschosse[1],
        &mk.stand(&s, st.clone(), Some(&f)).unwrap().geschosse[1]
    ));
    assert!(s.undo());
    let u = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert_eq!(rahmen(&u), rahmen(&a));
}

/// Außerhalb des Bildes fehlt ein Geschoss ganz; weit herausgezoomt bleiben
/// die Linien ohne Kreise und ohne Schrift, nah ist alles da.
#[test]
fn ausserhalb_und_weit_weg() {
    let Some(f) = schrift() else {
        return;
    };
    let s = siedlung(4);
    let theme = Theme::dark();
    let mut mk = Massketten::default();
    let oben = |x: f64, y: f64, halb: f64| {
        Camera::parallel(
            vec3(x, y, 0.0),
            90f64.to_radians(),
            -89.9f64.to_radians(),
            halb,
        )
    };
    let mut zeichnen = |cam: &Camera| {
        let b = Blick { cam, w: W, h: H };
        mk.zeichnen(
            &s,
            ViewKind::Plan,
            &[],
            Some(&f),
            &b,
            1.0,
            &theme,
            &Default::default(),
        )
    };
    // Nah am ersten Haus: nur seine Schrift, mit Kreisen
    let (l_nah, t_nah) = zeichnen(&oben(5000.0, 4000.0, 5000.0));
    assert_eq!(t_nah.len(), 1);
    // Weit weg von allen: nichts
    let (l, t) = zeichnen(&oben(500_000.0, 500_000.0, 9000.0));
    assert!(l.is_empty() && t.is_empty());
    // Alle Häuser winzig: Linien ohne Kreise, keine Schrift
    let (l_weit, t_weit) = zeichnen(&oben(15_000.0, 15_000.0, 400_000.0));
    assert!(t_weit.is_empty());
    assert!(!l_weit.is_empty() && l_weit.iter().all(|h| !h.round));
    assert!(l_weit.len() < 4 * l_nah.len());
}

/// Zwei Häuser 10 × 8 m, `luecke` mm zwischen den Außenwänden.
fn zwei(luecke: f64) -> Scene {
    let mut m = Model::new();
    m.begin("Zwei Häuser");
    for x in [0.0, 10_000.0 + luecke] {
        let b = m.add_building(1);
        let p = |dx: f64, y: f64| vec3(x + dx, y, 0.0);
        let pts = [
            p(0.0, 0.0),
            p(0.0, 8000.0),
            p(10_000.0, 8000.0),
            p(10_000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).expect("Haus");
    }
    m.commit();
    Scene::with_model(m)
}

/// Achsparallele Kästen der Maßzahlen eines Geschosses.
fn kaesten(g: &Geschoss) -> Vec<[f64; 4]> {
    g.b.zahlen
        .iter()
        .map(|z| {
            z.rahmen
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |k, p| {
                    [k[0].min(p.x), k[1].min(p.y), k[2].max(p.x), k[3].max(p.y)]
                })
        })
        .collect()
}

/// Zwei Häuser 6 m auseinander (Einstellungen zu #116, B2): keine Maßzahl
/// überdeckt eine des anderen Hauses; zueinander weicht ein Gesamtmaß, dann
/// hat das andere Platz.
/// Rückt ein Haus weg, kommen sie wieder, und das Bild des Nachbarn wird
/// neu; ändert sich am Weichen nichts, bleibt es.
#[test]
fn zwei_haeuser_sechs_meter() {
    let Some(f) = schrift() else {
        return;
    };
    let mut s = zwei(6000.0);
    let mut mk = Massketten::default();
    let st = geschosse(&s, ViewKind::Plan, &[]);
    assert_eq!(st.len(), 2);
    let a = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    let (l, r) = (kaesten(&a.geschosse[0]), kaesten(&a.geschosse[1]));
    let ueber = l
        .iter()
        .flat_map(|p| r.iter().map(move |q| (p, q)))
        .filter(|(p, q)| p[0] < q[2] && q[0] < p[2] && p[1] < q[3] && q[1] < p[3])
        .count();
    assert_eq!(ueber, 0);
    // Zusammen eine Zahl weniger als allein: das linke Gesamtmaß nach Osten
    let allein: Vec<usize> = st
        .iter()
        .map(|&g| {
            let mut m = Massketten::default();
            m.stand(&s, vec![g], Some(&f)).unwrap().geschosse[0]
                .b
                .zahlen
                .len()
        })
        .collect();
    assert_eq!([l.len() + 1, r.len()], allein[..]);
    // Die Nordwand des rechten Hauses rückt: am Weichen ändert sich nichts,
    // das linke Haus bleibt
    let run = zug(&s, st[1]);
    let c = s.chain(run).unwrap().clone();
    let nord = (0..c.points.len())
        .find(|&i| {
            let (p, q) = (c.points[i], c.points[(i + 1) % c.points.len()]);
            p.y > 7000.0 && q.y > 7000.0
        })
        .unwrap();
    s.begin("Wand verschieben");
    s.set_run_points(run, &c.with_segment_moved(nord, 500.0).unwrap().points);
    let b = mk.stand(&s, st.clone(), Some(&f)).unwrap();
    assert!(Rc::ptr_eq(&a.geschosse[0], &b.geschosse[0]));
    assert!(!Rc::ptr_eq(&a.geschosse[1], &b.geschosse[1]));
    s.rollback();
    // Rückt das rechte Haus 20 m weg, steht beim linken alles wieder
    let w = zwei(20_000.0);
    let n = mk
        .stand(&w, geschosse(&w, ViewKind::Plan, &[]), Some(&f))
        .unwrap();
    assert_eq!(n.geschosse[0].b.zahlen.len(), allein[0]);
}

/// Die Maus fragt nur Geschosse am Blickstrahl ab und verpasst keins, das
/// sie trifft: im Grundriss nah und fern, in 3D von oben und flach über
/// die Siedlung (Review L29).
#[test]
fn hover_nur_am_strahl() {
    let Some(f) = schrift() else {
        return;
    };
    let s = siedlung(100);
    let mut mk = Massketten::default();
    let st = mk.gezeigt(&s, ViewKind::Plan, &[], Some(&f)).unwrap();
    assert_eq!(st.geschosse.len(), 100);
    let (lo, hi) = s.bounds().unwrap();
    let mitte = (lo + hi) * 0.5;
    let oben =
        |p: Vec3, halb: f64| Camera::parallel(p, 90f64.to_radians(), -89.9f64.to_radians(), halb);
    for (k, cam) in [
        oben(mitte, 120_000.0),
        oben(vec3(5000.0, 4000.0, 0.0), 12_000.0),
        Camera::looking_at(
            mitte + vec3(0.55, -1.0, 0.9).normalized() * 300_000.0,
            mitte,
            45.0,
        ),
        Camera::looking_at(vec3(-20_000.0, 90_000.0, 1700.0), mitte, 60.0),
    ]
    .into_iter()
    .enumerate()
    {
        let blick = Blick {
            cam: &cam,
            w: W,
            h: H,
        };
        let (mut getroffen, mut gefragt) = (0, 0);
        for i in 0..(32 * 18) {
            let (x, y) = ((i % 32) as f64 * W / 31.0, (i / 32) as f64 * H / 17.0);
            let strahl = cam.ray(x, y, W, H);
            let nah = st.am_strahl(&blick, strahl, 12.0);
            gefragt += nah.len();
            for g in &st.geschosse {
                if g.unter_strahl(&blick, strahl, 12.0).is_some() {
                    assert!(
                        nah.iter().any(|n| std::ptr::eq(*n, &**g)),
                        "Kamera {k}, {x} {y}"
                    );
                    getroffen += 1;
                }
            }
        }
        assert!(getroffen > 0, "Kamera {k}");
        // Im Schnitt nur ein Bruchteil der Geschosse je Mausbewegung
        assert!(gefragt < 32 * 18 * 100 / 4, "Kamera {k}: {gefragt}");
    }
}

fn reps_von(n: usize) -> usize {
    if n >= 1000 {
        5
    } else {
        20
    }
}

/// Eine hervorgehobene Kette malt nur ihr Geschoss neu: Gleiche Farben
/// behalten den Schlüssel der Tinte, ohne Hervorhebung gilt wieder die
/// gemerkte (Stufe 2 auf dem Zwischenspeicher aus Review W30).
#[test]
fn hervorhebung_malt_nur_ihr_geschoss() {
    let Some(f) = schrift() else {
        return;
    };
    let s = siedlung(2);
    let theme = Theme::dark();
    let mut mk = Massketten::default();
    let cam = Camera::parallel(
        vec3(15_000.0, 4000.0, 0.0),
        90f64.to_radians(),
        -89.9f64.to_radians(),
        20_000.0,
    );
    let b = Blick {
        cam: &cam,
        w: W,
        h: H,
    };
    let mut schluessel = |h: &crate::wall_edit::mass_edit::Hervor| -> Vec<u64> {
        let (_, t) = mk.zeichnen(&s, ViewKind::Plan, &[], Some(&f), &b, 1.0, &theme, h);
        t.iter().map(|t| t.0).collect()
    };
    let ohne = schluessel(&Default::default());
    assert_eq!(ohne.len(), 2);
    let st = geschosse(&s, ViewKind::Plan, &[]);
    let a = Massketten::default().stand(&s, st, Some(&f)).unwrap();
    let k = crate::wall_edit::mass_treffer::id_von(&a.geschosse[0].a, 0, 0).unwrap();
    let h = crate::wall_edit::mass_edit::Hervor {
        ueber: Some(k),
        ..Default::default()
    };
    let hell = schluessel(&h);
    assert_eq!(hell.iter().filter(|k| ohne.contains(k)).count(), 1);
    assert_eq!(schluessel(&h), hell, "gleiche Farben, gleicher Schlüssel");
    assert_eq!(schluessel(&Default::default()), ohne);
}

fn ms(t: Instant, reps: usize) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0 / reps as f64
}

/// Bedienen nah am ersten Haus wie in der App ([`crate::bemassung_app`]):
/// je Ereignis die Ketten der Ansicht, das Ereignis und das Bild. Ergebnis
/// in ms: Gesamtkette Süd ziehen je Mausschritt, je Taste beim
/// Überschreiben ihres Maßes (Live-Vorschau) und Enter (Review W42,
/// Hinweis zur Probe).
fn bedienen(
    s: &mut Scene,
    mk: &mut Massketten,
    f: &Font,
    cam: &Camera,
    theme: &Theme,
    reps: usize,
) -> (f64, f64, f64) {
    let blick = Blick { cam, w: W, h: H };
    let mut e = WallEdit::default();
    let ev = |e: &mut WallEdit, s: &mut Scene, mk: &mut Massketten, x: Event| {
        let st = mk.gezeigt(s, ViewKind::Plan, &[], Some(f));
        e.masse.setze(st, s);
        e.handle(&x, s, cam, W, H, 1.0, true);
        let h = e.mass_hervor(s);
        std::hint::black_box(mk.zeichnen(s, ViewKind::Plan, &[], Some(f), &blick, 1.0, theme, &h));
    };
    let m = Modifiers::default();
    let taste = |key| Event::Key {
        key,
        down: true,
        repeat: false,
        mods: m,
    };
    // Gesamtkette Süd des ersten Hauses: Zahl und Linie
    let gesamt = |s: &Scene, mk: &mut Massketten| {
        let st = mk.gezeigt(s, ViewKind::Plan, &[], Some(f)).unwrap();
        let mut best: Option<(f64, Vec3, KettenId)> = None;
        for (a, b) in st.paare() {
            for z in &b.zahlen {
                let Zahlart::Mass { seite, kette, .. } = z.art else {
                    continue;
                };
                let sd = &a.seiten[seite];
                if sd.grad != 270 || sd.ketten[kette].art != Kettenart::Gesamt {
                    continue;
                }
                let p = z.rahmen.iter().fold(Vec3::ZERO, |s, p| s + *p) * 0.25;
                let d = (p - vec3(5000.0, -3000.0, 0.0)).length();
                if best.is_none_or(|b| d < b.0) {
                    let k = KettenId {
                        st: a.storey,
                        grad: 270,
                        art: Kettenart::Gesamt,
                    };
                    best = Some((d, p, k));
                }
            }
        }
        let (_, p, k) = best.unwrap();
        let fund = finde(&st, k).unwrap();
        let linie = fund.kette.lage;
        let n = fund.seite.n;
        let zahl = cam.project(p, W, H).unwrap();
        assert!(matches!(
            treffer(&st, cam, W, H, 1.0, zahl.0, zahl.1),
            Some(Treffer::Zahl(_))
        ));
        // Auf der Linie neben der Zahl
        let am = p + n * (linie - p.dot(n)) - fund.seite.t * 1500.0;
        (zahl, am, n)
    };
    let maus = |x: f64, y: f64| {
        [
            Event::MouseMove { x, y, mods: m },
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                mods: m,
            },
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                mods: m,
            },
        ]
    };
    // Kette ziehen: greifen, dann je Mausschritt 10 mm nach außen
    let (_, am, n) = gesamt(s, mk);
    let a = cam.project(am, W, H).unwrap();
    let [mv, down, _] = maus(a.0, a.1);
    ev(&mut e, s, mk, mv);
    ev(&mut e, s, mk, down);
    let schritt = |k: usize| {
        let q = cam
            .project(am + n * (300.0 + 10.0 * k as f64), W, H)
            .unwrap();
        Event::MouseMove {
            x: q.0,
            y: q.1,
            mods: m,
        }
    };
    ev(&mut e, s, mk, schritt(0));
    let t = Instant::now();
    for k in 1..=reps {
        ev(&mut e, s, mk, schritt(k));
    }
    let kette_z = ms(t, reps);
    let q = cam.project(am + n * 300.0, W, H).unwrap();
    ev(
        &mut e,
        s,
        mk,
        Event::MouseUp {
            button: MouseButton::Left,
            x: q.0,
            y: q.1,
            mods: m,
        },
    );
    // Überschreiben: Zahl anklicken, je Taste Live-Vorschau und Bild
    let (z, ..) = gesamt(s, mk);
    for x in maus(z.0, z.1) {
        ev(&mut e, s, mk, x);
    }
    assert!(e.input().is_some(), "Feld offen");
    let tasten = [
        Key::Char('1'),
        Key::Char('1'),
        Key::Backspace,
        Key::Backspace,
    ];
    let t = Instant::now();
    for _ in 0..reps {
        for k in tasten {
            ev(&mut e, s, mk, taste(k));
        }
    }
    let je_taste = ms(t, reps * tasten.len());
    ev(&mut e, s, mk, taste(Key::Escape));
    // Enter: je Runde neu anklicken und tippen, gemessen nur das Enter
    let mut dauer = 0.0;
    let serial = s.step_serial();
    for k in 0..reps {
        let (z, ..) = gesamt(s, mk);
        for x in maus(z.0, z.1) {
            ev(&mut e, s, mk, x);
        }
        for ch in if k % 2 == 0 { "11" } else { "10" }.chars() {
            ev(&mut e, s, mk, taste(Key::Char(ch)));
        }
        let t = Instant::now();
        ev(&mut e, s, mk, taste(Key::Enter));
        dauer += ms(t, 1);
    }
    assert_eq!(
        s.step_serial(),
        serial + reps as u64,
        "je Enter ein Schritt"
    );
    (kette_z, je_taste, dauer / reps as f64)
}

#[test]
#[ignore]
fn probe_bemassung() {
    let Some(f) = std::fs::read("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf")
        .ok()
        .and_then(Font::parse)
    else {
        eprintln!("Liberation Sans fehlt, Probe übersprungen");
        return;
    };
    let theme = Theme::dark();
    println!();
    println!(
        "{:>7} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8}",
        "Häuser",
        "Kalt",
        "Stand",
        "Ebene",
        "Hover",
        "Wand",
        "Ziehen",
        "ZiehNah",
        "Hochlad.",
        "Zeichn.",
        "Loslass",
        "Drehen",
        "KetteLos",
        "Wahl",
        "KetteZ",
        "Taste",
        "Enter",
        "Linien",
        "Dreiecke",
        "Teile",
        "TeilNah"
    );
    // Weit: 10 m zwischen den Häusern, nichts weicht; eng: 6 m bzw. 7 m,
    // überall weicht etwas
    for (n, eng) in [
        (1, false),
        (10, false),
        (100, false),
        (1000, false),
        (2000, false),
        (4000, false),
        (1000, true),
    ] {
        let mut s = if eng {
            siedlung_raster(n, 16_000.0, 15_000.0)
        } else {
            siedlung(n)
        };
        let mut mk = Massketten::default();
        // Ganze Siedlung im Bild, wie nach „Alles zeigen“
        let (lo, hi) = s.bounds().unwrap();
        let mitte = (lo + hi) * 0.5;
        let halb = 0.5 * (hi.y - lo.y).max((hi.x - lo.x) * H / W) + 6000.0;
        let cam = Camera::parallel(mitte, 90f64.to_radians(), -89.9f64.to_radians(), halb);
        let cam3 = Camera::looking_at(
            mitte + vec3(0.55, -1.0, 0.9).normalized() * (3.0 * halb),
            mitte,
            45.0,
        );
        let blick = Blick {
            cam: &cam,
            w: W,
            h: H,
        };
        let blick3 = Blick {
            cam: &cam3,
            w: W,
            h: H,
        };
        // Nah am ersten Haus, etwa 1:100 am Bildschirm
        let nah = Camera::parallel(
            vec3(5000.0, 4000.0, 0.0),
            90f64.to_radians(),
            -89.9f64.to_radians(),
            12_000.0,
        );
        let blick_nah = Blick {
            cam: &nah,
            w: W,
            h: H,
        };
        // Wie der Renderer: hochgeladen werden nur Teile mit neuem Schlüssel
        let mut da: HashSet<u64> = HashSet::new();
        let mut hoch = |ink: &[super::Teil]| -> usize {
            let n = ink
                .iter()
                .filter(|(k, _)| !da.contains(k))
                .map(|(_, t)| t.len())
                .sum();
            da = ink.iter().map(|(k, _)| *k).collect();
            n
        };
        let t = Instant::now();
        let (linien, ink) = mk.zeichnen(
            &s,
            ViewKind::Plan,
            &[],
            Some(&f),
            &blick,
            1.0,
            &theme,
            &Default::default(),
        );
        let kalt = ms(t, 1);
        hoch(&ink);
        // Stand ohne Änderung, dann auf eine andere Ebene und zurück
        let st = geschosse(&s, ViewKind::Plan, &[]);
        // (je 50-mal: einzeln zu kurz zum Messen)
        let t = Instant::now();
        for _ in 0..50 {
            std::hint::black_box(mk.stand(&s, st.clone(), Some(&f)));
        }
        let stand = ms(t, 50);
        let t = Instant::now();
        for _ in 0..50 {
            mk.stand(&s, Vec::new(), Some(&f));
            mk.stand(&s, st.clone(), Some(&f));
        }
        let ebene = ms(t, 50);
        // Maus im Freien zwischen den Häusern: Ketten der Ansicht vor dem
        // Ereignis und was darunter liegt (Review W36)
        let frei = cam.project(vec3(15_000.0, 14_000.0, 0.0), W, H).unwrap();
        let t = Instant::now();
        for _ in 0..50 {
            let st = mk.gezeigt(&s, ViewKind::Plan, &[], Some(&f)).unwrap();
            std::hint::black_box(treffer(&st, &cam, W, H, 1.0, frei.0, frei.1));
        }
        let hover = ms(t, 50);
        let dreiecke: usize = ink.iter().map(|(_, t)| t.len()).sum();
        let run = s.model().runs().ids().next().unwrap();
        let orig = s.chain(run).unwrap().clone();
        let reps = reps_von(n);
        s.begin("Wand verschieben");
        // Nur die Wand: Grundlast ohne Bemaßung
        let t = Instant::now();
        for k in 0..reps {
            let moved = orig.with_segment_moved(1, 10.0 * (k + 1) as f64).unwrap();
            s.set_run_points(run, &moved.points);
        }
        let wand = ms(t, reps);
        // Wand und Bemaßung je Mausschritt; hochzuladen je Schritt
        let mut hochgeladen = 0;
        let t = Instant::now();
        for k in 0..reps {
            let moved = orig.with_segment_moved(1, -10.0 * (k + 1) as f64).unwrap();
            s.set_run_points(run, &moved.points);
            let (_, ink) = mk.zeichnen(
                &s,
                ViewKind::Plan,
                &[],
                Some(&f),
                &blick,
                1.0,
                &theme,
                &Default::default(),
            );
            hochgeladen = hoch(&ink);
        }
        let ziehen = ms(t, reps) - wand;
        // Dasselbe nah am Haus
        let t = Instant::now();
        let mut teile_nah = 0;
        for k in 0..reps {
            let moved = orig.with_segment_moved(1, -10.0 * (k + 1) as f64).unwrap();
            s.set_run_points(run, &moved.points);
            let (_, ink) = mk.zeichnen(
                &s,
                ViewKind::Plan,
                &[],
                Some(&f),
                &blick_nah,
                1.0,
                &theme,
                &Default::default(),
            );
            teile_nah = ink.len();
        }
        let ziehen_nah = ms(t, reps) - wand;
        // Neuzeichnen ohne Änderung (Zoomen, Verschieben)
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(mk.zeichnen(
                &s,
                ViewKind::Plan,
                &[],
                Some(&f),
                &blick,
                1.0,
                &theme,
                &Default::default(),
            ));
        }
        let zeichnen = ms(t, reps);
        let t = Instant::now();
        s.commit();
        std::hint::black_box(mk.zeichnen(
            &s,
            ViewKind::Plan,
            &[],
            Some(&f),
            &blick,
            1.0,
            &theme,
            &Default::default(),
        ));
        let los = ms(t, 1);
        // In 3D: erstes Bild baut die verdeckbare Tinte, dann Drehen
        mk.zeichnen(
            &s,
            ViewKind::Persp,
            &[],
            Some(&f),
            &blick3,
            1.0,
            &theme,
            &Default::default(),
        );
        let t = Instant::now();
        for _ in 0..reps {
            std::hint::black_box(mk.zeichnen(
                &s,
                ViewKind::Persp,
                &[],
                Some(&f),
                &blick3,
                1.0,
                &theme,
                &Default::default(),
            ));
        }
        let drehen = ms(t, reps);
        // Kette loslassen: Versatz als Schritt, dann das Bild (Review W39)
        let st0 = geschosse(&s, ViewKind::Plan, &[])[0];
        let zeichne = |mk: &mut Massketten, s: &Scene, h: &Hervor| {
            std::hint::black_box(mk.zeichnen(
                s,
                ViewKind::Plan,
                &[],
                Some(&f),
                &blick,
                1.0,
                &theme,
                h,
            ));
        };
        let t = Instant::now();
        for k in 0..reps {
            s.begin(crate::wall_edit::mass_edit::SCHRITT_KETTE);
            s.set_massversatz(st0, 270, Kettenart::Gesamt, 500.0 + 10.0 * (k % 2) as f64);
            s.commit();
            zeichne(&mut mk, &s, &Default::default());
        }
        let kette_los = ms(t, reps);
        // Alle Ketten gewählt: je Bild ohne Änderung
        let gezeigt = mk.gezeigt(&s, ViewKind::Plan, &[], Some(&f)).unwrap();
        let alle = Hervor {
            gewaehlt: gezeigt
                .paare()
                .flat_map(|(a, _)| {
                    (0..a.seiten.len()).flat_map(move |si| {
                        (0..a.seiten[si].ketten.len()).filter_map(move |ki| id_von(a, si, ki))
                    })
                })
                .collect(),
            ..Default::default()
        };
        zeichne(&mut mk, &s, &alle);
        let t = Instant::now();
        for _ in 0..reps {
            zeichne(&mut mk, &s, &alle);
        }
        let wahl = ms(t, reps);
        let (kette_z, taste, enter) = bedienen(&mut s, &mut mk, &f, &nah, &theme, reps);
        println!(
            "{:>7} {:>8.2} {:>8.3} {:>8.2} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>9} {:>8.3} {:>8.2} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8} {:>9} {:>8} {:>8}",
            format!("{n}{}", if eng { " eng" } else { "" }),
            kalt,
            stand,
            ebene,
            hover,
            wand,
            ziehen,
            ziehen_nah,
            hochgeladen,
            zeichnen,
            los,
            drehen,
            kette_los,
            wahl,
            kette_z,
            taste,
            enter,
            linien.len(),
            dreiecke,
            ink.len(),
            teile_nah
        );
    }
    println!(
        "ms je Aufruf. Kalt = erstes Bild. Stand = Ketten ohne Änderung, Ebene = auf eine \
         andere Ebene und zurück, Hover = Maus im Freien (Ketten der Ansicht und \
         Treffer). Wand = Wand ziehen ohne Bemaßung, Ziehen = was \
         die Bemaßung je Mausschritt dazu kostet, Hochlad. = Dreiecke, die dabei neu zur \
         Grafikkarte gehen; ZiehNah = Ziehen nah am Haus (etwa 1:100 am Bildschirm). \
         Zeichn. = Bild ohne Änderung, Loslass = Loslassen und erstes \
         Bild, Drehen = Bild in 3D ohne Änderung, \
         KetteLos = Kette loslassen (Versatz als Schritt) und Bild, Wahl = Bild mit allen \
         Ketten gewählt; nah am ersten Haus wie in der App je Ereignis Ketten der Ansicht, \
         Ereignis und Bild: KetteZ = Gesamtkette ziehen je Mausschritt, Taste = je Taste \
         beim Überschreiben des Gesamtmaßes (Live-Vorschau), Enter = Übernehmen. Linien, Dreiecke, Teile = im Bild der ganzen \
         Siedlung (weit weg ohne Schrift und Kreise), TeilNah = Geschosse mit Schrift nah am \
         Haus."
    );
}

use super::*;

// Abnahmetest A302: Teilbilder statt ganzer Fensterbilder (nach Paket 9,
// Punkt 2: offen B6 Baustoff-Fenster, U6/U7 Katalog und Einstellungen;
// planung/nach-paket-9.md, Darstellungsregeln
// einstellungen/paket-n9-teilbilder-darstellung.md §2.1–2.3, Koordinator
// 22:12 und 22:13). Vorbereitet gegen main 38b94e2.
//
// Einbau: als `mod teilbilder { use super::*; … }` ans Ende von
// app/src/abnahme.rs. Nutzt aus abnahme.rs: gebaeude, mv.
//
// Geprüft ohne Fenster: Das letzte ganze Bild plus alle Teilbilder danach
// ist Pixel für Pixel gleich dem ganz neu gemalten Bild eines zweiten,
// gleich bedienten Fensters (vormultipliziertes RGBA8, Abweichung 0,
// §2.1). Die Maus fährt Zeile für Zeile, springt schnell über mehrere
// Zeilen und verlässt das Fenster (§2.2, keine Hover-Reste). Bei 100,
// 150 und 200 %. Mit `anim_ms` = 0, damit beide Fenster denselben Stand
// zeigen (§2.3); Übergänge selbst prüft Handtest H188.
//
// Angenommener Name nur im Adapter: `Prefs::paint_frame(t, fonts, w,
// scene) -> Frame` wie beim Katalog. Baustoffe und Katalog nutzen die
// vorhandenen `paint_frame`.

use crate::catalog_view::{Catalog, Frame};
use crate::material_view::MaterialView;
use crate::prefs::{Prefs, Win};
use sk_ui::widgets::Fonts;

// ===== Adapter =====

enum Fenster {
    Baustoffe(Box<MaterialView>),
    Katalog(Box<Catalog>),
    Einstellungen(Box<Prefs>),
}

// Die Einstellungen mit jedem Reiter: Linientypen, Schraffuren und
// Oberflächen haben gemerkte Vorschauen (Review 3x)
const ARTEN: [&str; 7] = [
    "Baustoffe",
    "Katalog",
    "Einstellungen",
    "Einstellungen · Linientypen",
    "Einstellungen · Schraffuren",
    "Einstellungen · Oberflächen",
    "Einstellungen · Bedienoberfläche",
];

fn oeffnen(art: &str, s: &mut Scene, th: &Theme) -> Fenster {
    use crate::prefs::Tab;
    let tab = match art {
        "Baustoffe" => return Fenster::Baustoffe(Box::new(MaterialView::open(s))),
        "Katalog" => return Fenster::Katalog(Box::new(Catalog::open(s, None))),
        "Einstellungen · Linientypen" => Tab::LineTypes,
        "Einstellungen · Schraffuren" => Tab::Fills,
        "Einstellungen · Oberflächen" => Tab::Surfaces,
        "Einstellungen · Bedienoberfläche" => Tab::Ui,
        _ => Tab::Pens,
    };
    let mut p = Prefs::open(s, th);
    p.show_tab(tab);
    Fenster::Einstellungen(Box::new(p))
}

fn einstellungen_ohne_datei() -> crate::settings::Settings {
    crate::settings::Settings::new(
        ["skizzeo", "--ohne-einstellungen"]
            .map(String::from)
            .into_iter(),
        None,
    )
}

/// Ereignis an das Fenster; `true`, wenn es neu gemalt werden will.
fn ereignis(f: &mut Fenster, e: &Event, s: &mut Scene, th: &Theme, fonts: &Fonts, w: Win) -> bool {
    match f {
        Fenster::Baustoffe(v) => {
            let mut cx = crate::material_view::Ctx {
                scene: s,
                theme: th,
                fonts,
                win: w,
                company: None,
                company_standard: false,
            };
            v.handle(e, &mut cx).repaint
        }
        Fenster::Katalog(c) => {
            let mut cx = crate::catalog_view::Ctx {
                scene: s,
                theme: th,
                fonts,
                win: w,
                company: None,
                company_standard: false,
            };
            c.handle(e, &mut cx).repaint
        }
        Fenster::Einstellungen(p) => {
            let mut t = th.clone();
            let mut st = einstellungen_ohne_datei();
            let mut cx = crate::prefs::Ctx {
                scene: s,
                theme: &mut t,
                settings: &mut st,
                fonts,
                win: w,
            };
            p.handle(e, &mut cx).repaint
        }
    }
}

/// Nächstes Bild für die App (ganz oder in Teilen).
fn teilbild(f: &mut Fenster, s: &Scene, th: &Theme, fonts: &Fonts, w: &Win) -> Frame {
    match f {
        Fenster::Baustoffe(v) => v.paint_frame(th, fonts, w),
        Fenster::Katalog(c) => c.paint_frame(th, fonts, w),
        Fenster::Einstellungen(p) => p.paint_frame(th, fonts, w, s),
    }
}

/// Das ganze Fenster neu gemalt: Breite, Höhe, vormultipliziertes RGBA8.
fn vollbild(
    f: &mut Fenster,
    s: &Scene,
    th: &Theme,
    fonts: &Fonts,
    w: &Win,
) -> (usize, usize, Vec<u8>) {
    let c = match f {
        Fenster::Baustoffe(v) => v.paint(th, fonts, w).0,
        Fenster::Katalog(c) => c.paint(th, fonts, w).0,
        Fenster::Einstellungen(p) => p.paint(th, fonts, w, s).0,
    };
    (c.width, c.height, c.to_premul_rgba8())
}

// ===== Test =====

/// Was die App zeigt: zuletzt hochgeladenes Bild samt Lage.
struct Schirm {
    x: i32,
    y: i32,
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Schirm {
    /// Bild übernehmen; `Some(Fläche)` bei Teilbildern, `None` bei ganzem Bild.
    fn nehmen(&mut self, f: Frame) -> Option<usize> {
        match f {
            Frame::Full { x, y, w, h, px } => {
                (self.x, self.y, self.w, self.h, self.px) = (x, y, w as usize, h as usize, px);
                None
            }
            Frame::Parts(teile) => {
                let mut flaeche = 0;
                for (x0, y0, tw, th, px) in teile {
                    let (x0, y0, tw, th) = (x0 as usize, y0 as usize, tw as usize, th as usize);
                    assert!(x0 + tw <= self.w && y0 + th <= self.h, "Teil im Bild");
                    for r in 0..th {
                        let ziel = ((y0 + r) * self.w + x0) * 4;
                        self.px[ziel..ziel + tw * 4]
                            .copy_from_slice(&px[r * tw * 4..(r + 1) * tw * 4]);
                    }
                    flaeche += tw * th;
                }
                Some(flaeche)
            }
        }
    }
}

fn erstes(f: Frame) -> Schirm {
    let mut s = Schirm {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
        px: Vec::new(),
    };
    assert!(s.nehmen(f).is_none(), "erstes Bild ganz");
    s
}

/// Erste abweichende Stelle (x, y) oder `None`.
fn abweichung(a: &Schirm, b: &(usize, usize, Vec<u8>)) -> Option<(usize, usize)> {
    assert_eq!((a.w, a.h), (b.0, b.1), "Bildgröße");
    // erst als Ganzes vergleichen: im Debug sonst ein Großteil der Laufzeit
    if a.px == b.2 {
        return None;
    }
    a.px.chunks(4)
        .zip(b.2.chunks(4))
        .position(|(p, q)| p != q)
        .map(|i| (i % a.w, i / a.w))
}

/// Mauspfad: im Zickzack über die Zeilen (18 dip, also jede Listenzeile;
/// bei 150 und 200 % alle 54 dip, damit der Test im Debug-Bau unter
/// einer Minute bleibt), dann schnelle Sprünge über mehrere Zeilen,
/// dann hinaus.
fn pfad(sc: &Schirm, scale: f64) -> Vec<(f64, f64)> {
    let (x0, y0) = (sc.x as f64, sc.y as f64);
    let (w, h) = (sc.w as f64, sc.h as f64);
    let mut v = Vec::new();
    let schritt_y = if scale == 1.0 { 18.0 } else { 54.0 } * scale;
    let mut y = 20.0 * scale;
    let mut zeile = 0;
    while y < h - 10.0 * scale {
        let n = 8;
        for i in 0..=n {
            let t = if zeile % 2 == 0 { i } else { n - i } as f64 / n as f64;
            v.push((x0 + 16.0 * scale + t * (w - 32.0 * scale), y0 + y));
        }
        y += schritt_y * if zeile % 3 == 2 { 4.0 } else { 1.0 };
        zeile += 1;
    }
    for k in 0..12 {
        let t = (k * 7 % 12) as f64 / 12.0;
        v.push((x0 + w * (0.2 + 0.6 * t), y0 + h * (0.1 + 0.8 * (1.0 - t))));
    }
    v.push((x0 - 40.0 * scale, y0 - 40.0 * scale));
    v.push((x0 + w * 0.5, y0 + h * 0.5));
    v.push((x0 + w + 60.0 * scale, y0 + h * 0.5));
    v
}

/// A302 (nach Paket 9 Punkt 2; Darstellung §2.1–2.3): Baustoff-Fenster,
/// Bauteilkatalog und Einstellungen.
/// - Nach jedem Überfahren: letztes ganzes Bild + Teilbilder = neu
///   gemaltes ganzes Bild, Abweichung 0, bei 100, 150 und 200 %.
/// - Auch nach schnellen Sprüngen und nach dem Verlassen des Fensters
///   keine Hover-Reste.
/// - Überfahren malt in der Regel nur Teile: mindestens die Hälfte der
///   Hover-Bilder sind Teilbilder, jedes kleiner als das halbe Fenster
///   (B6, U6, U7).
/// - Ein Wechsel des Farbschemas malt das ganze Fenster neu (§2.3): Das
///   nächste Bild gleicht dem ganz gemalten im neuen Schema.
#[test]
fn a302_teilbild_gleich_vollbild() {
    let mut s = Scene::with_model(Model::with_seed(302));
    gebaeude(&mut s);
    let fonts = Fonts::system();
    let mut th = Theme::dark();
    th.size.anim_ms = 0.0;
    // anderes Farbschema: Grund und Schrift getauscht, neue Revision
    let mut hell = th.clone();
    (hell.ui.bg, hell.ui.text) = (th.ui.text, th.ui.bg);
    hell.rev += 1;
    for art in ARTEN {
        for scale in [1.0f32, 1.5, 2.0] {
            let w = Win {
                w: (1440.0 * scale) as u32,
                h: (900.0 * scale) as u32,
                top: (32.0 * scale).round() as u32,
                scale,
            };
            let mut a = oeffnen(art, &mut s, &th);
            let mut b = oeffnen(art, &mut s, &th);
            let mut sc = erstes(teilbild(&mut a, &s, &th, &fonts, &w));
            assert_eq!(
                abweichung(&sc, &vollbild(&mut b, &s, &th, &fonts, &w)),
                None,
                "{art} {scale}: erstes Bild"
            );
            let (mut bilder, mut teile) = (0, 0);
            for (x, y) in pfad(&sc, scale as f64) {
                let e = mv(x, y);
                let neu_a = ereignis(&mut a, &e, &mut s, &th, &fonts, w);
                let neu_b = ereignis(&mut b, &e, &mut s, &th, &fonts, w);
                assert_eq!(neu_a, neu_b, "{art}: beide Fenster gleich bedient");
                if !neu_a {
                    continue;
                }
                bilder += 1;
                if let Some(f) = sc.nehmen(teilbild(&mut a, &s, &th, &fonts, &w)) {
                    teile += 1;
                    assert!(f * 2 < sc.w * sc.h, "{art} {scale}: Teil {f} zu groß");
                }
                assert_eq!(
                    abweichung(&sc, &vollbild(&mut b, &s, &th, &fonts, &w)),
                    None,
                    "{art} {scale}: Hover-Rest nach Maus bei ({x:.0}, {y:.0})"
                );
            }
            assert!(bilder > 0, "{art}: Überfahren malt");
            assert!(
                teile * 2 >= bilder,
                "{art} {scale}: nur {teile} von {bilder} Hover-Bildern als Teilbild"
            );

            // Farbschema gewechselt: ganzes Fenster neu
            sc.nehmen(teilbild(&mut a, &s, &hell, &fonts, &w));
            assert_eq!(
                abweichung(&sc, &vollbild(&mut b, &s, &hell, &fonts, &w)),
                None,
                "{art} {scale}: nach Wechsel des Farbschemas"
            );
        }
    }
}

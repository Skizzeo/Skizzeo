//! Teilpositionen je Dicke im LV (R1-B3, Review #213): Index projektweit,
//! Untertitel, OZ-Spalte.

use crate::scene::Scene;
use sk_cost::befund::Ort;
use sk_cost::lv::{Lv, LvWahl};
use sk_cost::{Dez, Umfang};
use sk_model::{Guid, Model, StoreyId};

/// Haus mit zwei Geschossen und je einem Raum samt Ausbau aus der Vorlage
/// (wie in `abnahme_rb7.rs`): Dämmung unter Estrich 40 mm im Erdgeschoss
/// (Sohlplatte), 45 mm im Obergeschoss (Decke), beide unter Parkett.
fn haus() -> (Scene, Vec<StoreyId>) {
    use sk_math::vec3;
    let mut m = Model::with_seed(9);
    let b = m.add_building(2);
    let pts = [
        vec3(0.0, 0.0, 0.0),
        vec3(0.0, 8000.0, 0.0),
        vec3(10000.0, 8000.0, 0.0),
        vec3(10000.0, 0.0, 0.0),
    ];
    m.build_from_polygon(b, &pts).unwrap();
    let geschosse: Vec<StoreyId> = m
        .levels_in(Some(b))
        .into_iter()
        .filter(|g| m.storey(*g).unwrap().kind == sk_model::LevelKind::Storey)
        .collect();
    m.begin("Räume");
    assert!(sk_raum::abgleichen(&mut m, &geschosse) > 0);
    m.commit();
    for g in &geschosse {
        let id = m.spaces(*g)[0];
        m.begin("Ausbau");
        assert!(sk_raum::ausbau::typen_anlegen(&mut m, id));
        m.commit();
    }
    (Scene::with_model(m), geschosse)
}

fn wahl(s: &mut Scene, g: Guid, untertitel: bool) -> LvWahl {
    let k = s.katalog(None);
    let l = k.leistung(g).unwrap();
    let mut los = l.titel;
    while let Some(p) = k.los(los).and_then(|t| t.parent) {
        los = p;
    }
    LvWahl {
        los,
        untertitel,
        preise: true,
        heute: Some((2030, 1)),
    }
}

fn daemmung(s: &mut Scene) -> Guid {
    s.katalog(None)
        .leistungen
        .iter()
        .find(|l| l.kurz.starts_with("Wärmedämmung EPS 035 DEO unter Estrich"))
        .unwrap()
        .guid
}

/// OZ und Kurztext der Positionen der Bauleistung `g`.
fn teile(lv: &Lv, g: Guid) -> Vec<(String, String)> {
    lv.titel
        .iter()
        .flat_map(|t| &t.positionen)
        .filter(|p| p.quelle == g)
        .map(|p| (p.oz.clone(), p.kurztext.clone()))
        .collect()
}

/// Review #213 B2: Der Index einer Dicke hängt nicht am Umfang. Ohne das
/// Obergeschoss bleibt im Umfang nur 40 mm, die Position heißt trotzdem
/// „….1“; ohne das Erdgeschoss heißt 45 mm „….2“, wie im Projekt.
#[test]
fn index_je_dicke_in_jedem_umfang_gleich() {
    let (mut s, geschosse) = haus();
    let g = daemmung(&mut s);
    let w = wahl(&mut s, g, false);
    let oz = {
        let k = s.katalog(None);
        k.oz(k.leistung(g).unwrap())
    };
    let projekt = teile(&s.lv(None, &Umfang::projekt(), &w), g);
    assert_eq!(
        projekt,
        vec![
            (
                format!("{oz}.1"),
                "Wärmedämmung EPS 035 DEO unter Estrich, d = 40 mm".to_string()
            ),
            (
                format!("{oz}.2"),
                "Wärmedämmung EPS 035 DEO unter Estrich, d = 45 mm".to_string()
            ),
        ]
    );
    for (ohne, erwartet) in [(1, &projekt[0]), (0, &projekt[1])] {
        let u = Umfang {
            gebaeude: None,
            ohne: vec![geschosse[ohne]],
        };
        // über die Szene (Ansicht) und über `lesen::lv`
        assert_eq!(teile(&s.lv(None, &u, &w), g), vec![erwartet.clone()]);
        let sched = s.schedule_in(&Umfang::projekt());
        let k = s.katalog(None);
        let lv = sk_cost::lesen::lv(s.model(), &sched, &k, &u, &w);
        assert_eq!(teile(&lv, g), vec![erwartet.clone()]);
    }
}

/// Review #213 H-213e: Mit Geschossen als Untertitel heißt die
/// Teilposition `TT.UU.PPPP.i`; Preisstand meldet jede Teilposition, nicht
/// nur die erste der Bauleistung.
#[test]
fn untertitel_je_dicke() {
    let (mut s, _) = haus();
    let g = daemmung(&mut s);
    let w = wahl(&mut s, g, true);
    let lv = s.lv(None, &Umfang::projekt(), &w);
    let t = teile(&lv, g);
    assert_eq!(t.len(), 2, "{t:?}");
    for ((oz, kurz), (i, d)) in t.iter().zip([(1, 40), (2, 45)]) {
        let stellen: Vec<usize> = oz.split('.').map(str::len).collect();
        assert_eq!(stellen, vec![2, 2, 4, 1], "{oz}");
        assert!(oz.ends_with(&format!(".{i}")), "{oz}");
        assert!(kurz.ends_with(&format!("d = {d} mm")), "{kurz}");
        assert!(
            lv.befunde
                .iter()
                .any(|f| f.satz.starts_with("Preisstand") && f.ort == Ort::Position(oz.clone())),
            "Preisstand an {oz}: {:?}",
            lv.befunde
        );
    }
    let p = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.quelle == g)
        .unwrap();
    assert_eq!(p.dicke, Some((1, Dez::ganz(40))));
}

/// Review #213 H-213g: Die schmale OZ-Spalte fasst die längste OZ einer
/// Teilposition mit Untertitel `TT.UU.PPPP.i` (Schrift der Ansicht). Ohne
/// die Schrift mit den Breiten von Arial (Ziffer 0,556 em, Punkt
/// 0,278 em), damit der Test nie still durchgeht (H-213m).
#[test]
fn oz_spalte_fasst_index() {
    let oz = "00.00.0000.0";
    let lib = std::path::Path::new("/usr/share/fonts/truetype/liberation");
    let breit = match std::fs::read(lib.join("LiberationSans-Regular.ttf"))
        .ok()
        .and_then(sk_paint::font::Font::parse)
    {
        Some(f) => f.width(oz, 11.0),
        None => {
            oz.chars()
                .map(|c| if c == '.' { 0.278 } else { 0.556 })
                .sum::<f32>()
                * 11.0
        }
    };
    assert!(breit > 60.0 && breit <= super::OZ_ENG - 4.0, "{breit}");
}

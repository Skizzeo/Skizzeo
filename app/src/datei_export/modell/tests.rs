use super::*;
use sk_model::Category;

const HAUS: &str = include_str!("../../abnahme_haus_v3.szo");
const FARBEN: [[u8; 3]; 2] = [[90, 140, 60], [120, 90, 60]];

fn haus() -> Model {
    sk_model::szo::read(HAUS, sk_model::GuidGen::with_seed(1))
        .unwrap()
        .model
}

/// Alle Namen der Szene als Pfade.
fn pfade(s: &Szene) -> Vec<String> {
    let mut v = Vec::new();
    s.je_knoten(|p, _| v.push(p.join("/")));
    v
}

#[test]
fn gliederung_wie_der_baum() {
    let m = haus();
    let (s, _) = szene(&m, Zweck::Bild, &FARBEN, "Haus");
    let p = pfade(&s);
    assert!(!p.is_empty());
    // Gebäude › Geschoss › Art › Bauteil, ohne Zahlen der Gruppen
    let aw: Vec<&String> = p.iter().filter(|x| x.contains("/AW-")).collect();
    assert!(!aw.is_empty(), "{p:?}");
    for x in &aw {
        assert!(x.starts_with("Gebäude"), "{x}");
        assert_eq!(x.matches('/').count(), 3, "{x}");
        assert!(!x.contains(" ("), "{x}");
    }
    // Jeder Bauteil mit Körper steht genau einmal da
    let koerper = m.element_solids().len();
    let mut blaetter = 0;
    s.je_knoten(|p, k| {
        if k.kinder.is_empty() && p != ["Gelände"] {
            blaetter += 1;
        }
    });
    assert_eq!(blaetter, koerper);
    assert!(
        s.materialien.iter().any(|x| x.name == "Stahlbeton"),
        "{:?}",
        s.materialien
    );
}

#[test]
fn ausgeblendetes_bleibt_draussen() {
    let mut m = haus();
    let (alle, _) = szene(&m, Zweck::Bild, &FARBEN, "Haus");
    let mut v = m.visibility().clone();
    v.hidden_cat.insert(Category::ExteriorWall);
    v.terrain_hidden = true;
    m.set_visibility(v);
    let (ohne, _) = szene(&m, Zweck::Bild, &FARBEN, "Haus");
    assert!(ohne.dreiecke() < alle.dreiecke());
    assert!(pfade(&ohne).iter().all(|x| !x.contains("/AW-")));
    assert!(pfade(&ohne).iter().all(|x| x != "Gelände"));
    // Mengen und Modell bleiben unberührt: der Export liest nur
    let text = sk_model::szo::write(&m);
    let _ = szene(&m, Zweck::Druck, &FARBEN, "Haus");
    assert_eq!(sk_model::szo::write(&m), text);
}

#[test]
fn druck_geschlossen_je_bauteil() {
    let m = haus();
    let (s, offen) = szene(&m, Zweck::Druck, &FARBEN, "Haus");
    assert_eq!(offen, 0);
    let (b, r) = sk_export::stl::binaer(&s, 100.0);
    assert_eq!(b.len(), 84 + 50 * r.dreiecke);
    assert!(r.groesse.iter().all(|c| *c > 10.0), "{:?}", r.groesse);
}

#[test]
fn glb_und_obj_aus_dem_haus() {
    let m = haus();
    let (s, _) = szene(&m, Zweck::Bild, &FARBEN, "Haus");
    let b = sk_export::glb::schreiben(&s, &Lage::default());
    assert_eq!(&b[0..4], b"glTF");
    assert_eq!(
        u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize,
        b.len()
    );
    let r = sk_export::obj::schreiben(&s, &Lage::default(), "haus");
    assert!(r.obj.contains("\nusemtl "));
    assert_eq!(r.mtl.matches("newmtl ").count(), s.materialien.len());
    assert_eq!(
        r.bilder.len(),
        s.materialien.iter().filter(|m| m.textur.is_some()).count()
    );
}

#[test]
fn saetze_der_statuszeile() {
    assert_eq!(ohne_zahl("Außenwände (4)"), "Außenwände");
    assert_eq!(ohne_zahl("Projekt „Haus (alt)“"), "Projekt „Haus (alt)“");
    assert_eq!(ohne_zahl("AW-001"), "AW-001");
    assert_eq!(tausender(52_000), "52.000");
    assert_eq!(tausender(999), "999");
    assert_eq!(tausender(1_234_567), "1.234.567");
    let b = sk_export::stl::DruckBericht {
        dreiecke: 10,
        groesse: [234.0, 180.0, 91.0],
        duenn: vec![
            ("Gebäude 1 / EG / Fallrohr".into(), 0.2),
            ("Gebäude 1 / DG / Attikablech".into(), 0.3),
        ],
    };
    assert_eq!(
        druck_satz(100.0, &b, 1),
        "STL 1:100 gespeichert: 23,4 × 18,0 × 9,1 cm im Druck · 2 Teile dünner als 0,8 mm: Fallrohr, Attikablech · 1 Körper nicht geschlossen"
    );
}

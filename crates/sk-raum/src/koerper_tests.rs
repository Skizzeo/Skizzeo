//! Körper des Ausbaus (RB4) gegen Handrechnung: Rauminhalt je Körper,
//! geschlossene Oberfläche und Kanten nur, wo sie knickt. Dieselben Räume
//! wie die Mengen; OK Fertigfußboden 0,160, UK Deckenputz 2,625.

use super::*;
use crate::raumbuch::koerper::{estrich, koerper, Koerper};
use sk_math::Vec3;
use sk_model::Solid;

/// Rauminhalt (mm³) über die Dreiecke, gemessen vom Punkt `o` aus.
fn inhalt_von(s: &Solid, o: Vec3) -> f64 {
    s.triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.p.map(|p| p - o);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// Rauminhalt (mm³); prüft dabei, dass er nicht vom Bezugspunkt abhängt
/// (die Oberfläche ist geschlossen und nach außen gerichtet).
fn inhalt(s: &Solid) -> f64 {
    let v = inhalt_von(s, vec3(0.0, 0.0, 0.0));
    let w = inhalt_von(s, vec3(1234.0, -5678.0, 910.0));
    assert!((v - w).abs() <= 1e-9 * v.abs().max(1.0), "{v} ≠ {w}");
    assert!(v > 0.0, "Rauminhalt {v}");
    v
}

/// Rauminhalte gleich bis auf die Rundung der Gleitkommarechnung.
fn gleich_v(ist: f64, soll: f64) {
    assert!(
        (ist - soll).abs() <= 1e-9 * soll.abs(),
        "ist {ist}, soll {soll}"
    );
}

/// Der Körper zu Fläche, Typ und Schicht; prüft, dass es genau einen gibt.
fn der<'a>(m: &Model, k: &'a [Koerper], f: Flaeche, typ: (Guid, usize)) -> &'a Koerper {
    let t: Vec<&Koerper> = k
        .iter()
        .filter(|x| x.flaeche == f && (m.layer_set(x.typ).unwrap().guid, x.schicht) == typ)
        .collect();
    assert_eq!(t.len(), 1, "{f:?} {typ:?}");
    t[0]
}

/// Jede Kante eines Klotzes kommt in beiden Richtungen vor (für Körper
/// ohne Löcher und ohne Punkte in einer Flucht).
fn dicht(s: &Solid) {
    let r = |p: Vec3| [p.x, p.y, p.z].map(|x| (x * 1e3).round() as i64);
    let mut kanten: Vec<([i64; 3], [i64; 3])> = s
        .triangles
        .iter()
        .flat_map(|t| (0..3).map(move |i| (r(t.p[i]), r(t.p[(i + 1) % 3]))))
        .collect();
    kanten.sort();
    for &(a, b) in &kanten {
        assert!(
            kanten.binary_search(&(b, a)).is_ok(),
            "offen bei {a:?}–{b:?}"
        );
    }
}

/// Gibt es eine Kante von `a` nach `b` (in einer der beiden Richtungen)?
fn kante(s: &Solid, a: Vec3, b: Vec3) -> bool {
    let nah = |p: Vec3, q: Vec3| (p - q).length() < 1e-6;
    s.edges
        .iter()
        .any(|e| (nah(e.a, a) && nah(e.b, b)) || (nah(e.a, b) && nah(e.b, a)))
}

/// Liegt eine Kante (oder ein Stück davon) auf der Strecke von `a` nach
/// `b`?
fn kante_auf(s: &Solid, a: Vec3, b: Vec3) -> bool {
    let l = (b - a).length();
    let u = (b - a) * (1.0 / l);
    let auf = |p: Vec3| {
        let t = (p - a).dot(u);
        (a + u * t - p).length() < 1e-6 && t > -1e-6 && t < l + 1e-6
    };
    s.edges.iter().any(|e| auf(e.a) && auf(e.b))
}

#[test]
fn wohnzimmer_koerper() {
    let h = haus();
    let f = wohnzimmer(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let v = |fl, typ| inhalt(&der(&h.m, &k, fl, typ).solid);
    assert_eq!(k.len(), 4 + 1 + 1 + 4 + 1, "{k:#?}");
    for x in &k {
        dicht(&x.solid);
    }

    // Estrich und Parkett je Schicht: Fläche wie die Menge × Dicke
    let boden = 19_730_900.0 + 13_275.0 + 30_150.0 + 331_650.0;
    for (j, d) in [65.0, 30.0, 40.0, 5.0].into_iter().enumerate() {
        gleich_v(v(Flaeche::Estrich, (SCREED_SLAB_TYPE_GUID, j)), boden * d);
    }
    gleich_v(v(Flaeche::Boden, (PARQUET_TYPE_GUID, 0)), boden * 20.0);
    // Deckenputz im Rohbaumaß; der Anstrich hat keinen Körper
    gleich_v(
        v(Flaeche::Decke, (CEILING_TYPE_GUID, 1)),
        20_000_000.0 * 10.0,
    );

    // Wandputz bis UK Deckenputz, auf Gehrung in den Ecken (Trapez im
    // Grundriss); ohne die Öffnung im Rohbaumaß, mit Laibungen bis zum
    // Element: seitlich 15 × 180, oben 15 hoch zwischen den seitlichen
    let rf = |k| v(Flaeche::Wand(k), (WOODCHIP_WALL_TYPE_GUID, 2));
    let h0 = 2625.0;
    gleich_v(
        rf(0),
        (5000.0 + 4970.0) / 2.0 * 15.0 * h0 - 1510.0 * 15.0 * 1510.0
            + 2.0 * 15.0 * 180.0 * 1510.0
            + 1480.0 * 180.0 * 15.0,
    );
    // Innentür: Umfassungszarge, keine Laibung
    gleich_v(
        rf(1),
        (4000.0 + 3970.0) / 2.0 * 15.0 * h0 - 885.0 * 15.0 * 2135.0,
    );
    // Fenstertür: seitliche Laibungen ab OK Fertigfußboden, darunter
    // Estrich und Parkett in der Nische
    gleich_v(
        rf(2),
        (5000.0 + 4970.0) / 2.0 * 15.0 * h0 - 2010.0 * 15.0 * 2385.0
            + 2.0 * 15.0 * 180.0 * 2225.0
            + 1980.0 * 180.0 * 15.0,
    );
    gleich_v(rf(3), (4000.0 + 3970.0) / 2.0 * 15.0 * h0);

    // Sockelleiste 60 × 15 auf Gehrung, Mittellinie (l − 45) je Seite; an
    // der Fenstertür bis an die geputzte Laibung, an der Innentür bis zur
    // Zarge
    gleich_v(
        v(Flaeche::Sockel, (OAK_SKIRTING_TYPE_GUID, 0)),
        (2.0 * 4955.0 + 2.0 * 3955.0 - 885.0 - 1980.0) * 15.0 * 60.0,
    );

    // Kanten: an der Fensterlaibung die Putzecke, keine Fuge zwischen
    // Wandfeld und Laibung; am Boden keine Fuge zum Streifen vor der
    // Fenstertür, wohl aber die Kante an der Nische
    let w0 = &der(&h.m, &k, Flaeche::Wand(0), (WOODCHIP_WALL_TYPE_GUID, 2)).solid;
    assert!(kante(
        w0,
        vec3(1015.0, 15.0, 875.0),
        vec3(1015.0, 15.0, 2370.0)
    ));
    assert!(!kante_auf(
        w0,
        vec3(1000.0, 15.0, 875.0),
        vec3(1000.0, 15.0, 2385.0)
    ));
    assert!(!kante_auf(
        w0,
        vec3(1000.0, 15.0, 0.0),
        vec3(1000.0, 15.0, h0)
    ));
    let pa = &der(&h.m, &k, Flaeche::Boden, (PARQUET_TYPE_GUID, 0)).solid;
    assert!(!kante_auf(
        pa,
        vec3(4000.0, 3985.0, 160.0),
        vec3(1990.0, 3985.0, 160.0)
    ));
    assert!(kante(
        pa,
        vec3(4000.0, 4165.0, 160.0),
        vec3(1990.0, 4165.0, 160.0)
    ));
    assert!(kante(
        pa,
        vec3(4985.0, 3985.0, 160.0),
        vec3(4000.0, 3985.0, 160.0)
    ));
}

#[test]
fn bad_koerper() {
    let h = haus();
    let f = bad(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(BATH_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let v = |fl, typ| inhalt(&der(&h.m, &k, fl, typ).solid);
    for x in &k {
        dicht(&x.solid);
    }
    // Estrich auf Decke mit ausgleichender Dämmung, Belag mit Abdichtung
    let boden = 4_668_900.0 + 13_275.0 + 101_775.0;
    for (j, d) in [65.0, 30.0, 50.0].into_iter().enumerate() {
        gleich_v(v(Flaeche::Estrich, (SCREED_FLOOR_TYPE_GUID, j)), boden * d);
    }
    gleich_v(v(Flaeche::Boden, (WET_FLOOR_TYPE_GUID, 0)), boden * 13.0);
    gleich_v(v(Flaeche::Boden, (WET_FLOOR_TYPE_GUID, 1)), boden * 2.0);
    // Wandfliesen 10 auf dem Putz, ab OK Fertigfußboden 1,20 hoch, auf
    // Gehrung; am Fenster bis an die geputzte Laibung, an der Tür ab Boden
    let wf = |k| v(Flaeche::Fliesen(k), (WALL_TILES_TYPE_GUID, 0));
    gleich_v(
        wf(0),
        (2370.0 + 2350.0) / 2.0 * 10.0 * 1200.0 - 870.0 * 10.0 * 485.0,
    );
    gleich_v(wf(1), (1970.0 + 1950.0) / 2.0 * 10.0 * 1200.0);
    gleich_v(
        wf(2),
        (2370.0 + 2350.0) / 2.0 * 10.0 * 1200.0 - 885.0 * 10.0 * 1200.0,
    );
    gleich_v(wf(3), (1970.0 + 1950.0) / 2.0 * 10.0 * 1200.0);
    assert!(!k.iter().any(|x| x.flaeche == Flaeche::Sockel));
}

#[test]
fn diele_koerper() {
    let h = haus();
    let f = diele(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(HALL_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let v = |fl, typ| inhalt(&der(&h.m, &k, fl, typ).solid);
    // Boden: die Menge ohne den übermessenen Pfeiler (0,33 × 0,33 bis
    // Putz); der Pfeiler hängt über einen Schlitz von 0,01 mm am Umriss,
    // höchstens 1 m lang
    let boden = 9_981_487.5 - 330.0 * 330.0;
    let schlitz = 0.01 * 1000.0;
    for (j, d) in [65.0, 30.0, 45.0, 5.0].into_iter().enumerate() {
        let ist = v(Flaeche::Estrich, (SCREED_SLAB_TYPE_GUID, j));
        assert!(
            ist <= boden * d && ist >= (boden - schlitz) * d,
            "{j}: {ist}"
        );
    }
    let ist = v(Flaeche::Boden, (PORCELAIN_TYPE_GUID, 0));
    assert!(
        ist <= boden * 15.0 && ist >= (boden - schlitz) * 15.0,
        "{ist}"
    );
    let decke = 9_750_000.0 - 300.0 * 300.0;
    let ist = v(Flaeche::Decke, (CEILING_TYPE_GUID, 1));
    assert!(
        ist <= decke * 10.0 && ist >= (decke - schlitz) * 10.0,
        "{ist}"
    );
    // Haustür: Laibungen bis zum Element
    let rf = |k| v(Flaeche::Wand(k), (WOODCHIP_WALL_TYPE_GUID, 2));
    gleich_v(
        rf(0),
        (4000.0 + 3970.0) / 2.0 * 15.0 * 2625.0 - 1250.0 * 15.0 * 2385.0
            + 2.0 * 15.0 * 180.0 * 2225.0
            + 1220.0 * 180.0 * 15.0,
    );
    // Pfeiler: Putz auf Gehrung um die Außenecken, 0,30 → 0,33
    for w in 6..10 {
        gleich_v(rf(w), (300.0 + 330.0) / 2.0 * 15.0 * 2625.0);
    }
    // Fliesensockel 60 × 10: Ringe bis Putz und bis Sockel, ohne die
    // Haustür im Fertigmaß und die drei Innentüren
    let l = 3970.0 * 1470.0 + 1470.0 * 2500.0 - (3950.0 * 1450.0 + 1450.0 * 2500.0);
    let p = 350.0 * 350.0 - 330.0 * 330.0;
    gleich_v(
        v(Flaeche::Sockel, (TILE_SKIRTING_TYPE_GUID, 0)),
        (l + p - (1220.0 + 3.0 * 885.0) * 10.0) * 60.0,
    );
}

#[test]
fn ecktuer_koerper() {
    let h = haus();
    let f = ecke(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let v = |fl, typ| inhalt(&der(&h.m, &k, fl, typ).solid);
    for x in &k {
        dicht(&x.solid);
    }
    let boden =
        3970.0 * 2970.0 + 2.0 * (1000.0 * 15.0 + 1000.0 * 165.0) + 165.0 * 165.0 - 15.0 * 15.0;
    gleich_v(v(Flaeche::Boden, (PARQUET_TYPE_GUID, 0)), boden * 20.0);
    // Je eine seitliche Laibung; die obere der ersten Wand läuft über das
    // Eckfeld bis 4,165, die der zweiten beginnt hinter ihr
    let rf = |k| v(Flaeche::Wand(k), (WOODCHIP_WALL_TYPE_GUID, 2));
    let sturz = (1000.0 + 985.0) / 2.0 * 15.0 * 2385.0;
    gleich_v(
        rf(0),
        (4000.0 + 3970.0) / 2.0 * 15.0 * 2625.0 - sturz
            + 15.0 * 180.0 * 2225.0
            + 1150.0 * 180.0 * 15.0,
    );
    gleich_v(
        rf(1),
        (3000.0 + 2970.0) / 2.0 * 15.0 * 2625.0 - sturz
            + 15.0 * 180.0 * 2225.0
            + 970.0 * 180.0 * 15.0,
    );
    // Sockel bis an die Laibungen, an der offenen Ecke keiner
    gleich_v(
        v(Flaeche::Sockel, (OAK_SKIRTING_TYPE_GUID, 0)),
        (15.0 * 3015.0 - 337.5 + 15.0 * 2015.0 - 337.5 + 3955.0 * 15.0 + 2955.0 * 15.0) * 60.0,
    );
}

/// Nur der Estrich: dieselben Körper wie unter allen, in derselben Folge.
#[test]
fn nur_der_estrich() {
    let h = haus();
    let raeume = [
        (wohnzimmer(&h), LIVING_SPACE_GUID),
        (bad(&h), BATH_SPACE_GUID),
        (diele(&h), HALL_SPACE_GUID),
        (ecke(&h), LIVING_SPACE_GUID),
    ];
    for (f, g) in raeume {
        let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(g));
        let text = |k: &Koerper| format!("{k:?}");
        let alle = koerper(&h.m, &f, &a);
        let alle: Vec<String> = alle
            .iter()
            .filter(|k| k.flaeche == Flaeche::Estrich)
            .map(text)
            .collect();
        let nur: Vec<String> = estrich(&h.m, &f, &a).iter().map(text).collect();
        assert!(!nur.is_empty());
        assert_eq!(nur, alle);
    }
}

#[test]
fn leerer_raum_ohne_koerper() {
    let h = haus();
    let leer = form(h.eg, Vec::new(), Vec::new(), Vec::new());
    let a = RaumAusbau::nach_vorlage(&h.m, &leer, &vorlage(LIVING_SPACE_GUID));
    assert!(koerper(&h.m, &leer, &a)
        .iter()
        .all(|k| k.solid.triangles.is_empty()));
}

/// Fläche (mm²) der Schnittflächen eines geschnittenen Körpers.
fn schnittflaeche(s: &Solid) -> f64 {
    s.triangles
        .iter()
        .filter(|t| t.mat & sk_model::material::CUT != 0)
        .map(|t| 0.5 * (t.p[1] - t.p[0]).cross(t.p[2] - t.p[0]).length())
        .sum()
}

/// Liegt eine Schnittkontur ganz auf der Strecke von `a` nach `b`?
fn schnittkante_auf(s: &Solid, a: Vec3, b: Vec3) -> bool {
    let l = (b - a).length();
    let u = (b - a) * (1.0 / l);
    let auf = |p: Vec3| {
        let t = (p - a).dot(u);
        t > -1e-6 && t < l + 1e-6 && (a + u * t - p).length() < 1e-6
    };
    s.edges
        .iter()
        .any(|e| e.kind == sk_model::edge_kind::CUT && auf(e.a) && auf(e.b))
}

/// Grundriss in 1,00 m Höhe: Wandputz mit Schnittfläche nach oben, ohne
/// Fuge zwischen Putzfeld und Laibung; der Rest ist geschlossen.
#[test]
fn grundriss_schneidet_den_putz() {
    let h = haus();
    let f = wohnzimmer(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let oben = vec3(0.0, 0.0, 1.0);
    let wand = der(&h.m, &k, Flaeche::Wand(0), (WOODCHIP_WALL_TYPE_GUID, 2));
    let s = wand.geschnitten(vec3(0.0, 0.0, 1000.0), oben);
    // Trapez ohne das Fenster (Brüstung 0,875), dazu die seitlichen Laibungen
    let trapez = (5000.0 + 4970.0) / 2.0 * 15.0;
    let flaeche = trapez - 1510.0 * 15.0 + 2.0 * 15.0 * 180.0;
    assert!(
        (schnittflaeche(&s) - flaeche).abs() < 1e-6,
        "{}",
        schnittflaeche(&s)
    );
    assert!(s
        .triangles
        .iter()
        .all(|t| t.p.iter().all(|p| p.z <= 1000.0 + 1e-6)));
    assert!(s
        .triangles
        .iter()
        .filter(|t| t.mat & sk_model::material::CUT != 0)
        .all(|t| t.n == oben && t.mat & !sk_model::material::CUT == wand.schnitt));
    let unter_dem_fenster = 1510.0 * 15.0 * 125.0 - 2.0 * 15.0 * 180.0 * 125.0;
    gleich_v(inhalt(&s), trapez * 1000.0 - unter_dem_fenster);
    // Fuge zwischen Putzfeld und Laibung links neben dem Fenster: keine
    // Kontur; die Laibung zum Fenster hin hat eine
    let z = 1000.0;
    assert!(!schnittkante_auf(
        &s,
        vec3(1000.0, 0.0, z),
        vec3(1000.0, 15.0, z)
    ));
    assert!(schnittkante_auf(
        &s,
        vec3(1000.0, -165.0, z),
        vec3(1000.0, 0.0, z)
    ));
    // Estrich und Belag liegen unter der Schnitthöhe: nur der Rest
    let estrich = der(&h.m, &k, Flaeche::Estrich, (SCREED_SLAB_TYPE_GUID, 0));
    let e = estrich.geschnitten(vec3(0.0, 0.0, 1000.0), oben);
    assert_eq!(schnittflaeche(&e), 0.0);
    assert_eq!(e.triangles.len(), estrich.solid.triangles.len());
}

/// Schnitt quer bei x = 2,50 m: Belag von der Putzoberfläche unten bis in
/// die Nische der Fenstertür oben als eine Fläche, Decke im Rohbaumaß.
#[test]
fn schnitt_durch_den_raum() {
    let h = haus();
    let f = wohnzimmer(&h);
    let a = RaumAusbau::nach_vorlage(&h.m, &f, &vorlage(LIVING_SPACE_GUID));
    let k = koerper(&h.m, &f, &a);
    let (p0, n) = (vec3(2500.0, 2000.0, 0.0), vec3(1.0, 0.0, 0.0));
    let belag = der(&h.m, &k, Flaeche::Boden, (PARQUET_TYPE_GUID, 0));
    let s = belag.geschnitten(p0, n);
    assert!((schnittflaeche(&s) - (4165.0 - 15.0) * 20.0).abs() < 1e-6);
    assert!(s
        .triangles
        .iter()
        .all(|t| t.p.iter().all(|p| p.x <= 2500.0 + 1e-6)));
    // Streifen und Nische hängen ohne Fuge am Belag
    let z = 160.0;
    assert!(!schnittkante_auf(
        &s,
        vec3(2500.0, 3985.0, z - 20.0),
        vec3(2500.0, 3985.0, z)
    ));
    assert!(!schnittkante_auf(
        &s,
        vec3(2500.0, 4000.0, z - 20.0),
        vec3(2500.0, 4000.0, z)
    ));
    assert!(schnittkante_auf(
        &s,
        vec3(2500.0, 4165.0, z - 20.0),
        vec3(2500.0, 4165.0, z)
    ));
    // links der Ebene: Belag ab der Putzoberfläche, Streifen und Nische der
    // Fenstertür ab x = 1,99
    gleich_v(
        inhalt(&s),
        (2485.0 * 3970.0 + 510.0 * 15.0 + 510.0 * 165.0) * 20.0,
    );
    let decke = der(&h.m, &k, Flaeche::Decke, (CEILING_TYPE_GUID, 1));
    let d = decke.geschnitten(p0, n);
    assert!((schnittflaeche(&d) - 4000.0 * 10.0).abs() < 1e-6);
    assert!(d
        .triangles
        .iter()
        .filter(|t| t.mat & sk_model::material::CUT != 0)
        .all(|t| t.n == n));
    gleich_v(inhalt(&d), 2500.0 * 4000.0 * 10.0);
}

/// Geometrie zu #137: Jeder Körper der vier Räume, geschnitten in sechs
/// Höhen und an senkrechten Ebenen (auch genau auf Flächen und Fugen, bei
/// 500 und 2000): Teil dahinter und Teil davor sind je geschlossen und
/// ergeben zusammen den ganzen Körper.
#[test]
fn schnitt_teilt_den_inhalt() {
    let h = haus();
    let raeume = [
        (wohnzimmer(&h), LIVING_SPACE_GUID),
        (bad(&h), BATH_SPACE_GUID),
        (diele(&h), HALL_SPACE_GUID),
        (ecke(&h), LIVING_SPACE_GUID),
    ];
    let mut schnitte = 0;
    let mut schlecht = Vec::new();
    let o = vec3(1234.0, -5678.0, 910.0);
    for (f, g) in &raeume {
        let a = RaumAusbau::nach_vorlage(&h.m, f, &vorlage(*g));
        for k in koerper(&h.m, f, &a) {
            let ganz = inhalt_von(&k.solid, vec3(0.0, 0.0, 0.0));
            let mut ebenen = Vec::new();
            for z in [100.0, 170.0, 1000.0, 2000.0, 2500.0, 2630.0] {
                ebenen.push((vec3(0.0, 0.0, z), vec3(0.0, 0.0, 1.0)));
            }
            for x in [500.37, 1234.5, 2000.37, 3500.37, 4100.37, 500.0, 2000.0] {
                ebenen.push((vec3(x, 0.0, 0.0), vec3(1.0, 0.0, 0.0)));
                ebenen.push((vec3(0.0, x, 0.0), vec3(0.0, 1.0, 0.0)));
            }
            for (p0, n) in ebenen {
                let unten = k.geschnitten(p0, n);
                let oben = k.geschnitten(p0, n * -1.0);
                let v =
                    [&unten, &oben].map(|s| (inhalt_von(s, vec3(0.0, 0.0, 0.0)), inhalt_von(s, o)));
                schnitte += 1;
                let tol = 1e-6 * ganz.abs().max(1.0);
                if v.iter().any(|(a, b)| (a - b).abs() > tol)
                    || (v[0].0 + v[1].0 - ganz).abs() > tol
                {
                    schlecht.push(format!(
                        "{:?} {} {p0:?} {n:?}: {ganz:.0} {v:?}",
                        k.flaeche, k.schicht
                    ));
                }
            }
        }
    }
    assert_eq!(schnitte, 53 * 20);
    assert!(schlecht.is_empty(), "{schlecht:#?}");
}

/// Geometrie zu #137: Liegt die Ebene auf der Putzfläche (Bad, Wand 2 bei
/// y = 2000), entsteht keine Schnittfläche; mitten durch den Putz die volle
/// (Wohnzimmer, Wand 1: 15 × 2625).
#[test]
fn ebene_auf_der_putzflaeche() {
    let h = haus();
    let flaeche = |f: &Rohbau, g, w, schicht, y| {
        let a = RaumAusbau::nach_vorlage(&h.m, f, &vorlage(g));
        let k = koerper(&h.m, f, &a)
            .into_iter()
            .find(|k| k.flaeche == Flaeche::Wand(w) && k.schicht == schicht)
            .unwrap();
        schnittflaeche(&k.geschnitten(vec3(0.0, y, 0.0), vec3(0.0, 1.0, 0.0)))
    };
    assert!(flaeche(&bad(&h), BATH_SPACE_GUID, 2, 1, 2000.0).abs() < 1e-6);
    gleich_v(
        flaeche(&wohnzimmer(&h), LIVING_SPACE_GUID, 1, 2, 2000.0),
        15.0 * 2625.0,
    );
}

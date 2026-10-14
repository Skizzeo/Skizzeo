//! Erdmengen der Geländeflächen (G3b, vob/pruefung-gelaende-g3.md B1 bis
//! B7): je Fläche das Planungsgelände über dem Netz des Aufmaßes
//! ([`sk_gelaende::planung`]), Abtrag und Auftrag zwischen N und Q,
//! Oberboden auf den Flächen außerhalb des Baufelds, Andecken auf allen
//! Grünflächen mit Oberbodenabtrag. In der Baugrube zählt die Aushubfläche:
//! Abtrag dort mindert die Verfüllung der Gründung.
//!
//! Die Massenbilanz verteilt den Eigenboden in fester Reihenfolge (B4):
//! Verfüllung aus der Baugrube (schon in G2), Geländeauftrag aus dem
//! Geländeabtrag, Rest aus dem Überschuss der Baugrube, Rest Lieferung;
//! was bleibt, fährt ab (Baugrube 311, Gelände 511). Der Faktor fest →
//! eingebaut wirkt nur in der Bilanz. Oberboden hat eine eigene Bilanz.
//! Ohne Geländeflächen bleibt alles wie in G2, Zeile für Zeile.

use super::{eckzahl, zahl, Boden, ErdZone, DREIECKSPRISMEN, SCHLUESSEL};
use super::{BACKFILL, CUT, DISPOSAL, EMBANK, EMBANK_SUPPLY, SURPLUS, TOPSOIL, TOPSOIL_SITE};
use super::{KG, KG_ANDECKEN, KG_GELAENDE, SPREAD, SPREAD_SLOPE, TOPSOIL_REMOVE, TOPSOIL_SUPPLY};
use crate::qto::AutoMenge;
use crate::Model;
use sk_gelaende::planung::{Aufbau, Grube, Hinweis, Planung, Zonen};
use sk_math::Vec3;

/// Eine Gründung, wie die Geländeflächen sie sehen: die Vorlage ihrer
/// Mengen (Träger Sohlplatte), Plattenumriss und Zone aus G2.
#[derive(Clone, Debug)]
pub struct Gruendung {
    pub vorlage: AutoMenge,
    pub outline: Vec<Vec3>,
    pub zone: ErdZone,
}

/// Hinweis an befestigten Flächen bis G4 (B2).
pub const BEFESTIGT: &str = "Aufbau der befestigten Fläche ab G4, Auftrag bis OK Fläche";
/// Hinweis, wenn die Bilanz begrenzt (B4).
pub const FAKTOR: &str = "Eigenboden fest → eingebaut Faktor {f} angenommen; ändert die \
                          Aufteilung Eigenboden/Lieferung";

fn hinweis_text(h: &Hinweis) -> &'static str {
    match h {
        Hinweis::ZuSteil(_) => "Kante steiler als ihre Böschung: dort keine Böschung",
        Hinweis::Einbuchtung(_) => {
            "enge Einbuchtung: Böschungen treffen sich vor dem Gelände, Bereich endet dort"
        }
        Hinweis::OffenesEnde(_) => {
            "Böschung erreicht das Gelände im Aufmaß nicht: Menge bis zum Rand des Aufmaßes"
        }
        Hinweis::SpitzeEcke(_) => "spitze Ecke, Böschung läuft lang aus",
        Hinweis::Ungueltig(_) => "Kante oder Neigung ungültig: dort keine Böschung",
    }
}

/// m³ bzw. m² als Text.
fn m3(v: f64) -> String {
    format!("{} m³", zahl(v, 1.0))
}

fn m2(v: f64) -> String {
    format!("{} m²", zahl(v, 1.0))
}

/// Summen über alle Flächen (m, m², m³).
#[derive(Default)]
struct Summe {
    abtrag: f64,
    auftrag: f64,
    oberboden_fl: f64,
    im_baufeld: f64,
    andecken: f64,
    andecken_steil: f64,
    unter_sohle: f64,
    befestigt: bool,
    hinweise: Vec<&'static str>,
}

/// Mengen der Geländeflächen des Modells `m` an die Mengen `out` der
/// Gründungen `gr` anfügen und deren Verfüllung und Abfuhr nach der Bilanz
/// ändern. Ohne Flächen, Aufmaß oder Gründung nichts.
pub(crate) fn gelaende_mengen(
    m: &Model,
    gr: &[Gruendung],
    boden: &Boden,
    out: &mut Vec<AutoMenge>,
) {
    if m.site_patches().is_empty() {
        return;
    }
    // Träger: die Platte mit der kleinsten Nummer
    let Some(traeger) = gr
        .iter()
        .min_by_key(|g| (g.vorlage.number.len(), g.vorlage.number.clone()))
    else {
        return;
    };
    let (Some(t), Some(frame), Some(h0)) = (m.terrain(), m.survey_frame(), m.terrain_nhn()) else {
        return;
    };
    let mesh = t.mesh();
    let im_netz = |p: &Vec3| {
        let (e, n) = frame.to_survey(p.x, p.y);
        [e - mesh.bezug.0, n - mesh.bezug.1]
    };
    let ring = |pts: &[Vec3]| pts.iter().map(im_netz).collect::<Vec<_>>();
    let o = boden.oberboden.max(0.0);
    let site = m.site_soil();
    // Andecken nur, wo Oberboden abgetragen wurde (B6)
    let d = if o > 0.0 { site.spread.max(0.0) } else { 0.0 };
    let baufelder: Vec<Option<Vec<Vec3>>> = gr
        .iter()
        .map(|g| {
            let x = g.zone.baufeld.max(0.0);
            sk_math::flaeche::versatz(&g.outline, &vec![-x; g.outline.len()])
        })
        .collect();
    let gruben: Vec<usize> = (0..gr.len())
        .filter(|&i| gr[i].zone.sohle.len() >= 3)
        .collect();
    let zonen = Zonen {
        haus: gr.iter().map(|g| ring(&g.outline)).collect(),
        baufeld: baufelder.iter().flatten().map(|b| ring(b)).collect(),
        gruben: gruben
            .iter()
            .map(|&i| Grube {
                sohle: ring(&gr[i].zone.sohle),
                platte: ring(&gr[i].outline),
                b: h0 + gr[i].zone.b / 1000.0,
            })
            .collect(),
    };
    let mut s = Summe::default();
    let mut weniger = vec![0.0; gr.len()];
    for p in m.site_patches() {
        let Some(b) = p.baustein(&frame, h0, mesh.bezug) else {
            continue;
        };
        let plan = Planung::neu(&b, &mesh.abfrage);
        let a = Aufbau {
            oberboden: o / 1000.0,
            andecken: d / 1000.0,
            befestigt: p.paved,
        };
        let e = plan.erdbau(&mesh.abfrage, &a, &zonen);
        s.abtrag += e.abtrag;
        s.auftrag += e.auftrag;
        s.oberboden_fl += e.oberboden_flaeche();
        s.im_baufeld += e.im_baufeld;
        s.andecken += e.andecken;
        s.andecken_steil += e.andecken_steil;
        s.unter_sohle += e.unter_sohle;
        s.befestigt |= p.paved;
        for (k, v) in gruben.iter().zip(&e.je_grube) {
            weniger[*k] += v;
        }
        for h in &plan.hinweise {
            let t = hinweis_text(h);
            if !s.hinweise.contains(&t) {
                s.hinweise.push(t);
            }
        }
    }
    if m.site_patches().len() > 1 && ueberlappen(m) {
        s.hinweise
            .push("Geländeflächen überlappen sich: Menge dort doppelt gezählt, Flächen prüfen");
    }

    let verfahren = format!(
        "; {DREIECKSPRISMEN}, {} Dreiecke, {}",
        mesh.netz.dreiecke.len(),
        if t.read.is_empty() {
            format!("Aufmaß {}", t.src)
        } else {
            format!("Aufmaß {}, {}", t.src, t.read)
        }
    );
    let f = site.reuse;
    let f_text = zahl(f, 1.0);
    let zeile = |key: &'static str, kg: u16, value: f64, formula: String, hinweis: Vec<String>| {
        let unit = SCHLUESSEL.iter().find(|k| k.0 == key).map_or("m3", |k| k.1);
        AutoMenge {
            key,
            unit,
            value,
            kg: Some(kg),
            formula,
            hinweis: hinweis.join("\n"),
            ..traeger.vorlage.clone()
        }
    };

    // B3: weniger Verfüllung je Gründung, um so viel mehr Überschuss
    let mut ueberschuss = 0.0;
    let mut abfuhr: Vec<f64> = vec![0.0; gr.len()];
    for (i, g) in gr.iter().enumerate() {
        let el = g.vorlage.element;
        let wert = |out: &[AutoMenge], key: &str| {
            out.iter()
                .filter(|r| r.element == el && r.key == key)
                .map(|r| r.value)
                .sum::<f64>()
        };
        let verf = wert(out, BACKFILL);
        let w = (weniger[i] * 1e9).min(verf);
        abfuhr[i] = wert(out, DISPOSAL) + w;
        ueberschuss += abfuhr[i];
        if w <= 0.0 {
            continue;
        }
        let neu = verf - w;
        let zusatz = format!(
            "; − Geländeabtrag im Arbeitsraum {} (VOB-Prüfung G3 B3)",
            m3(w / 1e9)
        );
        match out
            .iter()
            .position(|r| r.element == el && r.key == BACKFILL)
        {
            Some(k) if neu > 0.0 => {
                out[k].value = neu;
                out[k].formula.push_str(&zusatz);
            }
            Some(k) => {
                out.remove(k);
            }
            None => {}
        }
        abfuhr_setzen(
            out,
            &g.vorlage,
            abfuhr[i],
            &format!("; + weniger Verfüllung {}", m3(w / 1e9)),
        );
    }

    // B4: Bilanz in m³ (fest bzw. verdichtet)
    let gel = s.abtrag;
    let auf = s.auftrag;
    let eigen1 = auf.min(gel * f);
    let rest_gel = gel - eigen1 / f;
    let eigen2 = (auf - eigen1).min(ueberschuss / 1e9 * f).max(0.0);
    let liefer = (auf - eigen1 - eigen2).max(0.0);
    // aus der Baugrube entnommen, in der Reihenfolge der Gründungen
    let mut entnehmen = eigen2 / f * 1e9;
    for (i, g) in gr.iter().enumerate() {
        if entnehmen <= 0.0 {
            break;
        }
        let x = entnehmen.min(abfuhr[i]);
        if x <= 0.0 {
            continue;
        }
        entnehmen -= x;
        abfuhr[i] -= x;
        abfuhr_setzen(
            out,
            &g.vorlage,
            abfuhr[i],
            &format!("; − Einbau im Gelände {} (VOB-Prüfung G3 B4)", m3(x / 1e9)),
        );
    }
    let begrenzt = liefer > 1e-9 || rest_gel > 1e-9 || ueberschuss / 1e9 - eigen2 / f > 1e-9;
    let mut h_erdbau: Vec<String> = s.hinweise.iter().map(|h| h.to_string()).collect();
    if s.befestigt {
        h_erdbau.push(BEFESTIGT.to_string());
    }
    let mut neu = Vec::new();

    // B1: Oberboden auf den Flächen außerhalb des Baufelds
    let ob_site = s.oberboden_fl * o / 1000.0;
    if ob_site > 0.0 {
        neu.push(zeile(
            TOPSOIL_SITE,
            KG_GELAENDE,
            ob_site * 1e9,
            format!(
                "Geländeflächen samt Böschung außerhalb des Baufelds {} × {} m",
                m2(s.oberboden_fl),
                zahl(o, 1e3)
            ),
            Vec::new(),
        ));
    }
    if gel > 0.0 {
        let mut h = h_erdbau.clone();
        if s.unter_sohle > 0.0 {
            h.push(format!(
                "Planum unter UK Platte, die Gründung läge frei: {} Abtrag unter der Sohle",
                m3(s.unter_sohle)
            ));
        }
        neu.push(zeile(
            CUT,
            KG_GELAENDE,
            gel * 1e9,
            format!("Geländeflächen zwischen gewachsenem Boden und Planum{verfahren}"),
            h,
        ));
    }
    let faktor = || FAKTOR.replace("{f}", &f_text);
    if eigen1 + eigen2 > 0.0 {
        let mut h = h_erdbau.clone();
        if begrenzt {
            h.push(faktor());
        }
        neu.push(zeile(
            EMBANK,
            KG_GELAENDE,
            (eigen1 + eigen2) * 1e9,
            format!(
                "Auftrag {} bis Planum: aus Geländeabtrag {}, aus der Baugrube {}{verfahren}",
                m3(auf),
                m3(eigen1),
                m3(eigen2)
            ),
            h,
        ));
    }
    if liefer > 0.0 {
        let mut h = h_erdbau.clone();
        h.push(faktor());
        neu.push(zeile(
            EMBANK_SUPPLY,
            KG_GELAENDE,
            liefer * 1e9,
            format!(
                "Auftrag {} − Eigenboden {} (Faktor {f_text})",
                m3(auf),
                m3(eigen1 + eigen2)
            ),
            h,
        ));
    }
    if rest_gel > 0.0 {
        neu.push(zeile(
            SURPLUS,
            KG_GELAENDE,
            rest_gel * 1e9,
            format!(
                "Geländeabtrag {} − Einbau {} (Faktor {f_text})",
                m3(gel),
                m3(eigen1 / f)
            ),
            vec![faktor()],
        ));
    }

    // B5, B6: Andecken auf Grün, dazu das Baufeld außerhalb der Flächen
    if d > 0.0 {
        let feld: f64 = gr
            .iter()
            .map(|g| {
                let x = g.zone.baufeld.max(0.0);
                let u = sk_math::polygon::perimeter(&g.outline);
                (u * x + eckzahl(&g.outline) * x * x) / 1e6
            })
            .sum();
        let rest = (feld - s.im_baufeld).max(0.0);
        let eben = s.andecken + rest;
        let dicke = format!("Dicke {} m", zahl(d, 1e3));
        if eben > 0.0 {
            neu.push(zeile(
                SPREAD,
                KG_ANDECKEN,
                eben * 1e6,
                format!(
                    "Geländeflächen {} + Baufeld außerhalb {} (waagerecht), {dicke}",
                    m2(s.andecken),
                    m2(rest)
                ),
                Vec::new(),
            ));
        }
        if s.andecken_steil > 0.0 {
            neu.push(zeile(
                SPREAD_SLOPE,
                KG_ANDECKEN,
                s.andecken_steil * 1e6,
                format!("Böschungen steiler als 1:4, schräg gemessen, {dicke}"),
                Vec::new(),
            ));
        }
        // Oberbodenbilanz: abgetragen gegen Andeckung
        let ab = out
            .iter()
            .filter(|r| r.key == TOPSOIL)
            .map(|r| r.value)
            .sum::<f64>()
            / 1e9
            + ob_site;
        let bedarf = (eben + s.andecken_steil) * d / 1000.0;
        let ansatz = format!(
            "Andecken {} × {} m = {} gegen abgetragen {}",
            m2(eben + s.andecken_steil),
            zahl(d, 1e3),
            m3(bedarf),
            m3(ab)
        );
        if bedarf > ab + 1e-9 {
            neu.push(zeile(
                TOPSOIL_SUPPLY,
                KG_GELAENDE,
                (bedarf - ab) * 1e9,
                ansatz,
                Vec::new(),
            ));
        } else if ab > bedarf + 1e-9 {
            neu.push(zeile(
                TOPSOIL_REMOVE,
                KG_GELAENDE,
                (ab - bedarf) * 1e9,
                ansatz,
                Vec::new(),
            ));
        }
    }
    // hinter den Mengen des Trägers
    let nach = out
        .iter()
        .rposition(|r| r.element == traeger.vorlage.element && r.key.starts_with("earth."))
        .map_or(out.len(), |k| k + 1);
    out.splice(nach..nach, neu);
}

/// Abfuhr der Gründung auf `wert` (mm³) setzen und den Ansatz ergänzen;
/// fehlt die Zeile, entsteht sie, bei 0 fällt sie weg.
fn abfuhr_setzen(out: &mut Vec<AutoMenge>, vorlage: &AutoMenge, wert: f64, zusatz: &str) {
    let el = vorlage.element;
    match out
        .iter()
        .position(|r| r.element == el && r.key == DISPOSAL)
    {
        Some(k) if wert > 0.0 => {
            out[k].value = wert;
            out[k].formula.push_str(zusatz);
        }
        Some(k) => {
            out.remove(k);
        }
        None if wert > 0.0 => {
            let nach = out
                .iter()
                .rposition(|r| r.element == el && r.key.starts_with("earth."))
                .map_or(out.len(), |k| k + 1);
            out.insert(
                nach,
                AutoMenge {
                    key: DISPOSAL,
                    unit: "m3",
                    value: wert,
                    kg: Some(KG),
                    formula: zusatz.trim_start_matches("; ").to_string(),
                    hinweis: String::new(),
                    ..vorlage.clone()
                },
            );
        }
        None => {}
    }
}

/// Überlappen sich zwei Geländeflächen des Modells (im Grundriss, mehr
/// als 1 cm²)?
fn ueberlappen(m: &Model) -> bool {
    let f = m.site_patches();
    (0..f.len()).any(|i| {
        (i + 1..f.len())
            .any(|j| sk_gelaende::planung::ueberlappung(&f[i].outline, &f[j].outline) > 100.0)
    })
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::BEFESTIGT;
    use crate::model::{SiteSoil, Terrain, TerrainPatch};
    use crate::qto::AutoMenge;
    use crate::Model;

    /// Haus 10 × 8 m auf ebenem Aufmaß 60 × 60 m in 10,00 m ü. NHN, ±0,00
    /// auf 9,50: Gelände 0,50 über OK Platte, N 0,20 über ±0,00.
    fn haus() -> Model {
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            sk_math::vec3(7000.0, 6000.0, 0.0),
            sk_math::vec3(7000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        let a = sk_gelaende::lesen::lesen(b"1 0 0 10\n2 60 0 10\n3 60 60 10\n4 0 60 10\n").unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "eben.txt", "2026-10-14", 32)));
        assert!(m.set_terrain_nhn(9.5));
        m.commit();
        m
    }

    fn rechteck(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<[f64; 2]> {
        vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
    }

    /// Fläche mit senkrechten Kanten (ohne Böschung) in Höhe `z`.
    fn flaeche(r: Vec<[f64; 2]>, z: f64) -> TerrainPatch {
        TerrainPatch {
            slopes: vec![0.0],
            ..TerrainPatch::flat(r, z)
        }
    }

    fn mit(m: &Model, p: &[TerrainPatch]) -> Vec<AutoMenge> {
        let mut m = m.clone();
        m.begin("Fläche");
        for p in p {
            assert!(m.add_site_patch(p.clone()).is_some());
        }
        m.commit();
        crate::qto::schedule(&m).auto
    }

    /// Summe in m³ bzw. m².
    fn wert(v: &[AutoMenge], k: &str) -> f64 {
        let teiler = if k == SPREAD || k == SPREAD_SLOPE {
            1e6
        } else {
            1e9
        };
        v.iter()
            .filter(|r| r.key == k)
            .map(|r| r.value)
            .sum::<f64>()
            / teiler
    }

    fn nah(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// Fläche fern vom Haus, 10 × 10 m, P 0,10 über ±0,00: Abtrag N − Q =
    /// 0,20 + 0,10 = 0,30 m, alles Überschuss; Oberboden auf der ganzen
    /// Fläche, Andecken dort und auf dem Baufeld um die Platte.
    #[test]
    fn abtrag_fern_vom_haus() {
        let m = haus();
        let ohne = crate::qto::schedule(&m).auto;
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0)]);
        assert!(nah(wert(&v, CUT), 30.0), "{}", wert(&v, CUT));
        assert!(nah(wert(&v, SURPLUS), 30.0));
        assert_eq!(wert(&v, EMBANK) + wert(&v, EMBANK_SUPPLY), 0.0);
        assert!(nah(wert(&v, TOPSOIL_SITE), 30.0));
        // Baufeld 1,50 m um 10 × 8: 36 × 1,5 + 4 × 1,5² = 63 m²
        assert!(nah(wert(&v, SPREAD), 163.0), "{}", wert(&v, SPREAD));
        assert_eq!(wert(&v, SPREAD_SLOPE), 0.0);
        // Oberboden (80 + 63) × 0,30 + 30 gegen 163 × 0,20
        let ab = 143.0 * 0.3 + 30.0;
        assert!(nah(wert(&v, TOPSOIL_REMOVE), ab - 32.6));
        // die Mengen der Gründung bleiben
        for k in [TOPSOIL, EXCAVATION, TRENCH, BACKFILL, DISPOSAL] {
            assert!(nah(wert(&v, k), wert(&ohne, k)), "{k}");
        }
        // nach den Mengen der Gründung, KG und Verfahren
        let cut = v.iter().find(|r| r.key == CUT).unwrap();
        assert_eq!(cut.kg, Some(KG_GELAENDE));
        assert!(cut.formula.contains(DREIECKSPRISMEN), "{}", cut.formula);
        let spread = v.iter().find(|r| r.key == SPREAD).unwrap();
        assert_eq!((spread.kg, spread.unit), (Some(KG_ANDECKEN), "m2"));
        // Überschuss: Hinweis zum Faktor
        let s = v.iter().find(|r| r.key == SURPLUS).unwrap();
        assert!(s.hinweis.contains("Faktor 1,00"), "{}", s.hinweis);
    }

    /// Auftrag fern vom Haus: zuerst aus dem Überschuss der Baugrube, der
    /// Rest Lieferung (B4); die Abfuhr der Gründung sinkt.
    #[test]
    fn auftrag_aus_der_baugrube() {
        let m = haus();
        let ohne = crate::qto::schedule(&m).auto;
        let d = wert(&ohne, DISPOSAL);
        assert!(d > 0.0);
        // P 0,90: Q 0,70 über N 0,20, 0,50 m auf 100 m²
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 900.0)]);
        assert_eq!(wert(&v, CUT), 0.0);
        let eigen = d.min(50.0);
        assert!(nah(wert(&v, EMBANK), eigen), "{} {d}", wert(&v, EMBANK));
        assert!(nah(wert(&v, EMBANK_SUPPLY), 50.0 - eigen));
        assert!(nah(wert(&v, DISPOSAL), d - eigen));
        // Faktor 0,90: aus der Baugrube nur 0,9 × fest eingebaut
        let mut m2 = m.clone();
        m2.begin("Boden");
        assert!(m2.set_site_soil(SiteSoil {
            spread: 200.0,
            reuse: 0.9
        }));
        m2.commit();
        let v = mit(&m2, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 900.0)]);
        let eigen = (d * 0.9).min(50.0);
        assert!(nah(wert(&v, EMBANK), eigen));
        assert!(nah(wert(&v, DISPOSAL), d - eigen / 0.9));
        let e = v.iter().find(|r| r.key == EMBANK_SUPPLY).unwrap();
        assert!(e.hinweis.contains("Faktor 0,90"), "{}", e.hinweis);
    }

    /// Fläche am Haus im Arbeitsraum (B3): dort ist bis UK Platte
    /// ausgehoben; der Abtrag mindert die Verfüllung, nur unter der Sohle
    /// zählt er.
    #[test]
    fn im_arbeitsraum() {
        let m = haus();
        let ohne = crate::qto::schedule(&m).auto;
        let g = m.ground_bases().remove(0);
        let b = g.slab_bottom_z / 1000.0;
        assert!(b < 0.2, "UK Platte unter N: {b}");
        // P −0,15, Q −0,35 unter B: im Arbeitsraum (0,50 × 8 m) weniger
        // Verfüllung N − B, Abtrag B − Q
        let v = mit(&m, &[flaeche(rechteck(17e3, 6e3, 20e3, 14e3), -150.0)]);
        let ring = 0.5 * 8.0;
        let q = -0.35;
        let weniger = ring * (0.2 - b);
        assert!(nah(wert(&v, BACKFILL), wert(&ohne, BACKFILL) - weniger));
        let cut = 20.0 * (0.2 - q) + ring * (b - q);
        assert!(nah(wert(&v, CUT), cut), "{} {cut}", wert(&v, CUT));
        // Abfuhr: + weniger Verfüllung; der Geländeabtrag fährt eigens ab
        assert!(nah(wert(&v, DISPOSAL), wert(&ohne, DISPOSAL) + weniger));
        assert!(nah(wert(&v, SURPLUS), cut));
        let c = v.iter().find(|r| r.key == CUT).unwrap();
        assert!(c.hinweis.contains("unter der Sohle"), "{}", c.hinweis);
        // Andecken: Fläche außerhalb des Hauses, das Baufeld zählt nicht
        // doppelt (63 m² − 1,50 × 8 m im Baufeld)
        assert!(
            nah(wert(&v, SPREAD), 24.0 + 63.0 - 12.0),
            "{}",
            wert(&v, SPREAD)
        );
    }

    /// Befestigt: Q = P, kein Andecken, Hinweis B2; überlappende Flächen
    /// mit Hinweis.
    #[test]
    fn befestigt_und_ueberlappend() {
        let m = haus();
        let p = TerrainPatch {
            paved: true,
            ..flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0)
        };
        let v = mit(&m, std::slice::from_ref(&p));
        // N 0,20 − Q 0,10
        assert!(nah(wert(&v, CUT), 10.0));
        assert!(nah(wert(&v, SPREAD), 63.0));
        let c = v.iter().find(|r| r.key == CUT).unwrap();
        assert!(c.hinweis.contains(BEFESTIGT), "{}", c.hinweis);
        let mut q = p;
        q.outline = rechteck(35e3, 35e3, 45e3, 45e3);
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0), q]);
        let c = v.iter().find(|r| r.key == CUT).unwrap();
        assert!(c.hinweis.contains("überlappen"), "{}", c.hinweis);
    }
}

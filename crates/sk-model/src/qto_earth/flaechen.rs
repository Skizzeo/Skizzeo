//! Erdmengen der Geländeflächen (G3b, vob/pruefung-gelaende-g3.md B1 bis
//! B7): je Fläche das Planungsgelände über dem Netz des Aufmaßes
//! ([`sk_gelaende::planung`]), Abtrag und Auftrag zwischen N und Q,
//! Oberboden auf den Flächen außerhalb des Baufelds, Andecken auf allen
//! Grünflächen mit Oberbodenabtrag. In der Baugrube zählt die Aushubfläche:
//! Abtrag dort mindert die Verfüllung der Gründung.
//!
//! Die Massenbilanz verteilt den Eigenboden in fester Reihenfolge (B4):
//! Verfüllung aus der Baugrube (schon in G2), Geländeauftrag aus dem
//! Geländeabtrag, Rest aus dem Überschuss der Baugruben (anteilig zum
//! Überschuss jeder Baugrube, E2), Rest Lieferung; was bleibt, fährt ab
//! (Baugrube 311, Gelände 511). Der Faktor fest → eingebaut wirkt nur in
//! der Bilanz. Oberboden hat eine eigene Bilanz.
//! Ohne Geländeflächen bleibt alles wie in G2, Zeile für Zeile.
//!
//! Die Zeilen der Flächen gehören zum Grundstück, nicht zu einem Haus
//! (VOB-Prüfung G3, E1): ohne Gebäude, nur im Umfang Projekt. Bauteil,
//! Nummer und Geschoss sind die des Trägers, damit Hinweise und Sortierung
//! einen Ort haben. Verfüllung und Abfuhr bleiben an ihrem Haus.

use super::{eckzahl, zahl, Boden, ErdZone, SCHLUESSEL};
use super::{BACKFILL, CUT, DISPOSAL, EMBANK, EMBANK_SUPPLY, SURPLUS, TOPSOIL, TOPSOIL_SITE};
use super::{KG, KG_ANDECKEN, KG_GELAENDE, SPREAD, SPREAD_SLOPE, TOPSOIL_REMOVE, TOPSOIL_SUPPLY};
use crate::model::{SurveyFrame, TerrainMesh, TerrainPatch};
use crate::qto::AutoMenge;
use crate::ElementId;
use crate::Model;
use sk_gelaende::planung::{Aufbau, Erdbau, Grube, Hinweis, Planung, Zerlegt, Zonen};
use sk_math::Vec3;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// Eine Gründung, wie die Geländeflächen sie sehen: die Vorlage ihrer
/// Mengen (Träger Sohlplatte), Plattenumriss und Zone aus G2.
#[derive(Clone, Debug)]
pub struct Gruendung {
    pub vorlage: AutoMenge,
    pub outline: Vec<Vec3>,
    pub zone: ErdZone,
}

/// Hinweis, solange der Böschungskeil der Baugrube nicht exakt zählt
/// (VOB-Prüfung G3, C2; exakt mit G5).
pub const KEIL: &str = "Baugrube mit Böschung: Geländeabtrag über dem Böschungskeil angenähert";
/// Hinweis an befestigten Flächen, solange ihr Aufbau nicht zählt (B2).
pub const BEFESTIGT: &str =
    "Aufbau der befestigten Fläche wird noch nicht gerechnet, Auftrag bis OK Fläche";
/// Hinweis bei gewähltem Haus, wenn es Zeilen des Grundstücks gibt
/// (VOB-Prüfung G3, E1).
pub const GRUNDSTUECK: &str =
    "Erdarbeiten des Grundstücks: nicht in diesem Haus, siehe Außenanlagen";
/// Schlüssel der Zeilen, die zum Grundstück gehören und nicht zu einem Haus
/// (VOB-Prüfung G3, E1: Außenanlagen, KG 500). Verfüllung und Abfuhr der
/// Baugrube bleiben am Haus.
pub const GRUNDSTUECK_SCHLUESSEL: [&str; 9] = [
    TOPSOIL_SITE,
    CUT,
    EMBANK,
    EMBANK_SUPPLY,
    SURPLUS,
    TOPSOIL_REMOVE,
    TOPSOIL_SUPPLY,
    SPREAD,
    SPREAD_SLOPE,
];
/// Hinweis, wenn die Bilanz begrenzt (B4).
pub const FAKTOR: &str = "Eigenboden fest zu eingebaut {f} angenommen; ändert die \
                          Aufteilung Eigenboden/Lieferung";

/// Hinweis, wenn eine Fläche über das Aufmaß hinausreicht (H-173c).
pub const AUSSERHALB: &str = "Geländefläche reicht über das Aufmaß hinaus: dort ohne Mengen";
/// Hinweis, wenn sich eine Fläche nicht bauen lässt (H-173c).
pub const UNGUELTIG: &str = "Geländefläche lässt sich nicht bauen: ohne Mengen";
/// Hinweise, wenn Flächen ganz ohne Menge bleiben (H-173c, VOB-Prüfung G3:
/// nie still, sonst fehlt ein Vordersatz unbemerkt).
pub const GANZ_AUSSERHALB: &str = "Geländefläche ohne Mengen: liegt außerhalb des Aufmaßes";
pub const OHNE_AUFMASS: &str = "Geländefläche ohne Mengen: kein Höhenaufmaß";
// „keine Sohlplatte als Träger“ las sich als Balken (Bedienbarkeit, später
// zu Durchgang 43)
pub const OHNE_TRAEGER: &str = "Geländefläche ohne Mengen: kein Haus mit Bodenplatte im Modell";
/// Hinweis, wenn sich Flächen überlappen.
pub const UEBERLAPPT: &str =
    "Geländeflächen überlappen sich: Menge dort doppelt gezählt, Flächen prüfen";
/// Kleinere Mengen (m³, m²) sind Rundungsreste und geben keine Zeile
/// (H-173a), wie in der Oberbodenbilanz.
const REST: f64 = 1e-9;

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

/// Was die Flächen ergeben, bevor die Bilanz es auf die Zeilen verteilt.
#[derive(Default)]
pub(crate) struct Teil {
    s: Summe,
    /// Weniger Verfüllung je Gründung (m³).
    weniger: Vec<f64>,
    /// Böschungskeil der Baugrube unter einer Fläche, je Gründung (C2).
    keil: Vec<bool>,
}

/// Was eine Fläche ohne die Gründungen ergibt; es hängt nur von ihr, von
/// Lage und Höhe von ±0,00 und vom Netz ab: das Planungsgelände mit dem
/// Rechteck darum (`None`: lässt sich nicht bauen), die Ecken im Aufmaß
/// und die Zerlegung im Modell.
pub(crate) struct Plan {
    planung: Option<(Planung, [f64; 4])>,
    innen: usize,
    zerlegt: Zerlegt,
}

/// Was eine Fläche mit den Gründungen in ihrer Nähe ergibt, mit dem
/// Schlüssel, von dem es abhängt: Oberboden, Andeckung und die Zonen
/// dieser Gründungen als Bits.
pub(crate) struct Einzel {
    schluessel: Vec<u64>,
    e: Option<Erdbau>,
    hinweise: Vec<&'static str>,
}

/// Eine Fläche mit beiden Stufen.
struct Gerechnet {
    patch: TerrainPatch,
    plan: Arc<Plan>,
    einzel: Arc<Einzel>,
}

/// Der zuletzt gerechnete Stand: das Netz (dasselbe, nicht nur ein
/// gleiches), Lage und Höhe von ±0,00 als Bits, die übrigen Werte (Oberboden,
/// Andeckung, Umriss und Zone jeder Gründung), jede Fläche mit ihren
/// Stufen, ob sich Flächen überlappen, und das Ganze.
struct Gemerkt {
    netz: Arc<TerrainMesh>,
    lage: [u64; 4],
    werte: Vec<u64>,
    flaechen: Vec<Gerechnet>,
    ueberlappt: bool,
    teil: Arc<Teil>,
}

/// Platz für den zuletzt gerechneten Stand im Modell (Review #173 B1): Ein
/// Loslassen, das weder Flächen noch Gründungen noch Aufmaß ändert, rechnet
/// nichts neu. Ändert sich eine Fläche, rechnet nur sie neu; ändert sich
/// eine Gründung, nur der Erdbau der Flächen, deren Rechteck ihre Zonen
/// treffen. Das Planungsgelände rechnet nur neu, wenn sich die Fläche,
/// ±0,00 oder das Aufmaß ändert.
#[derive(Clone, Default)]
pub(crate) struct Merker(RefCell<Option<Arc<Gemerkt>>>);

impl std::fmt::Debug for Merker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Flächenmerker")
    }
}

impl Merker {
    fn alt(&self) -> Option<Arc<Gemerkt>> {
        self.0.borrow().clone()
    }

    /// Ohne Flächen oder Aufmaß nichts festhalten (H-173h), auch nicht das
    /// alte Netz.
    fn leeren(&self) {
        if self.0.borrow().is_some() {
            self.0.borrow_mut().take();
        }
    }
}

/// Die Zeilen einer Gründung, die die Bilanz ändert: Summen und erste
/// Zeile von Verfüllung und Abfuhr, die letzte ihrer Erdmengen.
#[derive(Default)]
struct Zeilen {
    verf: f64,
    abf: f64,
    verf_ix: Option<usize>,
    abf_ix: Option<usize>,
    letzte: Option<usize>,
}

/// Rechteck um ein Polygon (x0, y0, x1, y1).
fn rechteck(ring: &[[f64; 2]]) -> [f64; 4] {
    ring.iter()
        .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |r, q| {
            [
                r[0].min(q[0]),
                r[1].min(q[1]),
                r[2].max(q[0]),
                r[3].max(q[1]),
            ]
        })
}

fn treffen(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// Mengen der Geländeflächen des Modells `m` an die Mengen `out` der
/// Gründungen `gr` anfügen und deren Verfüllung und Abfuhr nach der Bilanz
/// ändern. Ohne Flächen, Aufmaß oder Gründung nichts.
pub(crate) fn gelaende_mengen(
    m: &Model,
    gr: &[Gruendung],
    boden: &Boden,
    out: &mut Vec<AutoMenge>,
    projekt: &mut Vec<String>,
) {
    if m.site_patches().is_empty() {
        m.flaechenmerker().leeren();
        return;
    }
    // Träger: die Platte mit der kleinsten Nummer, als Ort der Zeilen des
    // Grundstücks; ohne sie steht der Hinweis beim Projekt
    let Some(traeger) = gr
        .iter()
        .min_by_key(|g| (g.vorlage.number.len(), g.vorlage.number.clone()))
    else {
        m.flaechenmerker().leeren();
        projekt.push(OHNE_TRAEGER.to_string());
        return;
    };
    let (Some(t), Some(frame), Some(h0)) = (m.terrain(), m.survey_frame(), m.terrain_nhn()) else {
        m.flaechenmerker().leeren();
        // an jeder Erdmenge des Trägers, so steht er an jeder Position
        let el = traeger.vorlage.element;
        let mut da = false;
        for r in out
            .iter_mut()
            .filter(|r| r.element == el && r.key.starts_with("earth."))
        {
            anhaengen(r, OHNE_AUFMASS);
            da = true;
        }
        if !da {
            projekt.push(OHNE_AUFMASS.to_string());
        }
        return;
    };
    let mesh = t.mesh();
    let o = boden.oberboden.max(0.0);
    let site = m.site_soil();
    // Andecken nur, wo Oberboden abgetragen wurde (B6)
    let d = if o > 0.0 { site.spread.max(0.0) } else { 0.0 };
    let lage = [frame.e0, frame.n0, frame.g, h0].map(f64::to_bits);
    let mut werte: Vec<u64> = [o, d].map(f64::to_bits).to_vec();
    for g in gr {
        let z = &g.zone;
        werte.extend([g.outline.len(), z.sohle.len()].map(|n| n as u64));
        werte.extend([z.baufeld, z.b, if z.keil { 1.0 } else { 0.0 }].map(f64::to_bits));
        for p in g.outline.iter().chain(&z.sohle) {
            werte.extend([p.x, p.y, p.z].map(f64::to_bits));
        }
    }
    let alt = m.flaechenmerker().alt();
    let teil = match alt.as_ref().filter(|g| {
        Arc::ptr_eq(&g.netz, &mesh)
            && g.lage == lage
            && g.werte == werte
            && gleiche_flaechen(&g.flaechen, m.site_patches())
    }) {
        Some(g) => g.teil.clone(),
        None => {
            let alt = alt.filter(|g| Arc::ptr_eq(&g.netz, &mesh) && g.lage == lage);
            let g = rechnen(m, gr, &frame, h0, &mesh, o, d, alt.as_deref());
            let teil = g.teil.clone();
            *m.flaechenmerker().0.borrow_mut() = Some(Arc::new(Gemerkt { lage, werte, ..g }));
            teil
        }
    };
    let Teil { s, weniger, keil } = &*teil;

    let verfahren = super::netz::verfahren(t);
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
            // Grundstück, kein Haus (E1)
            building: None,
            ..traeger.vorlage.clone()
        }
    };

    // Die Zeilen der Gründungen einmal suchen, die Änderungen am Ende in
    // einem Gang setzen (Review #173 B3: vorher je Gründung über alle
    // Zeilen)
    let mut ix: HashMap<ElementId, Zeilen> = gr
        .iter()
        .map(|g| (g.vorlage.element, Zeilen::default()))
        .collect();
    for (k, r) in out.iter().enumerate() {
        let Some(z) = ix.get_mut(&r.element) else {
            continue;
        };
        if r.key.starts_with("earth.") {
            z.letzte = Some(k);
        }
        if r.key == BACKFILL {
            z.verf += r.value;
            z.verf_ix.get_or_insert(k);
        } else if r.key == DISPOSAL {
            z.abf += r.value;
            z.abf_ix.get_or_insert(k);
        }
    }

    // B3: weniger Verfüllung je Gründung, um so viel mehr Überschuss
    let mut ueberschuss = 0.0;
    let mut abfuhr: Vec<f64> = vec![0.0; gr.len()];
    let mut verfuellen: Vec<Option<(f64, String)>> = vec![None; gr.len()];
    let mut zusatz: Vec<Option<String>> = vec![None; gr.len()];
    for (i, g) in gr.iter().enumerate() {
        let z = &ix[&g.vorlage.element];
        let w = (weniger[i] * 1e9).min(z.verf);
        abfuhr[i] = z.abf + w;
        ueberschuss += abfuhr[i];
        if w <= REST * 1e9 {
            continue;
        }
        verfuellen[i] = Some((
            z.verf - w,
            format!("; − Geländeabtrag im Arbeitsraum {}", m3(w / 1e9)),
        ));
        zusatz[i] = Some(format!("; + weniger Verfüllung {}", m3(w / 1e9)));
    }

    // B4: Bilanz in m³ (fest bzw. verdichtet)
    let gel = s.abtrag;
    let auf = s.auftrag;
    let eigen1 = auf.min(gel * f);
    let rest_gel = gel - eigen1 / f;
    let eigen2 = (auf - eigen1).min(ueberschuss / 1e9 * f).max(0.0);
    let liefer = (auf - eigen1 - eigen2).max(0.0);
    // aus den Baugruben entnommen, anteilig zu ihrem Überschuss (E2:
    // neutral gegenüber den Häusern, nicht nach Plattennummer)
    let entnehmen = eigen2 / f * 1e9;
    for i in 0..gr.len() {
        if entnehmen <= 0.0 || ueberschuss <= 0.0 {
            break;
        }
        let x = (entnehmen * abfuhr[i] / ueberschuss).min(abfuhr[i]);
        if x <= 0.0 {
            continue;
        }
        // der Anteil nachprüfbar, wenn mehrere Baugruben abgeben; die Zahlen
        // mit Namen (Bedienbarkeit, später zu Durchgang 43)
        let anteil = if abfuhr[i] < ueberschuss * (1.0 - 1e-9) {
            format!(
                " (Einbau {} m³ × Überschuss dieser Baugrube {} m³ / Überschuss aller \
                 Baugruben {} m³)",
                zahl(entnehmen, 1e9),
                zahl(abfuhr[i], 1e9),
                zahl(ueberschuss, 1e9)
            )
        } else {
            String::new()
        };
        abfuhr[i] -= x;
        zusatz[i]
            .get_or_insert_with(String::new)
            .push_str(&format!("; − Einbau im Gelände {}{anteil}", m3(x / 1e9)));
    }

    // Verfüllung und Abfuhr setzen: ganz entnommen (bis auf Rundungsreste,
    // H173-3) fällt die Zeile weg, eine fehlende
    // Abfuhr entsteht hinter den Erdmengen ihrer Gründung; Keil (C2) an der
    // Verfüllung, die bleibt
    let mut weg = vec![false; out.len()];
    let mut danach: Vec<(usize, AutoMenge)> = Vec::new();
    let mut am_ende = Vec::new();
    for (i, g) in gr.iter().enumerate() {
        let z = &ix[&g.vorlage.element];
        if let (Some(k), Some((neu, text))) = (z.verf_ix, &verfuellen[i]) {
            if *neu > 0.0 {
                out[k].value = *neu;
                out[k].formula.push_str(text);
            } else {
                weg[k] = true;
            }
        }
        if keil[i] {
            if let Some(k) = z.verf_ix.filter(|&k| !weg[k]) {
                anhaengen(&mut out[k], KEIL);
            }
        }
        let Some(text) = &zusatz[i] else {
            continue;
        };
        match z.abf_ix {
            Some(k) if abfuhr[i] > REST * 1e9 => {
                out[k].value = abfuhr[i];
                out[k].formula.push_str(text);
            }
            Some(k) => weg[k] = true,
            None if abfuhr[i] > REST * 1e9 => {
                let r = AutoMenge {
                    key: DISPOSAL,
                    unit: "m3",
                    value: abfuhr[i],
                    kg: Some(KG),
                    formula: text.trim_start_matches("; ").to_string(),
                    hinweis: String::new(),
                    ..g.vorlage.clone()
                };
                match z.letzte {
                    Some(k) => danach.push((k, r)),
                    None => am_ende.push(r),
                }
            }
            None => {}
        }
    }
    if weg.contains(&true) || !danach.is_empty() || !am_ende.is_empty() {
        danach.sort_by_key(|x| x.0);
        let mut danach = danach.into_iter().peekable();
        let alt = std::mem::take(out);
        for (k, r) in alt.into_iter().enumerate() {
            if !weg[k] {
                out.push(r);
            }
            while let Some((_, r)) = danach.next_if(|x| x.0 == k) {
                out.push(r);
            }
        }
        out.extend(am_ende);
    }
    let begrenzt = liefer > 1e-9 || rest_gel > 1e-9 || ueberschuss / 1e9 - eigen2 / f > 1e-9;
    let mut h_erdbau: Vec<String> = s.hinweise.iter().map(|h| h.to_string()).collect();
    if s.befestigt {
        h_erdbau.push(BEFESTIGT.to_string());
    }
    let mut neu = Vec::new();

    // B1: Oberboden auf den Flächen außerhalb des Baufelds
    let ob_site = s.oberboden_fl * o / 1000.0;
    if ob_site > REST {
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
    if gel > REST {
        let mut h = h_erdbau.clone();
        if keil.contains(&true) {
            h.push(KEIL.to_string());
        }
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
    if eigen1 + eigen2 > REST {
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
    if liefer > REST {
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
    if rest_gel > REST {
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
        // die Zulage steiler als 1:4 setzt die Grundposition voraus: dort
        // stehen alle Flächen (VOB-Prüfung G3, C1)
        let alle = eben + s.andecken_steil;
        if alle > 0.0 {
            let steil = if s.andecken_steil > 0.0 {
                format!(" + Böschungen steiler als 1:4 {}", m2(s.andecken_steil))
            } else {
                String::new()
            };
            neu.push(zeile(
                SPREAD,
                KG_ANDECKEN,
                alle * 1e6,
                format!(
                    "Geländeflächen {} (schräg gemessen){steil} + Baufeld außerhalb {} \
                     (waagerecht), {dicke}",
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
        let bedarf = alle * d / 1000.0;
        let ansatz = format!(
            "Andecken {} × {} m = {} gegen abgetragen {}",
            m2(alle),
            zahl(d, 1e3),
            m3(bedarf),
            m3(ab)
        );
        if bedarf > ab + 1e-9 {
            neu.push(zeile(
                TOPSOIL_SUPPLY,
                KG_ANDECKEN,
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
    // Flächen ohne Mengen: der Hinweis an jeder Zeile der Flächen, auch
    // wenn sonst keine Menge aus ihnen folgt (H-173c)
    for h in [AUSSERHALB, GANZ_AUSSERHALB, UNGUELTIG] {
        if !s.hinweise.contains(&h) {
            continue;
        }
        if neu.is_empty() {
            projekt.push(h.to_string());
        }
        for r in neu.iter_mut().filter(|r| !r.hinweis.contains(h)) {
            anhaengen(r, h);
        }
    }
    // hinter den Mengen des Trägers
    let nach = out
        .iter()
        .rposition(|r| r.element == traeger.vorlage.element && r.key.starts_with("earth."))
        .map_or(out.len(), |k| k + 1);
    out.splice(nach..nach, neu);
}

/// Das Planungsgelände jeder Fläche des Modells `m` im Netz `mesh` (für die
/// Darstellung, G3c), in der Reihenfolge der Flächen; `None`, wo sie sich
/// nicht bauen lässt. Was der Merker der Mengen zu Netz, Lage und Fläche
/// schon weiß, rechnet nicht neu.
pub(crate) fn planungen(
    m: &Model,
    frame: &SurveyFrame,
    h0: f64,
    mesh: &Arc<TerrainMesh>,
) -> Vec<Option<Planung>> {
    let lage = [frame.e0, frame.n0, frame.g, h0].map(f64::to_bits);
    let alt = m
        .flaechenmerker()
        .alt()
        .filter(|g| Arc::ptr_eq(&g.netz, mesh) && g.lage == lage);
    m.site_patches()
        .iter()
        .map(|p| {
            let gemerkt = alt
                .as_ref()
                .and_then(|g| g.flaechen.iter().find(|x| x.patch == *p));
            match gemerkt {
                Some(x) => x.plan.planung.as_ref().map(|(pl, _)| pl.clone()),
                None => p
                    .baustein(frame, h0, mesh.bezug)
                    .map(|b| Planung::neu(&b, &mesh.abfrage)),
            }
        })
        .collect()
}

fn gleiche_flaechen(alt: &[Gerechnet], p: &[TerrainPatch]) -> bool {
    alt.len() == p.len() && alt.iter().zip(p).all(|(a, p)| a.patch == *p)
}

/// Eine Gründung im Netz: ihre Zonen, das Rechteck darum und ihre Werte
/// als Bits.
struct Zone {
    haus: Vec<[f64; 2]>,
    baufeld: Option<Vec<[f64; 2]>>,
    grube: Option<Grube>,
    rechteck: [f64; 4],
    bits: Vec<u64>,
}

/// Die Flächen des Modells `m` über dem Netz `mesh` gegen die Gründungen
/// `gr`; Oberboden `o` und Andeckung `d` in mm, ±0,00 auf `h0` m ü. NHN.
/// Was `alt` (bei gleichem Netz und gleicher Lage) zu einer gleichen
/// Fläche schon weiß, rechnet nicht neu, auch nicht die Überlappung
/// zweier solcher Flächen (H-173k).
#[allow(clippy::too_many_arguments)]
fn rechnen(
    m: &Model,
    gr: &[Gruendung],
    frame: &SurveyFrame,
    h0: f64,
    mesh: &Arc<TerrainMesh>,
    o: f64,
    d: f64,
    alt: Option<&Gemerkt>,
) -> Gemerkt {
    let im_netz = |x: f64, y: f64| {
        let (e, n) = frame.to_survey(x, y);
        [e - mesh.bezug.0, n - mesh.bezug.1]
    };
    let ring = |pts: &[Vec3]| pts.iter().map(|p| im_netz(p.x, p.y)).collect::<Vec<_>>();
    let baufelder: Vec<Option<Vec<Vec3>>> = gr
        .iter()
        .map(|g| {
            let x = g.zone.baufeld.max(0.0);
            sk_math::flaeche::versatz(&g.outline, &vec![-x; g.outline.len()])
        })
        .collect();
    let zonen: Vec<Zone> = gr
        .iter()
        .zip(&baufelder)
        .map(|(g, bf)| {
            let haus = ring(&g.outline);
            let baufeld = bf.as_ref().map(|b| ring(b));
            let grube = (g.zone.sohle.len() >= 3).then(|| Grube {
                sohle: ring(&g.zone.sohle),
                platte: haus.clone(),
                b: h0 + g.zone.b / 1000.0,
            });
            let alle = || {
                haus.iter()
                    .chain(baufeld.iter().flatten())
                    .chain(grube.iter().flat_map(|g| &g.sohle))
            };
            let rechteck = rechteck(&alle().copied().collect::<Vec<_>>());
            let mut bits = vec![
                haus.len() as u64,
                baufeld.as_ref().map_or(u64::MAX, |b| b.len() as u64),
                grube.as_ref().map_or(u64::MAX, |g| g.sohle.len() as u64),
                grube.as_ref().map_or(0, |g| g.b.to_bits()),
            ];
            bits.extend(alle().flat_map(|p| p.map(f64::to_bits)));
            Zone {
                haus,
                baufeld,
                grube,
                rechteck,
                bits,
            }
        })
        .collect();
    let plan_neu = |p: &TerrainPatch| {
        let innen = p
            .outline
            .iter()
            .filter(|q| {
                let [x, y] = im_netz(q[0], q[1]);
                mesh.abfrage.innen(x, y)
            })
            .count();
        let planung = p.baustein(frame, h0, mesh.bezug).map(|b| {
            let plan = Planung::neu(&b, &mesh.abfrage);
            let h = plan.huelle();
            (plan, h)
        });
        Plan {
            planung,
            innen,
            zerlegt: Zerlegt::neu(&p.outline),
        }
    };
    let einzel_neu = |p: &TerrainPatch, plan: &Plan, nah: &[usize], schluessel: Vec<u64>| {
        let mut hinweise = Vec::new();
        let mut dazu = |t: &'static str| {
            if !hinweise.contains(&t) {
                hinweise.push(t);
            }
        };
        let e = match &plan.planung {
            None => {
                dazu(UNGUELTIG);
                None
            }
            Some((planung, _)) => {
                let z = Zonen {
                    haus: nah.iter().map(|&i| zonen[i].haus.clone()).collect(),
                    baufeld: nah
                        .iter()
                        .filter_map(|&i| zonen[i].baufeld.clone())
                        .collect(),
                    gruben: nah.iter().filter_map(|&i| zonen[i].grube.clone()).collect(),
                };
                let a = Aufbau {
                    oberboden: o / 1000.0,
                    andecken: d / 1000.0,
                    befestigt: p.paved,
                };
                let e = planung.erdbau(&mesh.abfrage, &a, &z);
                if plan.innen == 0 && e.flaeche + e.im_baufeld <= REST {
                    dazu(GANZ_AUSSERHALB);
                } else if plan.innen < p.outline.len() {
                    dazu(AUSSERHALB);
                }
                for h in &planung.hinweise {
                    dazu(hinweis_text(h));
                }
                Some(e)
            }
        };
        Einzel {
            schluessel,
            e,
            hinweise,
        }
    };
    // gleiche Flächen aus dem alten Stand, je eine einmal; der Erdbau nur
    // gegen die Gründungen, deren Zonen das Rechteck der Fläche treffen:
    // die übrigen ändern ihn nicht
    let mut frei: Vec<&Gerechnet> = alt.map(|g| g.flaechen.iter().collect()).unwrap_or_default();
    let mut flaechen = Vec::new();
    let mut nahe = Vec::new();
    let mut neu = Vec::new();
    for p in m.site_patches() {
        let vorher = frei
            .iter()
            .position(|x| x.patch == *p)
            .map(|k| frei.swap_remove(k));
        neu.push(vorher.is_none());
        let plan = match vorher {
            Some(x) => x.plan.clone(),
            None => Arc::new(plan_neu(p)),
        };
        let nah: Vec<usize> = match &plan.planung {
            Some((_, h)) => (0..gr.len())
                .filter(|&i| treffen(h, &zonen[i].rechteck))
                .collect(),
            None => Vec::new(),
        };
        let mut schluessel: Vec<u64> = [o, d].map(f64::to_bits).to_vec();
        for &i in &nah {
            schluessel.push(zonen[i].bits.len() as u64);
            schluessel.extend(&zonen[i].bits);
        }
        let einzel = match vorher.filter(|x| x.einzel.schluessel == schluessel) {
            Some(x) => x.einzel.clone(),
            None => Arc::new(einzel_neu(p, &plan, &nah, schluessel)),
        };
        flaechen.push(Gerechnet {
            patch: p.clone(),
            plan,
            einzel,
        });
        nahe.push(nah);
    }

    let mut s = Summe::default();
    let mut weniger = vec![0.0; gr.len()];
    let hinweis = |s: &mut Summe, t: &'static str| {
        if !s.hinweise.contains(&t) {
            s.hinweise.push(t);
        }
    };
    for (x, nah) in flaechen.iter().zip(&nahe) {
        for h in &x.einzel.hinweise {
            hinweis(&mut s, h);
        }
        let Some(e) = &x.einzel.e else {
            continue;
        };
        s.abtrag += e.abtrag;
        s.auftrag += e.auftrag;
        s.oberboden_fl += e.oberboden_flaeche();
        s.im_baufeld += e.im_baufeld;
        s.andecken += e.andecken;
        s.andecken_steil += e.andecken_steil;
        s.unter_sohle += e.unter_sohle;
        s.befestigt |= x.patch.paved;
        let gruben = nah.iter().filter(|&&i| zonen[i].grube.is_some());
        for (k, v) in gruben.zip(&e.je_grube) {
            weniger[*k] += v;
        }
    }
    // C2: über dem Böschungskeil einer Baugrube ist schon ausgehoben; exakt
    // erst mit G5, bis dahin ein Hinweis an Abtrag und Verfüllung
    let keil: Vec<bool> = gr
        .iter()
        .zip(&baufelder)
        .map(|(g, bf)| {
            let Some(bf) = bf.as_ref().filter(|_| g.zone.keil) else {
                return false;
            };
            let flach = |v: &[Vec3]| v.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>();
            let feld = Zerlegt::neu(&flach(bf));
            let sohle = Zerlegt::neu(&flach(&g.zone.sohle));
            flaechen.iter().any(|x| {
                let z = &x.plan.zerlegt;
                z.ueberlappung(&feld) - z.ueberlappung(&sohle) > 100.0
            })
        })
        .collect();
    // Überlappten sich die alten Flächen nicht, genügen die Paare mit
    // einer neuen Fläche; sonst alle (das überlappende Paar kann weg sein)
    let z: Vec<&Zerlegt> = flaechen.iter().map(|x| &x.plan.zerlegt).collect();
    let pruefen = |i: usize, j: usize| z[i].ueberlappung(z[j]) > 100.0;
    let alle = |i: usize| (i + 1..z.len()).any(|j| pruefen(i, j));
    let ueberlappt = match alt.map(|g| g.ueberlappt) {
        Some(false) => {
            (0..z.len()).any(|i| (i + 1..z.len()).any(|j| (neu[i] || neu[j]) && pruefen(i, j)))
        }
        Some(true) if !neu.contains(&true) && alt.is_some_and(|g| g.flaechen.len() == z.len()) => {
            true
        }
        _ => (0..z.len()).any(alle),
    };
    if ueberlappt {
        hinweis(&mut s, UEBERLAPPT);
    }
    Gemerkt {
        netz: mesh.clone(),
        lage: [0; 4],
        werte: Vec::new(),
        flaechen,
        ueberlappt,
        teil: Arc::new(Teil { s, weniger, keil }),
    }
}

/// Hinweis `h` an die Zeile `r` anhängen, je Hinweis eine Zeile.
fn anhaengen(r: &mut AutoMenge, h: &str) {
    if !r.hinweis.is_empty() {
        r.hinweis.push('\n');
    }
    r.hinweis.push_str(h);
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::{AUSSERHALB, BEFESTIGT, GANZ_AUSSERHALB, KEIL, OHNE_AUFMASS};
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
        assert!(s.hinweis.contains("eingebaut 1,00"), "{}", s.hinweis);
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
        assert!(e.hinweis.contains("eingebaut 0,90"), "{}", e.hinweis);
    }

    /// Zwei Häuser geben Boden in das Gelände anteilig zu ihrem Überschuss,
    /// nicht das mit der kleineren Nummer alles (VOB-Prüfung G3, E2).
    #[test]
    fn einbau_anteilig_zum_ueberschuss() {
        let mut m = haus();
        m.begin("Haus 2");
        let b = m.add_building(1);
        let pts = [
            sk_math::vec3(30000.0, 6000.0, 0.0),
            sk_math::vec3(30000.0, 14000.0, 0.0),
            sk_math::vec3(40000.0, 14000.0, 0.0),
            sk_math::vec3(40000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.commit();
        let je_haus = |v: &[AutoMenge]| -> Vec<f64> {
            let mut d: Vec<(String, f64)> = v
                .iter()
                .filter(|r| r.key == DISPOSAL)
                .map(|r| (r.number.clone(), r.value / 1e9))
                .collect();
            d.sort_by(|a, b| a.0.cmp(&b.0));
            d.into_iter().map(|x| x.1).collect()
        };
        let ohne = je_haus(&crate::qto::schedule(&m).auto);
        assert_eq!(ohne.len(), 2);
        let summe = ohne[0] + ohne[1];
        // 0,50 m Auftrag auf 25 m², weniger als eine Baugrube abgibt
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 35e3, 35e3), 900.0)]);
        let auf = wert(&v, EMBANK);
        assert!(nah(auf, 12.5), "{auf}");
        assert!(auf < ohne[0]);
        let mit_flaeche = je_haus(&v);
        for (a, b) in ohne.iter().zip(&mit_flaeche) {
            let anteil = auf * a / summe;
            assert!(nah(a - b, anteil), "{ohne:?} {mit_flaeche:?}");
            // mit dem Anteil im Rechenweg (VOB #185)
            let teil = format!(
                "− Einbau im Gelände {} (Einbau {} m³ × Überschuss dieser Baugrube {} m³ / Überschuss aller \
                 Baugruben {} m³)",
                super::m3(anteil),
                super::zahl(auf, 1.0),
                super::zahl(*a, 1.0),
                super::zahl(summe, 1.0)
            );
            assert_eq!(
                v.iter()
                    .filter(|r| r.key == DISPOSAL && r.formula.contains(&teil))
                    .count(),
                1,
                "{teil}"
            );
        }
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

    /// Auftrag mit Böschung 1:1,5 (steiler als 1:4): die Grundposition
    /// Andecken enthält die Böschung, die Zulage nur sie (VOB-Prüfung G3,
    /// C1); die Oberbodenbilanz zählt sie einmal.
    #[test]
    fn andecken_mit_zulage() {
        let m = haus();
        let p = TerrainPatch {
            slopes: vec![1.5],
            ..TerrainPatch::flat(rechteck(30e3, 30e3, 40e3, 40e3), 1500.0)
        };
        let v = mit(&m, &[p]);
        let (alle, steil) = (wert(&v, SPREAD), wert(&v, SPREAD_SLOPE));
        assert!(
            steil > 0.0 && alle > 100.0 + 63.0 + steil - 1e-6,
            "{alle} {steil}"
        );
        let spread = v.iter().find(|r| r.key == SPREAD).unwrap();
        assert!(
            spread.formula.contains("steiler als 1:4"),
            "{}",
            spread.formula
        );
        // Bilanz: alle × 0,20 gegen abgetragen
        let ab = wert(&v, TOPSOIL) + wert(&v, TOPSOIL_SITE);
        let bedarf = alle * 0.2;
        let diff = wert(&v, TOPSOIL_SUPPLY) - wert(&v, TOPSOIL_REMOVE);
        assert!((diff - (bedarf - ab)).abs() < 1e-6, "{diff} {bedarf} {ab}");
        if let Some(r) = v.iter().find(|r| r.key == TOPSOIL_SUPPLY) {
            assert_eq!(r.kg, Some(KG_ANDECKEN));
        }
    }

    /// Baugrube tiefer als 1,25 m mit Böschungskeil, Fläche im Baufeld
    /// außerhalb der Sohle: Hinweis an Abtrag und Verfüllung (C2).
    #[test]
    fn keil_der_baugrube() {
        let mut m = haus();
        m.begin("tiefer");
        assert!(m.set_terrain_nhn(8.0));
        m.commit();
        let v = mit(&m, &[flaeche(rechteck(17.6e3, 6e3, 18.4e3, 14e3), 1800.0)]);
        let c = v.iter().find(|r| r.key == CUT).unwrap();
        assert!(c.hinweis.contains(KEIL), "{}", c.hinweis);
        let b = v.iter().find(|r| r.key == BACKFILL).unwrap();
        assert!(b.hinweis.contains(KEIL), "{}", b.hinweis);
        // fern vom Haus kein Hinweis
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 1800.0)]);
        assert!(v.iter().all(|r| !r.hinweis.contains(KEIL)));
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

    /// Gemerkt, bis sich Flächen, Gründungen oder das Aufmaß ändern; der
    /// Faktor wirkt erst in der Bilanz und rechnet nichts neu (Review #173
    /// B1).
    #[test]
    fn gemerkt_bis_sich_etwas_aendert() {
        let teil = |m: &Model| {
            m.flaechenmerker()
                .0
                .borrow()
                .as_ref()
                .map(|x| std::sync::Arc::as_ptr(&x.teil))
        };
        let mut m = haus();
        m.begin("Fläche");
        let i = m
            .add_site_patch(flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0))
            .unwrap();
        m.commit();
        let v = crate::qto::schedule(&m).auto;
        let t = teil(&m);
        assert!(t.is_some());
        let w = crate::qto::schedule(&m).auto;
        assert_eq!(teil(&m), t);
        assert_eq!(wert(&v, CUT), wert(&w, CUT));
        m.begin("Boden");
        assert!(m.set_site_soil(SiteSoil {
            spread: 200.0,
            reuse: 0.9
        }));
        m.commit();
        let _ = crate::qto::schedule(&m);
        assert_eq!(teil(&m), t);
        // andere Höhe der Fläche: neu
        m.begin("Höhe");
        assert!(m.set_site_patch(i, flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 200.0)));
        m.commit();
        let v = crate::qto::schedule(&m).auto;
        assert_ne!(teil(&m), t);
        assert!(nah(wert(&v, CUT), 20.0), "{}", wert(&v, CUT));
        // eine zweite Fläche: die erste rechnet nicht neu
        let einzeln = |m: &Model| -> Vec<*const super::Einzel> {
            m.flaechenmerker()
                .0
                .borrow()
                .as_ref()
                .map_or(Vec::new(), |g| {
                    g.flaechen
                        .iter()
                        .map(|x| std::sync::Arc::as_ptr(&x.einzel))
                        .collect()
                })
        };
        let erste = einzeln(&m);
        m.begin("zweite");
        assert!(m
            .add_site_patch(flaeche(rechteck(-10e3, 30e3, 0.0, 38e3), 100.0))
            .is_some());
        m.commit();
        let _ = crate::qto::schedule(&m);
        let beide = einzeln(&m);
        assert_eq!((beide.len(), beide[0]), (2, erste[0]));
        // ±0,00 woanders: neu
        let t = teil(&m);
        m.begin("tiefer");
        assert!(m.set_terrain_nhn(9.0));
        m.commit();
        let _ = crate::qto::schedule(&m);
        assert_ne!(teil(&m), t);
    }

    /// Ein weiteres Haus rechnet nur den Erdbau der Flächen neu, deren
    /// Rechteck seine Zonen treffen, und nie das Planungsgelände; das
    /// Ergebnis gleicht dem ohne Merker (Review #173 B1).
    #[test]
    fn nur_flaechen_am_geaenderten_haus() {
        let zeiger = |m: &Model| -> Vec<(*const super::Plan, *const super::Einzel)> {
            m.flaechenmerker()
                .0
                .borrow()
                .as_ref()
                .map_or(Vec::new(), |g| {
                    g.flaechen
                        .iter()
                        .map(|x| {
                            (
                                std::sync::Arc::as_ptr(&x.plan),
                                std::sync::Arc::as_ptr(&x.einzel),
                            )
                        })
                        .collect()
                })
        };
        let wie_frisch = |m: &Model| {
            let v = crate::qto::schedule(m).auto;
            let frisch = m.clone();
            frisch.flaechenmerker().leeren();
            let w = crate::qto::schedule(&frisch).auto;
            let z = |v: &[AutoMenge]| {
                v.iter()
                    .map(|r| (r.key, r.value.to_bits(), r.hinweis.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(z(&v), z(&w));
            v
        };
        let neues_haus = |m: &mut Model, x0: f64, y0: f64| {
            let b = m.add_building(1);
            let pts = [
                sk_math::vec3(x0, y0, 0.0),
                sk_math::vec3(x0, y0 + 6000.0, 0.0),
                sk_math::vec3(x0 + 7000.0, y0 + 6000.0, 0.0),
                sk_math::vec3(x0 + 7000.0, y0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
        };
        let mut m = haus();
        m.begin("Flächen");
        // am Haus im Arbeitsraum und fern davon
        assert!(m
            .add_site_patch(flaeche(rechteck(17e3, 6e3, 20e3, 14e3), -150.0))
            .is_some());
        assert!(m
            .add_site_patch(flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0))
            .is_some());
        m.commit();
        let _ = wie_frisch(&m);
        let z0 = zeiger(&m);
        assert_eq!(z0.len(), 2);
        // fern von beiden Flächen: nichts neu bis auf das Ganze
        neues_haus(&mut m, 30e3, -15e3);
        let _ = wie_frisch(&m);
        assert_eq!(zeiger(&m), z0);
        // neben der zweiten Fläche: nur deren Erdbau neu
        neues_haus(&mut m, 22e3, 32e3);
        let v = wie_frisch(&m);
        let z1 = zeiger(&m);
        assert_eq!(z1[0], z0[0]);
        assert_eq!(z1[1].0, z0[1].0);
        assert_ne!(z1[1].1, z0[1].1);
        assert_eq!(m.ground_bases().len(), 3);
        assert!(wert(&v, CUT) > 0.0);
    }

    /// Die Bilanz verteilt Abfuhr und Einbau in fester Reihenfolge der
    /// Gründungen: nach Entfernen und neuem Bauen eines Hauses und nach
    /// Speichern und Öffnen gleich je Haus (Review #173 B2).
    #[test]
    fn bilanz_je_haus_nach_speichern_gleich() {
        use std::collections::BTreeMap;
        let neues_haus = |m: &mut Model, x0: f64, y0: f64| {
            let b = m.add_building(1);
            let pts = [
                sk_math::vec3(x0, y0, 0.0),
                sk_math::vec3(x0, y0 + 6000.0, 0.0),
                sk_math::vec3(x0 + 7000.0, y0 + 6000.0, 0.0),
                sk_math::vec3(x0 + 7000.0, y0, 0.0),
            ];
            m.build_from_polygon(b, &pts).unwrap();
            b
        };
        // je Haus (Nummer der Sohlplatte) und Schlüssel die Menge in mm³
        let je_haus = |m: &Model| -> BTreeMap<(String, &'static str), i64> {
            let mut out = BTreeMap::new();
            for r in crate::qto::schedule(m).auto {
                if r.key.starts_with("earth.") {
                    *out.entry((r.number.clone(), r.key)).or_insert(0) += r.value.round() as i64;
                }
            }
            out
        };
        let mut m = haus();
        let b = neues_haus(&mut m, 25e3, 5e3);
        neues_haus(&mut m, 25e3, -15e3);
        assert!(m.remove_building(b));
        neues_haus(&mut m, 25e3, 5e3);
        m.begin("Fläche");
        // Auftrag 0,70 − 0,20 = 0,50 m auf 8 × 8 m: aus den Baugruben
        assert!(m
            .add_site_patch(flaeche(rechteck(-10e3, 30e3, -2e3, 38e3), 900.0))
            .is_some());
        m.commit();
        let vorher = je_haus(&m);
        let einbau: Vec<_> = crate::qto::schedule(&m)
            .auto
            .into_iter()
            .filter(|r| r.key == DISPOSAL && r.formula.contains("Einbau im Gelände"))
            .collect();
        assert!(!einbau.is_empty(), "{vorher:?}");
        // eine ganz entnommene Baugrube ohne Abfuhrzeile aus Rundungsresten
        // (H173-3)
        assert!(crate::qto::schedule(&m)
            .auto
            .iter()
            .filter(|r| r.key == DISPOSAL)
            .all(|r| r.value > super::REST * 1e9));
        let text = crate::szo::write(&m);
        let wieder = crate::szo::read(&text, crate::GuidGen::with_seed(3))
            .unwrap()
            .model;
        assert_eq!(je_haus(&wieder), vorher);
        // die Zeilen je Schlüssel in der Reihenfolge der Nummern, vor und
        // nach dem Öffnen gleich (H-185a)
        let folge = |m: &Model| -> Vec<(&'static str, String)> {
            crate::qto::schedule(m)
                .auto
                .into_iter()
                .filter(|r| r.key.starts_with("earth.") && r.building.is_some())
                .map(|r| (r.key, r.number))
                .collect()
        };
        let f = folge(&m);
        for k in [DISPOSAL, BACKFILL] {
            let n: Vec<&String> = f.iter().filter(|x| x.0 == k).map(|x| &x.1).collect();
            // nach dem Rang des Trägers: Länge, dann Nummer (H-186a)
            let rang = |s: &String| (s.len(), s.clone());
            assert!(n.windows(2).all(|w| rang(w[0]) < rang(w[1])), "{k}: {n:?}");
        }
        assert_eq!(folge(&wieder), f);
    }

    /// Ohne Flächen oder ohne Aufmaß hält der Merker nichts fest, auch
    /// nicht das alte Netz (H-173h).
    #[test]
    fn leer_ohne_flaechen() {
        let gemerkt = |m: &Model| m.flaechenmerker().0.borrow().is_some();
        let mut m = haus();
        m.begin("Fläche");
        let i = m
            .add_site_patch(flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0))
            .unwrap();
        m.commit();
        let _ = crate::qto::schedule(&m);
        assert!(gemerkt(&m));
        m.begin("weg");
        assert!(m.remove_site_patch(i));
        m.commit();
        let _ = crate::qto::schedule(&m);
        assert!(!gemerkt(&m));
    }

    /// Ganz unter dem Haus: keine Zeile aus Rundungsresten (H-173a).
    #[test]
    fn unter_dem_haus_nichts() {
        let m = haus();
        let v = mit(&m, &[flaeche(rechteck(8e3, 7e3, 16e3, 13e3), 100.0)]);
        for k in [CUT, SURPLUS, EMBANK, EMBANK_SUPPLY, TOPSOIL_SITE] {
            assert!(v.iter().all(|r| r.key != k), "{k}");
        }
    }

    /// Über das Aufmaß hinaus: Hinweis an den Mengen (H-173c).
    #[test]
    fn ueber_das_aufmass_hinaus() {
        let m = haus();
        // das Aufmaß reicht im Modell bis x 42 m: teils darüber hinaus
        let v = mit(&m, &[flaeche(rechteck(35e3, 30e3, 50e3, 38e3), 100.0)]);
        let c = v.iter().find(|r| r.key == CUT).unwrap();
        assert!(c.hinweis.contains(AUSSERHALB), "{}", c.hinweis);
        // ganz außerhalb: ohne Abtrag, der Hinweis am Andecken
        let v = mit(&m, &[flaeche(rechteck(50e3, 50e3, 70e3, 70e3), 100.0)]);
        assert!(v.iter().all(|r| r.key != CUT));
        let s = v.iter().find(|r| r.key == SPREAD).unwrap();
        assert!(s.hinweis.contains(GANZ_AUSSERHALB), "{}", s.hinweis);
        // ohne Höhenaufmaß: der Hinweis an den Mengen der Gründung
        let mut ohne = Model::with_seed(7);
        let b = ohne.add_building(1);
        let pts = [
            sk_math::vec3(7000.0, 6000.0, 0.0),
            sk_math::vec3(7000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 6000.0, 0.0),
        ];
        ohne.build_from_polygon(b, &pts).unwrap();
        let v = mit(&ohne, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0)]);
        assert!(
            v.iter()
                .any(|r| r.key.starts_with("earth.") && r.hinweis.contains(OHNE_AUFMASS)),
            "{v:?}"
        );
        let v = mit(&m, &[flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0)]);
        assert!(v.iter().all(|r| !r.hinweis.contains(AUSSERHALB)));
    }

    /// Messung (Review #173 B1): Netz mit rund 650.000 Dreiecken, Flächen
    /// als Kreise mit 400 Kanten. Kalt, warm (nichts geändert, wie beim
    /// Loslassen einer Wand) und nach Änderung einer Fläche.
    /// `cargo test --release -p sk-model tempo_flaechen -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn tempo_flaechen() {
        use std::fmt::Write;
        use std::time::Instant;
        let mut txt = String::new();
        let k = 571;
        for i in 0..k {
            for j in 0..k {
                let (x, y) = (i as f64 * 0.25, j as f64 * 0.25);
                let z = 10.0 + 0.4 * (x / 9.0).sin() + 0.3 * (y / 7.0).cos();
                let _ = writeln!(txt, "{} {x} {y} {z}", i * k + j);
            }
        }
        let a = sk_gelaende::lesen::lesen(txt.as_bytes()).unwrap();
        let mut m = Model::with_seed(7);
        let b = m.add_building(1);
        let pts = [
            sk_math::vec3(7000.0, 6000.0, 0.0),
            sk_math::vec3(7000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 14000.0, 0.0),
            sk_math::vec3(17000.0, 6000.0, 0.0),
        ];
        m.build_from_polygon(b, &pts).unwrap();
        m.begin("Aufmaß");
        assert!(m.import_terrain(Terrain::from_survey(&a, "tempo.txt", "2026-10-14", 32)));
        assert!(m.set_terrain_nhn(9.5));
        m.commit();
        let n_dr = m.terrain().unwrap().mesh().netz.dreiecke.len();
        let kreis = |cx: f64, cy: f64, z: f64| {
            let r: Vec<[f64; 2]> = (0..400)
                .map(|i| {
                    let w = i as f64 / 400.0 * std::f64::consts::TAU;
                    [cx + 4000.0 * w.cos(), cy + 4000.0 * w.sin()]
                })
                .collect();
            TerrainPatch::flat(r, z)
        };
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let _ = crate::qto::schedule(&m);
        eprintln!("{n_dr} Dreiecke, ohne Flächen {:.1} ms", ms(t));
        for anzahl in [1usize, 10, 100] {
            let mut m = m.clone();
            m.begin("Flächen");
            for f in 0..anzahl {
                let (cx, cy) = (
                    -10_000.0 + (f % 10) as f64 * 11_000.0,
                    30_000.0 + (f / 10) as f64 * 9_000.0,
                );
                let z = if f % 2 == 0 { 600.0 } else { -200.0 };
                assert!(m.add_site_patch(kreis(cx, cy, z)).is_some());
            }
            m.commit();
            let t = Instant::now();
            let _ = crate::qto::schedule(&m);
            let kalt = ms(t);
            let t = Instant::now();
            let _ = crate::qto::schedule(&m);
            let warm = ms(t);
            // Darstellung (G3c): das Planungsgelände mit den Plänen aus dem
            // Merker
            let t = Instant::now();
            let pt = m.planned_terrain().unwrap();
            let bild = ms(t);
            eprintln!(
                "{anzahl} × 400 Kanten: Planungsgelände {bild:.1} ms, {} Dreiecke ersetzt durch {}, {} Schraffen",
                pt.replaced.len(),
                pt.triangles.len(),
                pt.hatch.len()
            );
            m.begin("eine");
            assert!(m.set_site_patch(0, kreis(-10_000.0, 30_000.0, 700.0)));
            m.commit();
            let t = Instant::now();
            let _ = crate::qto::schedule(&m);
            let eine = ms(t);
            let t = Instant::now();
            let _ = m.planned_terrain().unwrap();
            let eine_bild = ms(t);
            let haus = |m: &mut Model, x0: f64, y0: f64| {
                let b = m.add_building(1);
                let pts = [
                    sk_math::vec3(x0, y0, 0.0),
                    sk_math::vec3(x0, y0 + 6000.0, 0.0),
                    sk_math::vec3(x0 + 7000.0, y0 + 6000.0, 0.0),
                    sk_math::vec3(x0 + 7000.0, y0, 0.0),
                ];
                m.build_from_polygon(b, &pts).unwrap();
                let t = Instant::now();
                let _ = crate::qto::schedule(m);
                ms(t)
            };
            let fern = haus(&mut m, 30_000.0, 2_000.0);
            let t = Instant::now();
            let _ = m.planned_terrain().unwrap();
            let fern_bild = ms(t);
            let nah = haus(&mut m, -14_000.0, 17_000.0);
            eprintln!(
                "{anzahl} × 400 Kanten: kalt {kalt:.1} ms, warm {warm:.1} ms, \
                 eine Fläche geändert {eine:.1} ms, Haus fern {fern:.1} ms, \
                 Haus an einer Fläche {nah:.1} ms; Bild nach geänderter Fläche \
                 {eine_bild:.1} ms, nach Wandschritt {fern_bild:.1} ms"
            );
        }
    }

    /// Ohne Sohlplatte trägt keine Zeile den Hinweis: er steht in der
    /// Mengenliste als Projekthinweis (VOB-Prüfung G3, D2).
    #[test]
    fn ohne_sohlplatte_projekthinweis() {
        let mut m = Model::with_seed(7);
        m.begin("Fläche");
        assert!(m
            .add_site_patch(flaeche(rechteck(30e3, 30e3, 40e3, 40e3), 100.0))
            .is_some());
        m.commit();
        let s = crate::qto::schedule(&m);
        assert!(s.auto.iter().all(|r| !r.key.starts_with("earth.")));
        assert_eq!(s.hinweise, vec![super::OHNE_TRAEGER.to_string()]);
        let u = crate::qto::Umfang::default();
        assert_eq!(s.restrict(&m, &u).hinweise, s.hinweise);
        assert!(crate::qto::schedule(&haus()).hinweise.is_empty());
    }
}

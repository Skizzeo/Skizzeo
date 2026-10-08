//! Kostenrechnung (KA-0e): Mengenzeilen des fertigen `Schedule` →
//! Zuordnung (Regel 81) → Positionen mit Mengenansatz → EP und GP in Cent
//! (Regel 83) → Summen mit Rundungsausgleich (Regel 96). Liest nur; rechnet
//! keine Geometrie und ruft nie `qto::schedule` (Regel 98).
//!
//! `kosten` und `kosten_mit` sind derselbe Weg: `kosten` ist `kosten_mit`
//! mit leerem Speicher. Der Speicher hält die Zuordnung je Bauteilart, Typ
//! und Schichtfolge (Stufe 1, Bausteingrenze §5); Geld entsteht erst am Ende
//! aus den Positionsmengen.

use crate::befund::{Befund, Ort};
use crate::geld::{runden, Cent, Dez};
use crate::katalog::{fnv, Bezug, Einheit, Katalog, Leistung, FNV_START};
use crate::zuordnung::{self, dicke, Grund, Zuordnung};
use sk_model::element::Category;
use sk_model::library::Material;
use sk_model::qto::{FormworkQto, Schedule, Umfang};
use sk_model::{BuildingId, ElementId, Guid, MaterialLayer, Model, StoreyId};
use std::collections::HashMap;

/// Kleinste Einheit je Einheit (mm, mm², mm³, g): so viele je Einheit.
pub(crate) fn skala(e: Einheit) -> i128 {
    match e {
        Einheit::M2 => 1_000_000,
        Einheit::M3 => 1_000_000_000,
        Einheit::M => 1_000,
        Einheit::T => 1_000_000,
        Einheit::Kg => 1_000,
        Einheit::St => 1,
    }
}

/// Menge in kleinster Einheit → Menge auf 3 Stellen (Regel 83).
pub(crate) fn drei(menge: i128, e: Einheit) -> Dez {
    let milli = runden(menge * 1000, skala(e));
    Dez((milli * 1000) as i64)
}

/// GP = Menge (3 Stellen) × EP, auf den Cent.
pub(crate) fn gp(menge: Dez, ep: Cent) -> Cent {
    Cent(runden(menge.0 as i128 * ep.0 as i128, Dez::SKALA as i128) as i64)
}

/// `f64` aus der Mengenliste (mm, mm², mm³) auf eine ganze kleinste Einheit,
/// kaufmännisch.
fn ganz(x: f64) -> i128 {
    x.round() as i128
}

/// Eine Zeile im Mengenansatz einer Position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ansatz {
    pub element: ElementId,
    /// Bauteilnummer, z. B. „AW-003“.
    pub nummer: String,
    pub geschoss: StoreyId,
    pub gebaeude: Option<BuildingId>,
    /// Kostengruppe: die der Bauleistung, sonst die des Bauteils
    /// (Folgepositionen: des auslösenden Bauteils, Regel 95).
    pub kg: Option<u16>,
    /// Menge in der kleinsten Einheit (mm, mm², mm³, g), ganzzahlig.
    pub menge: i128,
    /// Folgeposition: die auslösende Bauleistung („aus B30 Decke“).
    pub aus: Option<Guid>,
    /// Auflagertasche der Wandschicht (mm³), schon von `menge` abgezogen;
    /// nur bei Positionen nach Volumen, sonst 0 (Zeile „− Auflager“ im
    /// Mengenansatz des LV, `LayerRow.pocket`).
    pub auflager: i128,
}

/// Was eine Position rechnet.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Quelle {
    /// Bauleistung (genau, gewählt oder nach Regel).
    Leistung(Guid),
    /// Bauleistung, geschätzt nach der nächsten Dicke (Stufe 3).
    Geschaetzt(Guid),
    /// Richtpreis des Baustoffs, nur Material (Stufe 4).
    Richtpreis(Guid),
}

/// Eine Kostenposition im Umfang.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub quelle: Quelle,
    /// Kurztext, bei K12 mit „· d=…mm“, geschätzt mit Vermerk.
    pub kurz: String,
    /// OZ-Vorstufe Titel + `pos` („02.0010“); leer beim Richtpreis.
    pub oz: String,
    pub einheit: Einheit,
    /// Menge auf 3 Stellen.
    pub menge: Dez,
    pub lohn: Cent,
    pub stoff: Cent,
    pub geraet: Cent,
    pub sonst: Cent,
    /// Nachunternehmerpreis; dann EP = NU, Lohn und Stoff 0.
    pub nu: Option<Cent>,
    pub ep: Cent,
    pub gp: Cent,
    /// „nur Material“ = Menge × Stoff-EP; 0 bei NU.
    pub stoff_gp: Cent,
    /// Ein Artikel ohne Preis oder gar kein Preis (Fall 8).
    pub preis_fehlt: bool,
    /// Gewerk (Bauleistung; Richtpreis: Gewerk der Schicht).
    pub gewerk: Option<Guid>,
    pub ansatz: Vec<Ansatz>,
    /// Baustoff und Dicke der Schicht, an der die Bauleistung rechnet
    /// (Artikel der Schicht, geschätzt nach Dicke); Preisblatt, KA-2c.
    pub schicht: Option<(Guid, Dez)>,
}

impl Position {
    /// Teilmengen auf 3 Stellen je Schlüssel (Regel 96, etwa je Geschoss oder
    /// Kostengruppe), in der Reihenfolge des ersten Vorkommens im Ansatz.
    pub fn teile<K: PartialEq + Copy>(&self, schluessel: impl Fn(&Ansatz) -> K) -> Vec<(K, Dez)> {
        let mut t: Vec<(K, i128)> = Vec::new();
        for a in &self.ansatz {
            let k = schluessel(a);
            match t.iter_mut().find(|(x, _)| *x == k) {
                Some((_, v)) => *v += a.menge,
                None => t.push((k, a.menge)),
            }
        }
        t.into_iter()
            .map(|(k, v)| (k, drei(v, self.einheit)))
            .collect()
    }

    /// GP einer (Teil-)Menge auf 3 Stellen: Menge × EP, mit `nur_material`
    /// × Stoff-EP, auf den Cent. Für die ganze Menge gleich `gp` bzw.
    /// `stoff_gp`.
    pub fn gp_von(&self, menge: Dez, nur_material: bool) -> Cent {
        gp(menge, if nur_material { self.stoff } else { self.ep })
    }
}

/// Eine Mengenzeile ohne Bauleistung, grau und nie still weggelassen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OhneZeile {
    pub element: ElementId,
    pub nummer: String,
    pub geschoss: StoreyId,
    pub gebaeude: Option<BuildingId>,
    /// Gewerk und Kostengruppe der Schicht (die Zeile steht am Ende ihres
    /// Gewerks bzw. ihrer KG, ka-2-fach §2.1).
    pub gewerk: Option<Guid>,
    pub kg: Option<u16>,
    /// Typ des Bauteils (für „Bauleistung wählen“ an der Schicht).
    pub typ: Option<Guid>,
    pub schicht: usize,
    pub baustoff: Guid,
    pub dicke: Dez,
    pub einheit: Einheit,
    pub menge: Dez,
    /// Mengenbezüge, die das Bauteil hat (Regel 80, „Bauleistung wählen“).
    pub bezuege: Vec<Bezug>,
}

/// Das Kostenblatt eines Umfangs (Bausteingrenze §5, ka-0-fach §1.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kostenblatt {
    /// Quelle der Stammdaten, z. B. „Werkspreise 10/2026“.
    pub quelle: String,
    pub positionen: Vec<Position>,
    pub ohne: Vec<OhneZeile>,
    /// Summe der Positions-GP (in jeder Gliederung gleich).
    pub netto: Cent,
    pub mwst: Cent,
    pub brutto: Cent,
    /// MwSt.-Satz in Prozent (Firmenwert `vat`).
    pub mwst_satz: Dez,
    /// MwSt. auf „nur Material“ (Modus Material, ka-2-fach §2.3).
    pub mwst_material: Cent,
    pub lohn: Cent,
    pub stoff: Cent,
    pub geraet: Cent,
    pub sonst: Cent,
    /// Summe der NU-Positionen.
    pub nu: Cent,
    /// „nur Material“: Summe der Stoff-GP ohne NU.
    pub nur_material: Cent,
    /// Positionen mit „Preis fehlt“ („unvollständig: n“).
    pub unvollstaendig: usize,
    /// „davon geschätzt: n Zeilen, x €“.
    pub geschaetzt: usize,
    pub geschaetzt_betrag: Cent,
    pub nach_gewerk: Vec<(Option<Guid>, Cent)>,
    /// Teil-GP je Kostengruppe; `ausgleich_kg` = Netto − Σ (Regel 96).
    pub nach_kg: Vec<(Option<u16>, Cent)>,
    pub ausgleich_kg: Cent,
    /// Teil-GP je Geschoss; `ausgleich_geschoss` = Netto − Σ.
    pub nach_geschoss: Vec<(StoreyId, Cent)>,
    pub ausgleich_geschoss: Cent,
    pub befunde: Vec<Befund>,
}

/// Zwischenspeicher der Zuordnung (Bausteingrenze §5, Stufe 1): ein Wert des
/// Aufrufers, geht in `kosten_mit` hinein und kommt heraus.
#[derive(Clone, Debug, Default)]
pub struct Kostenspeicher {
    stempel: u64,
    typen: HashMap<Schluessel, Vec<SchichtWert>>,
    neu: u64,
}

impl Kostenspeicher {
    /// Wie viele (Bauteilart, Typ, Schichtfolge) der letzte Aufruf neu
    /// zugeordnet hat.
    pub fn neu_zugeordnet(&self) -> u64 {
        self.neu
    }
}

/// Bauteilart, Typ und Fingerabdruck der Schichten samt der Baustoffwerte,
/// von denen die Zuordnung abhängt (Art, Rohdichte, Richtpreis), und der
/// Namen, die in den Befundsätzen stehen (Typ, Baustoffe).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Schluessel {
    kat: Category,
    typ: Option<Guid>,
    schichten: u64,
}

/// Was je Schicht feststeht, solange Katalog und Schichten gleich sind.
#[derive(Clone, Debug)]
struct SchichtWert {
    zuordnung: Zuordnung,
    dicke: Dez,
    baustoff: Guid,
    /// Stoff-EP (vor NU) und „Preis fehlt“ der Bauleistung an dieser Schicht.
    stoff: Cent,
    preis_fehlt: bool,
    /// Richtpreis-EP (nur Material) bei Stufe 4.
    richtpreis: Option<(Cent, Einheit)>,
    /// Folgen: Bauleistung, Faktor, Stoff-EP, Preis fehlt.
    folgen: Vec<(Guid, Dez, Cent, bool)>,
    befunde: Vec<Befund>,
}

/// Fingerabdruck der Schichten (FNV-1a über die Werte, die die Zuordnung
/// lesen kann, und die Namen in ihren Befundsätzen).
fn fingerabdruck(m: &Model, typname: &str, layers: &[MaterialLayer]) -> u64 {
    let mut h = fnv(fnv(FNV_START, typname.as_bytes()), b"|");
    for l in layers {
        let mat = m.material(l.material);
        h = fnv(h, &mat.map_or([0; 16], |x| x.guid.0.to_le_bytes()));
        h = fnv(h, &l.thickness.to_bits().to_le_bytes());
        h = fnv(h, sk_model::szo::layer_function(l.function).as_bytes());
        h = fnv(h, &l.svc.map_or([0; 16], |g| g.0.to_le_bytes()));
        if let Some(x) = mat {
            h = fnv(fnv(h, x.name.as_bytes()), b"|");
            h = fnv(h, &[x.category as u8]);
            h = fnv(h, &x.density.to_bits().to_le_bytes());
            if let Some((p, e)) = zuordnung::richtpreis(x) {
                h = fnv(h, &p.0.to_le_bytes());
                h = fnv(h, e.wort().as_bytes());
            }
        }
        h = fnv(h, b"|");
    }
    h
}

/// Name des Baustoffs.
pub(crate) fn baustoff_name(m: &Model, l: &MaterialLayer) -> String {
    m.material(l.material)
        .map_or(crate::wort::EIN_EINTRAG.into(), |x| x.name.clone())
}

/// Dicke für Befundsätze: „17,5 mm“.
pub(crate) fn mm_text(t: Dez) -> String {
    format!("{} mm", t.text().replace('.', ","))
}

/// Dicke im Kurztext (K12): „· d=120mm“.
fn mm_kurz(t: Dez) -> String {
    format!("{}mm", t.text().replace('.', ","))
}

/// Dicke im Vermerk „geschätzt für d=20cm“ (ka-0-fach §1.3).
fn cm_kurz(t: Dez) -> String {
    format!("{}cm", Dez(t.0 / 10).text().replace('.', ","))
}

/// Ein Stoffanteil im EP einer Bauleistung (Preisblatt, KA-2c): Artikel,
/// Menge je Einheit der Bauleistung, Preis je Artikeleinheit und Betrag vor
/// dem Zuschlag in 10⁻¹² € (nach Umrechnung der Einheit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stoffteil {
    /// Artikel; `None` beim Richtpreis des Baustoffs.
    pub artikel: Option<Guid>,
    pub name: String,
    pub menge: Dez,
    pub einheit: Einheit,
    pub preis: Dez,
    pub wert: i128,
    /// Hauptstoff: Artikel der Schicht oder Artikel aus dem Baustoff der
    /// Bauleistung (Planstein); sonst Nebenstoff (Dünnbettmörtel), den das
    /// Preisblatt nur als Zeile zeigt.
    pub haupt: bool,
}

/// Stoff-EP einer Bauleistung an einer Schicht (Regeln 82, 83): Summe der
/// Stoffanteile in 10⁻¹² €, mit Zuschlag auf den Cent. `faktor`: Umrechnung
/// bei „geschätzt nach“ (Schichtdicke, nur m² und Artikel mit Dicke).
#[allow(clippy::too_many_arguments)]
pub(crate) fn stoff_ep(
    k: &Katalog,
    l: &Leistung,
    schicht: Option<(Dez, Option<&Material>)>,
    geschaetzt: bool,
    ort: &Ort,
    befunde: &mut Vec<Befund>,
) -> (Cent, bool) {
    let (teile, fehlt) = stoff_teile(k, l, schicht, geschaetzt, ort, befunde);
    let summe: i128 = teile.iter().map(|t| t.wert).sum();
    (mit_zuschlag(k, summe), fehlt)
}

/// Summe der Stoffanteile (10⁻¹² €) mit Zuschlag, auf den Cent.
pub(crate) fn mit_zuschlag(k: &Katalog, summe: i128) -> Cent {
    let z = k.werte.zuschlag.0 as i128;
    Cent(runden(
        summe * (100 * Dez::SKALA as i128 + z),
        100 * (Dez::SKALA as i128) * 10_000_000_000,
    ) as i64)
}

/// Die Stoffanteile hinter [`stoff_ep`], je Anteil ein [`Stoffteil`];
/// `true`, wenn ein Preis fehlt.
pub(crate) fn stoff_teile(
    k: &Katalog,
    l: &Leistung,
    schicht: Option<(Dez, Option<&Material>)>,
    geschaetzt: bool,
    ort: &Ort,
    befunde: &mut Vec<Befund>,
) -> (Vec<Stoffteil>, bool) {
    let mut teile = Vec::new();
    let mut fehlt = false;
    let t = schicht.map(|(t, _)| t);
    for a in k.anteile_von(l.guid) {
        let q = a.menge.0 as i128;
        match a.artikel {
            Some(g) => {
                let Some(art) = k.artikel(g) else {
                    fehlt = true;
                    continue;
                };
                let Some(p) = art.preis else {
                    fehlt = true;
                    befunde.push(Befund::warnung(
                        82,
                        format!("{}: Für {} gibt es keinen Preis.", l.kurz, art.name),
                        ort.clone(),
                    ));
                    continue;
                };
                let mut v = q * p.0 as i128;
                // geschätzt nach: Stoff je m² nach Dicke (Regel 81 Stufe 3)
                if geschaetzt && l.einheit == Einheit::M2 {
                    if let (Some(at), Some(lt)) = (art.t.filter(|d| d.0 > 0), t) {
                        v = runden(v * lt.0 as i128, at.0 as i128);
                    }
                }
                teile.push(Stoffteil {
                    artikel: Some(g),
                    name: art.name.clone(),
                    menge: a.menge,
                    einheit: art.einheit,
                    preis: p,
                    wert: v,
                    haupt: art.mat.is_some() && art.mat == l.mat,
                });
            }
            None => {
                // Artikel der Schicht (Regel 82)
                let Some((t, mat)) = schicht else {
                    fehlt = true;
                    continue;
                };
                let mg = mat.map(|x| x.guid);
                let gefunden = mg.and_then(|g| zuordnung::artikel_der_schicht(k, g, t));
                let (preis, einheit, artikel, name) = match gefunden {
                    Some(art) => match art.preis {
                        Some(p) => (p, art.einheit, Some(art.guid), art.name.clone()),
                        None => {
                            fehlt = true;
                            befunde.push(Befund::warnung(
                                82,
                                format!("{}: Für {} gibt es keinen Preis.", l.kurz, art.name),
                                ort.clone(),
                            ));
                            continue;
                        }
                    },
                    None => match mat.and_then(zuordnung::richtpreis) {
                        Some((p, e)) => {
                            let name = mat.map_or(crate::wort::EIN_EINTRAG, |x| x.name.as_str());
                            befunde.push(Befund::hinweis(
                                78,
                                format!(
                                    "{name} hat keinen Artikel; gerechnet wird mit dem Richtpreis {} €/{}.",
                                    p.cent().deutsch(),
                                    e.zeichen()
                                ),
                                ort.clone(),
                            ));
                            (p, e, None, name.to_string())
                        }
                        None => {
                            fehlt = true;
                            let name = mat.map_or(crate::wort::EIN_EINTRAG, |x| x.name.as_str());
                            befunde.push(Befund::warnung(
                                82,
                                format!(
                                    "{}: Für {name} {} gibt es keinen Preis.",
                                    l.kurz,
                                    mm_text(t)
                                ),
                                ort.clone(),
                            ));
                            continue;
                        }
                    },
                };
                // Umrechnung je Einheit der Bauleistung
                let v = q * preis.0 as i128;
                let v = match (einheit, l.einheit) {
                    (a, b) if a == b => v,
                    // m³-Artikel auf m²: Dicke/1000
                    (Einheit::M3, Einheit::M2) => runden(v * t.0 as i128, 1_000_000_000),
                    // t-Artikel auf m³: Rohdichte/1000
                    (Einheit::T, Einheit::M3) => {
                        let rho = mat.map_or(0, |x| (x.density * 1000.0).round() as i128);
                        runden(v * rho, 1_000_000)
                    }
                    (a, _) => {
                        befunde.push(Befund::warnung(
                            82,
                            format!(
                                "{}: Einheit des Artikels {} passt nicht.",
                                l.kurz,
                                a.zeichen()
                            ),
                            ort.clone(),
                        ));
                        0
                    }
                };
                teile.push(Stoffteil {
                    artikel,
                    name,
                    menge: a.menge,
                    einheit,
                    preis,
                    wert: v,
                    haupt: true,
                });
            }
        }
    }
    // ohne jeden Anteil, Zeitansatz, Gerät, Sonstiges oder NU: Preis fehlt
    let leer = l.nu.is_none()
        && l.stunden == Dez::NULL
        && l.geraet == Dez::NULL
        && l.sonst == Dez::NULL
        && k.anteile_von(l.guid).next().is_none();
    (teile, fehlt || leer)
}

/// Ordnet alle Schichten eines Typs zu und rechnet die Stoff-EP.
fn schichten_rechnen(
    m: &Model,
    k: &Katalog,
    kat: Category,
    typ: Option<Guid>,
    typname: &str,
    layers: &[MaterialLayer],
) -> Vec<SchichtWert> {
    layers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let mat = m.material(s.material);
            let z = zuordnung::zuordnen(k, kat, s, mat);
            let t = dicke(s.thickness);
            let ort = match typ {
                Some(typ) => Ort::Schicht { typ, schicht: i },
                None => Ort::Datei,
            };
            let name = baustoff_name(m, s);
            let mut befunde = Vec::new();
            let leistung = z.leistung.and_then(|g| k.leistung(g));
            let kurz = leistung.map_or(String::new(), |l| l.kurz.clone());
            let dick = mm_text(t);
            match &z.grund {
                Grund::Mehrdeutig { .. } => befunde.push(Befund::warnung(
                    81,
                    format!("{typname}, Schicht {name} {dick}: mehrere Bauleistungen passen; gewählt ist {kurz}."),
                    ort.clone(),
                )),
                Grund::Geschaetzt { .. } => befunde.push(Befund::warnung(
                    81,
                    format!("{typname}, Schicht {name} {dick}: geschätzt nach {kurz}"),
                    ort.clone(),
                )),
                Grund::Richtpreis => befunde.push(Befund::warnung(
                    81,
                    format!("{typname}, Schicht {name} {dick}: nur Material nach Richtpreis"),
                    ort.clone(),
                )),
                Grund::Ohne if mat.is_some_and(|x| x.category != sk_model::library::MatCategory::Air) => {
                    befunde.push(Befund::hinweis(
                        81,
                        format!("{typname}, Schicht {name} {dick}: keine Bauleistung gefunden."),
                        ort.clone(),
                    ))
                }
                _ => {}
            }
            let (stoff, preis_fehlt) = match leistung {
                Some(l) => stoff_ep(k, l, Some((t, mat)), z.geschaetzt(), &ort, &mut befunde),
                None => (Cent::NULL, false),
            };
            if let Some(l) = leistung.filter(|l| l.nu.is_some()) {
                if l.stunden != Dez::NULL || k.anteile_von(l.guid).next().is_some() {
                    befunde.push(Befund::hinweis(
                        83,
                        format!("{}: Nachunternehmerpreis gesetzt, Lohn- und Stoffanteile werden nicht gerechnet.", l.kurz),
                        ort.clone(),
                    ));
                }
            }
            let richtpreis = match z.grund {
                Grund::Richtpreis => mat
                    .and_then(zuordnung::richtpreis)
                    .map(|(p, e)| (p.cent(), e)),
                _ => None,
            };
            let folgen = leistung
                .filter(|_| !z.geschaetzt() || matches!(z.grund, Grund::Geschaetzt { .. }))
                .map(|l| {
                    k.folgen_von(l.guid)
                        .filter_map(|f| {
                            let fl = k.leistung(f.folge)?;
                            let (st, pf) = stoff_ep(k, fl, Some((t, mat)), false, &ort, &mut befunde);
                            Some((f.folge, f.faktor, st, pf))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            SchichtWert {
                zuordnung: z,
                dicke: t,
                baustoff: mat.map_or(Guid(0), |x| x.guid),
                stoff,
                preis_fehlt,
                richtpreis,
                folgen,
                befunde,
            }
        })
        .collect()
}

/// Abdeckung (Regel 97): Jede Schicht eines Werkstyps im Modell hat genau
/// eine Bauleistung über Stufe 1 oder 2 und einen Preis. Ausnahmen: Luft
/// und die Dachterrasse (keine Werksbauleistung vorgesehen).
pub(crate) fn abdeckung(m: &Model, k: &Katalog) -> Vec<Befund> {
    use sk_model::library::{MatCategory, TypeCategory};
    let werk = Model::new();
    let mut out = Vec::new();
    for (_, t) in m.layer_sets().iter() {
        if t.category == TypeCategory::RoofTerrace
            || !werk.layer_sets().iter().any(|(_, w)| w.guid == t.guid)
        {
            continue;
        }
        let Some(kat) = Category::ALL
            .into_iter()
            .find(|c| TypeCategory::of(*c) == Some(t.category))
        else {
            continue;
        };
        for (i, s) in t.layers.iter().enumerate() {
            let mat = m.material(s.material);
            if mat.is_none_or(|x| x.category == MatCategory::Air) {
                continue;
            }
            let ort = Ort::Schicht {
                typ: t.guid,
                schicht: i,
            };
            let z = zuordnung::zuordnen(k, kat, s, mat);
            let kopf = format!(
                "Werksschicht {} {} in {}",
                baustoff_name(m, s),
                mm_text(dicke(s.thickness)),
                t.name
            );
            let l = z.leistung.and_then(|g| k.leistung(g));
            let kurz = l.map_or(String::new(), |l| l.kurz.clone());
            match &z.grund {
                Grund::Gewaehlt | Grund::Regel => {}
                Grund::Mehrdeutig { .. } => out.push(Befund::warnung(
                    97,
                    format!(
                        "{kopf} hat keine genaue Bauleistung (mehrere passen; gewählt ist {kurz})."
                    ),
                    ort.clone(),
                )),
                Grund::Geschaetzt { .. } => out.push(Befund::warnung(
                    97,
                    format!("{kopf} hat keine genaue Bauleistung (geschätzt nach {kurz})."),
                    ort.clone(),
                )),
                Grund::Richtpreis | Grund::Ohne => out.push(Befund::warnung(
                    97,
                    format!("{kopf} hat keine Bauleistung."),
                    ort.clone(),
                )),
            }
            // Preis über Artikel oder NU
            if let Some(l) = l.filter(|_| !z.geschaetzt()) {
                let mut bf = Vec::new();
                let (_, fehlt) =
                    stoff_ep(k, l, Some((dicke(s.thickness), mat)), false, &ort, &mut bf);
                if fehlt {
                    if bf.is_empty() {
                        bf.push(Befund::warnung(
                            82,
                            format!(
                                "{}: Für {} {} gibt es keinen Preis.",
                                l.kurz,
                                baustoff_name(m, s),
                                mm_text(dicke(s.thickness))
                            ),
                            ort.clone(),
                        ));
                    }
                    for b in bf {
                        if !out.contains(&b) {
                            out.push(b);
                        }
                    }
                }
            }
        }
    }
    out
}

/// Lohn-, Gerät- und Sonstiges-EP und NU einer Bauleistung (Regel 83).
pub(crate) fn feste_ep(k: &Katalog, l: &Leistung) -> (Cent, Cent, Cent, Option<Cent>) {
    let lohn = runden(
        l.stunden.0 as i128 * k.werte.lohn.0 as i128,
        Dez::SKALA as i128 * 10_000,
    );
    (
        Cent(lohn as i64),
        l.geraet.cent(),
        l.sonst.cent(),
        l.nu.map(Dez::cent),
    )
}

/// Schlüssel einer Position: Quelle und Stoff-EP (K12).
type PosKey = (Quelle, Cent, Option<Guid>);

/// Gesammelte Position vor dem Geld.
struct Sammel {
    quelle: Quelle,
    stoff: Cent,
    preis_fehlt: bool,
    einheit: Einheit,
    gewerk: Option<Guid>,
    dicken: Vec<Dez>,
    ansatz: Vec<Ansatz>,
    schicht: Option<(Guid, Dez)>,
}

/// Wie [`kosten`], mit Zwischenspeicher: dasselbe Blatt auf den Cent.
pub fn kosten_mit(
    sp: Kostenspeicher,
    m: &Model,
    sched: &Schedule,
    k: &Katalog,
    u: &Umfang,
) -> (Kostenblatt, Kostenspeicher) {
    let eingeschraenkt;
    let s = if u.alles() {
        sched
    } else {
        eingeschraenkt = sched.restrict(m, u);
        &eingeschraenkt
    };
    let alt = if sp.stempel == k.stempel {
        sp.typen
    } else {
        HashMap::new()
    };
    let mut alt = alt;
    let mut typen: HashMap<Schluessel, Vec<SchichtWert>> = HashMap::new();
    let mut neu = 0u64;
    let schalung: HashMap<ElementId, &FormworkQto> =
        s.formwork.iter().map(|(e, f)| (*e, f)).collect();
    let mut je_element: HashMap<ElementId, Schluessel> = HashMap::new();
    let mut sammel: Vec<Sammel> = Vec::new();
    let mut index: HashMap<PosKey, usize> = HashMap::new();
    let mut ohne: Vec<OhneZeile> = Vec::new();
    let mut befunde: Vec<Befund> = Vec::new();
    let mut lose_gemeldet: Vec<ElementId> = Vec::new();
    // Bauteilmengen (Umfang, Schalung) je (Bauteil, Folge) nur einmal
    let mut einmal: Vec<(ElementId, Guid)> = Vec::new();
    let mut ep_fest: HashMap<Guid, (Cent, Cent, Cent, Option<Cent>)> = HashMap::new();

    let rows = s.layer_rows(m);
    for (gebaeude, r) in &rows {
        let Some(e) = m.element(r.element) else {
            continue;
        };
        let typname = || {
            e.layer_set.and_then(|t| m.layer_set(t)).map_or_else(
                || sk_model::kinds::spec(r.category).name.to_string(),
                |t| t.name.clone(),
            )
        };
        let key = *je_element.entry(r.element).or_insert_with(|| {
            let layers = m.element_layers(r.element);
            Schluessel {
                kat: r.category,
                typ: e.layer_set.and_then(|t| m.layer_set(t)).map(|t| t.guid),
                schichten: fingerabdruck(m, &typname(), &layers),
            }
        });
        let werte = typen.entry(key).or_insert_with(|| match alt.remove(&key) {
            Some(w) => w,
            None => {
                neu += 1;
                let layers = m.element_layers(r.element);
                schichten_rechnen(m, k, r.category, key.typ, &typname(), &layers)
            }
        });
        let Some(w) = werte.get(r.layer) else {
            continue;
        };
        if gebaeude.is_none() && !lose_gemeldet.contains(&r.element) {
            lose_gemeldet.push(r.element);
            befunde.push(Befund::hinweis(
                95,
                format!(
                    "{} gehört zu keinem Gebäude und zählt nur im Umfang Projekt.",
                    r.number
                ),
                Ort::Bauteil(e.guid),
            ));
        }
        let ansatz = |menge: i128| Ansatz {
            element: r.element,
            nummer: r.number.clone(),
            geschoss: r.storey,
            gebaeude: *gebaeude,
            kg: r.kg,
            menge,
            aus: None,
            auflager: 0,
        };
        let gewerk_schicht = r.trade.and_then(|t| m.trade(t)).map(|t| t.guid);
        let mut legen = |key: PosKey,
                         einheit: Einheit,
                         gewerk: Option<Guid>,
                         preis_fehlt: bool,
                         a: Ansatz,
                         dicke: Dez| {
            let i = *index.entry(key.clone()).or_insert_with(|| {
                sammel.push(Sammel {
                    quelle: key.0.clone(),
                    stoff: key.1,
                    preis_fehlt: false,
                    einheit,
                    gewerk,
                    dicken: Vec::new(),
                    ansatz: Vec::new(),
                    schicht: (!matches!(key.0, Quelle::Richtpreis(_)))
                        .then_some((w.baustoff, w.dicke)),
                });
                sammel.len() - 1
            });
            let p = &mut sammel[i];
            p.preis_fehlt |= preis_fehlt;
            if !p.dicken.contains(&dicke) {
                p.dicken.push(dicke);
            }
            p.ansatz.push(a);
        };
        // Menge der Schicht in kleinster Einheit
        let menge_von = |b: Bezug, folge: bool| -> Option<i128> {
            Some(match b {
                Bezug::Flaeche => ganz(r.face),
                Bezug::Volumen => ganz(r.volume),
                Bezug::Laenge if folge => ganz(r.bill_length),
                Bezug::Laenge => ganz(r.length),
                Bezug::Umfang => ganz(schalung.get(&r.element)?.edge),
                Bezug::Schalung => ganz(schalung.get(&r.element)?.soffit),
                Bezug::Stahl => {
                    let grad = k.werte.stahl(zuordnung::kategorie_wort(r.category))?;
                    // mm³ × kg/m³ → g: Volumen × Grad ÷ 10⁶ (Grad als Festkomma)
                    runden(ganz(r.volume) * grad.0 as i128, 1_000_000_000_000)
                }
            })
        };
        match (
            &w.zuordnung.grund,
            w.zuordnung.leistung.and_then(|g| k.leistung(g)),
        ) {
            (Grund::Richtpreis, _) => {
                let Some((p, e)) = w.richtpreis else { continue };
                let menge = match e {
                    Einheit::M3 => ganz(r.volume),
                    Einheit::M2 => ganz(r.face),
                    Einheit::M => ganz(if r.length > 0.0 {
                        r.length
                    } else {
                        r.bill_length
                    }),
                    Einheit::T => {
                        let rho = m.material(r.material).map_or(0.0, |x| x.density);
                        // mm³ × kg/m³ → g
                        ganz(r.volume * rho / 1e6)
                    }
                    _ => 0,
                };
                legen(
                    (Quelle::Richtpreis(w.baustoff), p, gewerk_schicht),
                    e,
                    gewerk_schicht,
                    false,
                    ansatz(menge),
                    w.dicke,
                );
            }
            (_, Some(l)) => {
                let quelle = match w.zuordnung.grund {
                    Grund::Geschaetzt { .. } => Quelle::Geschaetzt(l.guid),
                    _ => Quelle::Leistung(l.guid),
                };
                let menge = menge_von(l.bezug, false).unwrap_or(0);
                let wand = matches!(r.category, Category::ExteriorWall | Category::InteriorWall);
                let auflager = if l.bezug == Bezug::Volumen && wand {
                    ganz(r.pocket)
                } else {
                    0
                };
                legen(
                    (quelle, w.stoff, None),
                    l.einheit,
                    Some(l.gewerk),
                    w.preis_fehlt,
                    // KG der Bauleistung, sonst die des Bauteils (wie im LV)
                    Ansatz {
                        auflager,
                        kg: l.kg.or(r.kg),
                        ..ansatz(menge)
                    },
                    w.dicke,
                );
                // Folgepositionen (Regel 85), Ort und KG der Quelle
                for (fg, faktor, stoff, pf) in &w.folgen {
                    let Some(fl) = k.leistung(*fg) else { continue };
                    let bauteilmenge = matches!(fl.bezug, Bezug::Umfang | Bezug::Schalung);
                    if bauteilmenge {
                        if einmal.contains(&(r.element, *fg)) {
                            continue;
                        }
                        einmal.push((r.element, *fg));
                    }
                    let Some(menge) = menge_von(fl.bezug, true) else {
                        befunde.push(Befund::warnung(
                            85,
                            format!(
                                "{}: Folgeposition {} hat an {} keine Menge.",
                                l.kurz, fl.kurz, r.number
                            ),
                            Ort::Bauteil(e.guid),
                        ));
                        continue;
                    };
                    let menge = runden(menge * faktor.0 as i128, Dez::SKALA as i128);
                    // 0 ergibt keine Menge und keine Position (VK-01)
                    if menge == 0 {
                        continue;
                    }
                    legen(
                        (Quelle::Leistung(*fg), *stoff, None),
                        fl.einheit,
                        Some(fl.gewerk),
                        *pf,
                        Ansatz {
                            aus: Some(l.guid),
                            kg: fl.kg.or(r.kg),
                            ..ansatz(menge)
                        },
                        Dez::NULL,
                    );
                }
            }
            _ => {
                // ohne Bauleistung: grau mit Menge (Dachterrasse, Attikablech)
                let (einheit, menge) = if r.face > 0.0 {
                    (Einheit::M2, ganz(r.face))
                } else if r.bill_length > 0.0 {
                    (Einheit::M, ganz(r.bill_length))
                } else if r.length > 0.0 {
                    (Einheit::M, ganz(r.length))
                } else {
                    (Einheit::M3, ganz(r.volume))
                };
                ohne.push(OhneZeile {
                    element: r.element,
                    nummer: r.number.clone(),
                    geschoss: r.storey,
                    gebaeude: *gebaeude,
                    gewerk: gewerk_schicht,
                    kg: r.kg,
                    typ: key.typ,
                    schicht: r.layer,
                    baustoff: w.baustoff,
                    dicke: w.dicke,
                    einheit,
                    menge: drei(menge, einheit),
                    bezuege: [
                        (Bezug::Flaeche, r.face),
                        (Bezug::Volumen, r.volume),
                        (Bezug::Laenge, r.length.max(r.bill_length)),
                    ]
                    .into_iter()
                    .filter(|(_, v)| ganz(*v) > 0)
                    .map(|(b, _)| b)
                    .collect(),
                });
            }
        }
    }
    // Befunde der benutzten Typen, je einmal
    let mut schluessel: Vec<&Schluessel> = typen.keys().collect();
    schluessel.sort_by_key(|s| (s.typ, s.kat as u8, s.schichten));
    for s in schluessel {
        for w in &typen[s] {
            for b in &w.befunde {
                if !befunde.contains(b) {
                    befunde.push(b.clone());
                }
            }
        }
    }

    // Geld (Regel 83) je Position
    let mut positionen: Vec<Position> = Vec::new();
    // Zahl der Stoff-EP je genauer Bauleistung (K12: „· d=…mm“)
    let mut eps: HashMap<Guid, usize> = HashMap::new();
    for p in &sammel {
        if let Quelle::Leistung(g) = p.quelle {
            *eps.entry(g).or_default() += 1;
        }
    }
    for p in sammel {
        let menge_klein: i128 = p.ansatz.iter().map(|a| a.menge).sum();
        let menge = drei(menge_klein, p.einheit);
        let (kurz, oz, lohn, geraet, sonst, nu) = match &p.quelle {
            Quelle::Leistung(g) | Quelle::Geschaetzt(g) => {
                let l = k.leistung(*g).expect("Leistung");
                let (lo, ge, so, nu) = *ep_fest.entry(*g).or_insert_with(|| feste_ep(k, l));
                let mut kurz = l.kurz.clone();
                let d = p.dicken.iter().copied().filter(|d| d.0 > 0).max();
                match (&p.quelle, d) {
                    (Quelle::Geschaetzt(_), Some(d)) => {
                        kurz += &format!(" · geschätzt für d={}", cm_kurz(d));
                    }
                    (Quelle::Leistung(_), Some(d)) if eps[g] > 1 => {
                        kurz += &format!(" · d={}", mm_kurz(d));
                    }
                    _ => {}
                }
                // Geschätzte Zeilen stehen in keinem LV und haben keine OZ
                let oz = match p.quelle {
                    Quelle::Leistung(_) => k.oz_voll(l),
                    _ => String::new(),
                };
                (kurz, oz, lo, ge, so, nu)
            }
            Quelle::Richtpreis(b) => {
                let name = m
                    .materials()
                    .iter()
                    .find(|(_, x)| x.guid == *b)
                    .map_or(crate::wort::EIN_EINTRAG.to_string(), |(_, x)| {
                        x.name.clone()
                    });
                (
                    format!("{name} · geschätzt, nur Material"),
                    String::new(),
                    Cent::NULL,
                    Cent::NULL,
                    Cent::NULL,
                    None,
                )
            }
        };
        let (lohn, stoff, geraet, sonst, ep) = match nu {
            Some(n) => (Cent::NULL, Cent::NULL, Cent::NULL, Cent::NULL, n),
            None => (
                lohn,
                p.stoff,
                geraet,
                sonst,
                lohn + p.stoff + geraet + sonst,
            ),
        };
        let gp_wert = gp(menge, ep);
        let stoff_gp = if nu.is_some() {
            Cent::NULL
        } else {
            gp(menge, stoff)
        };
        positionen.push(Position {
            quelle: p.quelle,
            kurz,
            oz,
            einheit: p.einheit,
            menge,
            lohn,
            stoff,
            geraet,
            sonst,
            nu,
            ep,
            gp: gp_wert,
            stoff_gp,
            preis_fehlt: p.preis_fehlt,
            gewerk: p.gewerk,
            ansatz: p.ansatz,
            schicht: p.schicht,
        });
    }
    // Reihenfolge: Los, Titel, pos; geschätzt hinter genau; Richtpreis zuletzt
    let rang = |p: &Position| {
        let (los, titel, pos) = match &p.quelle {
            Quelle::Leistung(g) | Quelle::Geschaetzt(g) => {
                let l = k.leistung(*g).expect("Leistung");
                let t = k.los(l.titel);
                let los = t
                    .and_then(|t| t.parent)
                    .and_then(|g| k.los(g))
                    .map_or("", |x| x.nr.as_str());
                let tn = t.map_or("", |t| t.nr.as_str());
                (
                    (0, los.len(), los.to_string()),
                    (tn.len(), tn.to_string()),
                    l.pos,
                )
            }
            Quelle::Richtpreis(_) => ((1, 0, String::new()), (0, String::new()), 0),
        };
        let g = matches!(p.quelle, Quelle::Geschaetzt(_));
        (los, titel, pos, g, p.stoff, p.kurz.clone())
    };
    positionen.sort_by_cached_key(rang);

    let summe = |f: &dyn Fn(&Position) -> Cent| positionen.iter().map(f).sum::<Cent>();
    let netto = summe(&|p| p.gp);
    let mwst_von = |c: Cent| {
        Cent(runden(
            c.0 as i128 * k.werte.mwst.0 as i128,
            100 * Dez::SKALA as i128,
        ) as i64)
    };
    let mwst = mwst_von(netto);
    let teil = |p: &Position, f: Cent| gp(p.menge, f);
    let lohn = summe(&|p| teil(p, p.lohn));
    let stoff = summe(&|p| teil(p, p.stoff));
    let geraet = summe(&|p| teil(p, p.geraet));
    let sonst = summe(&|p| teil(p, p.sonst));
    let nu = summe(&|p| if p.nu.is_some() { p.gp } else { Cent::NULL });
    let nur_material = summe(&|p| p.stoff_gp);
    let geschaetzte: Vec<&Position> = positionen
        .iter()
        .filter(|p| !matches!(p.quelle, Quelle::Leistung(_)))
        .collect();
    let mut nach_gewerk: Vec<(Option<Guid>, Cent)> = Vec::new();
    for p in &positionen {
        match nach_gewerk.iter_mut().find(|(g, _)| *g == p.gewerk) {
            Some((_, c)) => *c += p.gp,
            None => nach_gewerk.push((p.gewerk, p.gp)),
        }
    }
    // Teilungen (Regel 96): Teilmenge auf 3 Stellen × EP, Ausgleich am Ende
    let geschoss_folge: Vec<StoreyId> = {
        let mut v: Vec<StoreyId> = Vec::new();
        for (_, r) in &rows {
            if !v.contains(&r.storey) {
                v.push(r.storey);
            }
        }
        v
    };
    let mut nach_geschoss: Vec<(StoreyId, Cent)> = Vec::new();
    let mut nach_kg: Vec<(Option<u16>, Cent)> = Vec::new();
    for p in &positionen {
        let je_geschoss: Vec<(StoreyId, Cent)> = p
            .teile(|a| a.geschoss)
            .into_iter()
            .map(|(st, menge)| (st, p.gp_von(menge, false)))
            .collect();
        let je_kg: Vec<(Option<u16>, Cent)> = p
            .teile(|a| a.kg)
            .into_iter()
            .map(|(kg, menge)| (kg, p.gp_von(menge, false)))
            .collect();
        // Schranke je Position und Teilung: Teile × (0,0005 × EP + 0,005) €
        befunde.extend(summenprobe("Geschosse", &p.kurz, &je_geschoss, p.ep, p.gp));
        befunde.extend(summenprobe("Kostengruppen", &p.kurz, &je_kg, p.ep, p.gp));
        for (st, c) in je_geschoss {
            match nach_geschoss.iter_mut().find(|(s, _)| *s == st) {
                Some((_, v)) => *v += c,
                None => nach_geschoss.push((st, c)),
            }
        }
        for (kg, c) in je_kg {
            match nach_kg.iter_mut().find(|(x, _)| *x == kg) {
                Some((_, v)) => *v += c,
                None => nach_kg.push((kg, c)),
            }
        }
    }
    // Geschosse von unten nach oben
    let hoehe = |s: &StoreyId| {
        m.storey(*s)
            .map_or(0, |x| (x.elevation * 1000.0).round() as i64)
    };
    nach_geschoss.sort_by_key(|(s, _)| (hoehe(s), geschoss_folge.iter().position(|x| x == s)));
    nach_kg.sort_by_key(|(k, _)| k.unwrap_or(u16::MAX));
    let ausgleich_geschoss = netto - nach_geschoss.iter().map(|x| x.1).sum::<Cent>();
    let ausgleich_kg = netto - nach_kg.iter().map(|x| x.1).sum::<Cent>();
    let blatt = Kostenblatt {
        quelle: k.quelle.text(),
        unvollstaendig: positionen.iter().filter(|p| p.preis_fehlt).count(),
        geschaetzt: geschaetzte.len(),
        geschaetzt_betrag: geschaetzte.iter().map(|p| p.gp).sum(),
        positionen,
        ohne,
        netto,
        mwst,
        brutto: netto + mwst,
        mwst_satz: k.werte.mwst,
        mwst_material: mwst_von(nur_material),
        lohn,
        stoff,
        geraet,
        sonst,
        nu,
        nur_material,
        nach_gewerk,
        nach_kg,
        ausgleich_kg,
        nach_geschoss,
        ausgleich_geschoss,
        befunde,
    };
    (
        blatt,
        Kostenspeicher {
            stempel: k.stempel,
            typen,
            neu,
        },
    )
}

/// Das Kostenblatt im Umfang `u` (Regel 98: liest nur).
pub fn kosten(m: &Model, sched: &Schedule, k: &Katalog, u: &Umfang) -> Kostenblatt {
    kosten_mit(Kostenspeicher::default(), m, sched, k, u).0
}

/// Regel 96 je Position und Teilung: |Σ Teil-GP − GP| ≤ Teile × (0,0005 ×
/// EP + 0,005) €. `teile` sind (Schlüssel, Teil-GP).
fn summenprobe<K>(
    art: &str,
    kurz: &str,
    teile: &[(K, Cent)],
    ep: Cent,
    gp: Cent,
) -> Option<Befund> {
    let summe: Cent = teile.iter().map(|t| t.1).sum();
    let schranke = teile.len() as i128 * (ep.0 as i128 * 5 + 5_000);
    (((summe - gp).0 as i128).abs() * 10_000 > schranke).then(|| {
        Befund::warnung(
            96,
            format!(
                "Summenprobe {art}, {kurz}: Teile ergeben {summe}, das Ganze {gp}; erlaubt sind {} €.",
                Cent((schranke / 10_000) as i64).deutsch()
            ),
            Ort::Datei,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regel 96 gilt für jede Teilung, auch nach Kostengruppen (Prüfung
    /// BIM-Integration 10:40): zwei Teile, EP 100,00 € → Schranke 0,11 €.
    #[test]
    fn summenprobe_je_teilung() {
        let ep = Cent(10_000);
        let teile = [(1, Cent(5_000)), (2, Cent(5_011))];
        assert!(summenprobe("Kostengruppen", "x", &teile, ep, Cent(10_000)).is_none());
        let teile = [(1, Cent(5_000)), (2, Cent(5_012))];
        let b = summenprobe("Kostengruppen", "x", &teile, ep, Cent(10_000)).unwrap();
        assert_eq!(b.regel, 96);
        assert!(
            b.satz.starts_with("Summenprobe Kostengruppen, x:"),
            "{}",
            b.satz
        );
    }
}

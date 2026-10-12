//! Ausbau eines Raums (Raumbuch RB3, `raumbuch/plan.md` §5): welcher
//! Ausbautyp an welcher Fläche liegt, die wirksamen Schichtdicken (die
//! Dämmung unter dem Estrich gleicht den Belag aus) und was der Grundriss
//! davon braucht (`grundriss/schnittstelle-raum.md` §6.3): Ausbaudicke je
//! Kante in Fußbodenhöhe, Putzdicke je Laibung, OK Fertigfußboden und UK
//! Fertigdecke.

use crate::erkennung::{Kante, RaumForm};
use crate::oeffnungen::{OpeningKind, Rohbau};
use sk_model::model::{
    DEFAULT_SPACE_GUID, SCREED_FLOOR_TYPE_GUID, SCREED_SLAB_TYPE_GUID, SPACE_TEMPLATES,
};
use sk_model::{ElementId, Guid, LayerFunction, LayerSetId, LevelKind, Model, SpaceType, StoreyId};
use std::borrow::Cow;

/// Fußbodenaufbau (mm) von OK Rohdecke bis OK Fertigfußboden, in jedem Raum
/// des Geschosses gleich (Jörn 11.10. 17:39); Vorgabe je Geschoss.
pub const AUFBAU: f64 = 160.0;

/// Ausbau einer Wandseite.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeitenAusbau {
    /// Wandbekleidung (Putz, Tapete, Anstrich); leer ohne. Hat der Wandtyp
    /// selbst Putz zum Raum, zählen davon nur die Beschichtungen ohne Dicke,
    /// hat er auch eine Beschichtung, bleibt sie leer (plan.md §5.6).
    pub wand: Option<LayerSetId>,
    /// Wandfliesen ab OK Fertigfußboden bis zur Höhe (mm).
    pub fliesen: Option<(LayerSetId, f64)>,
    pub sockel: Option<LayerSetId>,
}

/// Ausbau eines Raums, aufgelöst bis auf die Typen des Projekts.
#[derive(Clone, Debug, PartialEq)]
pub struct RaumAusbau {
    pub boden: Option<LayerSetId>,
    pub estrich: Option<LayerSetId>,
    pub decke: Option<LayerSetId>,
    /// Je Kante in der Zählung von [`Rohbau::kanten`].
    pub seiten: Vec<SeitenAusbau>,
    /// Fußbodenaufbau des Geschosses (mm).
    pub aufbau: f64,
}

/// Estrichaufbau, den das Geschoss vorgibt (plan.md §5.1): direkt über der
/// Gründung auf der Sohlplatte mit Abdichtung, sonst auf der Decke.
pub fn estrich_vorgabe(model: &Model, geschoss: StoreyId) -> Guid {
    let Some(s) = model.storey(geschoss) else {
        return SCREED_FLOOR_TYPE_GUID;
    };
    let auf_sohle = model.storeys().iter().any(|(_, g)| {
        g.kind == LevelKind::Foundation
            && g.building == s.building
            && (g.elevation + g.height - s.elevation).abs() < 1.0
    });
    if auf_sohle {
        SCREED_SLAB_TYPE_GUID
    } else {
        SCREED_FLOOR_TYPE_GUID
    }
}

/// Raumvorlage zur Guid: die des Projekts, sonst die des Werks.
pub fn vorlage(model: &Model, g: Guid) -> Option<Cow<'_, SpaceType>> {
    match model.space_type(g) {
        Some(s) => Some(Cow::Borrowed(s)),
        None => SPACE_TEMPLATES
            .iter()
            .find(|t| t.guid == g)
            .map(|t| Cow::Owned(t.space_type())),
    }
}

/// Nutzung des Raums: die gewählte Raumvorlage (`use=`); ohne Wahl und ohne
/// gespeicherten Raum Wohnzimmer ([`DEFAULT_SPACE_GUID`], mit Grundriss
/// abgestimmt 12.10.).
pub fn nutzung(model: &Model, form: &RaumForm) -> Guid {
    form.raum
        .and_then(|id| model.space(id))
        .and_then(|s| s.nutzung)
        .unwrap_or(DEFAULT_SPACE_GUID)
}

/// Ausbautypen, die ein Raum mit der Vorlage `s` im Geschoss `geschoss`
/// braucht: die der Vorlage und, wo sie keinen Estrich nennt, der des
/// Geschosses ([`estrich_vorgabe`]).
pub fn benoetigt(model: &Model, s: &SpaceType, geschoss: StoreyId) -> Vec<Guid> {
    let mut v: Vec<Guid> = s.slots().into_iter().filter_map(|x| x.2).collect();
    if s.screed.is_none() {
        v.push(estrich_vorgabe(model, geschoss));
    }
    v
}

/// Was dem Raum nach seiner Nutzung im Projekt fehlt (Guids der Ausbautypen).
pub fn fehlende_typen(model: &Model, form: &RaumForm) -> Vec<Guid> {
    vorlage(model, nutzung(model, form)).map_or_else(Vec::new, |s| {
        benoetigt(model, &s, form.geschoss)
            .into_iter()
            .filter(|g| model.type_by_guid(*g).is_none())
            .collect()
    })
}

/// Legt im offenen Schritt an, was der Raum `raum` nach seiner Nutzung
/// braucht: die Raumvorlage (ohne Wahl Wohnzimmer) samt ihren Ausbautypen
/// und den Estrich des Geschosses. Aufrufen, wenn ein Raum entsteht oder
/// seine Nutzung wechselt; danach rechnet das Raumbuch mit Typen des
/// Projekts. `false`, wenn `raum` kein Raum ist oder etwas fehlt, das weder
/// Projekt noch Werk kennt.
pub fn typen_anlegen(model: &mut Model, raum: ElementId) -> bool {
    let (Some(e), Some(s)) = (model.element(raum), model.space(raum)) else {
        return false;
    };
    let geschoss = e.storey;
    let g = s.nutzung.unwrap_or(DEFAULT_SPACE_GUID);
    if model.ensure_space_type(g).is_none() {
        return false;
    }
    let Some(s) = model.space_type(g).cloned() else {
        return false;
    };
    benoetigt(model, &s, geschoss)
        .into_iter()
        .all(|t| model.ensure_finish_type(t).is_some())
}

impl RaumAusbau {
    /// Ausbau des Raums nach seiner Nutzung ([`nutzung`], [`vorlage`]). Kennen
    /// weder Projekt noch Werk die Vorlage, gilt Wohnzimmer.
    pub fn fuer(model: &Model, roh: &Rohbau) -> Option<RaumAusbau> {
        let s = vorlage(model, nutzung(model, &roh.form))
            .or_else(|| vorlage(model, DEFAULT_SPACE_GUID))?;
        Some(RaumAusbau::nach_vorlage(model, roh, &s))
    }

    /// Ausbau nach der Raumvorlage `s`, jede Wandseite wie die Vorlage. Wo
    /// der Wandtyp selbst Putz zum Raum hat, setzt das Raumbuch nur Anstrich
    /// oder Tapete dazu, wo er auch die Beschichtung hat, nichts (§5.6); wo
    /// Wandfliesen ab Boden liegen, keine Sockelleiste (§5.7). Typen, die das
    /// Projekt nicht führt, bleiben leer.
    pub fn nach_vorlage(model: &Model, form: &Rohbau, s: &SpaceType) -> RaumAusbau {
        let typ = |g: Option<Guid>| g.and_then(|g| model.type_by_guid(g));
        let estrich = s
            .screed
            .or(Some(estrich_vorgabe(model, form.form.geschoss)));
        let fliesen = s
            .tiles
            .and_then(|t| Some((model.type_by_guid(t.set)?, t.height)));
        let seite = |k: &Kante| SeitenAusbau {
            wand: if k.ausbau_typ.iter().any(|l| l.0 <= 0.0) {
                None
            } else {
                typ(s.wall)
            },
            fliesen,
            sockel: if fliesen.is_some() {
                None
            } else {
                typ(s.skirting)
            },
        };
        RaumAusbau {
            boden: typ(s.floor),
            estrich: typ(estrich),
            decke: typ(s.ceiling),
            seiten: form.kanten().map(seite).collect(),
            aufbau: AUFBAU,
        }
    }
}

/// Dicke eines Typs (mm), 0 ohne Typ.
pub fn dicke(model: &Model, id: Option<LayerSetId>) -> f64 {
    id.and_then(|id| model.layer_set(id))
        .map_or(0.0, |t| t.thickness())
}

/// Schichten des Estrichaufbaus mit wirksamer Dicke unter einem Belag der
/// Dicke `belag`: die unterste Dämmschicht gleicht aus, bis Estrichaufbau
/// und Belag den Fußbodenaufbau `aufbau` ergeben (Jörn 17:39). Ohne
/// Dämmschicht bleibt der Aufbau, wie er ist.
pub fn estrich_schichten(model: &Model, id: LayerSetId, belag: f64, aufbau: f64) -> Vec<f64> {
    let Some(t) = model.layer_set(id) else {
        return Vec::new();
    };
    let mut d: Vec<f64> = t.layers.iter().map(|l| l.thickness).collect();
    let ziel = aufbau - belag;
    let summe: f64 = d.iter().sum();
    if let Some(k) = t
        .layers
        .iter()
        .rposition(|l| l.function == LayerFunction::Insulation)
    {
        d[k] = (d[k] + ziel - summe).max(0.0);
    }
    d
}

/// Putzdicke einer Wandseite (mm): die dicken Schichten der Wandbekleidung
/// des Raumbuchs, sonst der Ausbau aus dem Wandtyp.
pub fn putz(model: &Model, kante: &Kante, seite: &SeitenAusbau) -> f64 {
    if !kante.ausbau_typ.is_empty() {
        return kante.ausbau_typ.iter().map(|l| l.0).sum();
    }
    dicke(model, seite.wand)
}

/// Wird die Laibung einer Öffnung dieser Art von innen behandelt? Fenster
/// und Außentüren ja; die Innentür hat als Vorgabe eine Umfassungszarge,
/// die die Laibung deckt (plan.md §5.4, VOB-Prüfung 18:10 Punkt 7).
pub fn laibung_behandelt(art: OpeningKind) -> bool {
    art != OpeningKind::InnerDoor
}

/// Ausbaudicke je Kante (mm) in Fußbodenhöhe, für den Fertigumriss des
/// Grundrisses: Putz plus Wandfliese, wo sie unten liegt; Sockelleisten
/// zählen nicht.
pub fn dicken(model: &Model, form: &Rohbau, a: &RaumAusbau) -> Vec<f64> {
    form.kanten()
        .zip(&a.seiten)
        .map(|(k, s)| putz(model, k, s) + s.fliesen.map_or(0.0, |f| dicke(model, Some(f.0))))
        .collect()
}

/// Putzdicke je Laibung (mm), in der Reihenfolge von
/// [`Rohbau::oeffnungen`]; 0, wo die Laibung nicht behandelt wird.
pub fn laibungen(model: &Model, form: &Rohbau, a: &RaumAusbau) -> Vec<f64> {
    let kanten: Vec<&Kante> = form.kanten().collect();
    form.oeffnungen
        .iter()
        .map(|o| match (kanten.get(o.kante), a.seiten.get(o.kante)) {
            (Some(k), Some(s)) if laibung_behandelt(o.art) => putz(model, k, s),
            _ => 0.0,
        })
        .collect()
}

/// Belag und Estrichaufbau mit wirksamen Dicken (mm).
pub(crate) fn boden(model: &Model, a: &RaumAusbau) -> (f64, Vec<f64>) {
    let belag = dicke(model, a.boden);
    let estrich = a
        .estrich
        .map_or_else(Vec::new, |e| estrich_schichten(model, e, belag, a.aufbau));
    (belag, estrich)
}

/// OK Fertigfußboden und UK Fertigdecke (mm, absolut).
pub fn hoehen(model: &Model, form: &Rohbau, a: &RaumAusbau) -> (f64, f64) {
    let (belag, estrich) = boden(model, a);
    let ok = form.z_boden + estrich.iter().sum::<f64>() + belag;
    (ok, form.z_decke - dicke(model, a.decke))
}

//! Ausbau eines Raums (Raumbuch RB3, `raumbuch/plan.md` §5): welcher
//! Ausbautyp an welcher Fläche liegt, die wirksamen Schichtdicken (die
//! Dämmung unter dem Estrich gleicht den Belag aus) und was der Grundriss
//! davon braucht (`grundriss/schnittstelle-raum.md` §6.3): Ausbaudicke je
//! Kante in Fußbodenhöhe, Putzdicke je Laibung, OK Fertigfußboden und UK
//! Fertigdecke.

use crate::erkennung::{Begrenzung, Kante, RaumForm, Seite};
use crate::oeffnungen::{Geschosse, OpeningKind, Rohbau};
use sk_model::model::{
    AusbauAbweichung, AusbauStelle, AusbauWahl, WandSeite, DEFAULT_SPACE_GUID,
    SCREED_FLOOR_TYPE_GUID, SCREED_SLAB_TYPE_GUID, SPACE_TEMPLATES, TILES_DEFAULT,
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

impl SeitenAusbau {
    fn mit_typ(&self) -> bool {
        self.wand.is_some() || self.fliesen.is_some() || self.sockel.is_some()
    }
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
    estrich_auf(auf_sohle)
}

fn estrich_auf(sohle: bool) -> Guid {
    if sohle {
        SCREED_SLAB_TYPE_GUID
    } else {
        SCREED_FLOOR_TYPE_GUID
    }
}

/// Wandseite einer Kante für die Abweichungen (RB6): Guid der Wand und Seite
/// zur Zeichenrichtung; an der Stirn eines Wandendes keine.
pub fn wand_seite(model: &Model, k: &Kante) -> Option<WandSeite> {
    let Begrenzung::Wand { wand, seite, .. } = &k.quelle;
    let links = match seite {
        Seite::Links => true,
        Seite::Rechts => false,
        Seite::Anfang | Seite::Ende => return None,
    };
    Some(WandSeite {
        wand: model.element(*wand)?.guid,
        links,
    })
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
        RaumAusbau::fuer_mit(model, roh, || estrich_vorgabe(model, roh.form.geschoss))
    }

    /// Wie [`RaumAusbau::fuer`], die Vorgabe des Estrichs aus den Geschossen
    /// `g` statt aus einem Gang über alle (Review #137 B2).
    pub fn fuer_in(model: &Model, roh: &Rohbau, g: &Geschosse) -> Option<RaumAusbau> {
        RaumAusbau::fuer_mit(model, roh, || estrich_auf(g.auf_sohle(roh.form.geschoss)))
    }

    fn fuer_mit(model: &Model, roh: &Rohbau, vorgabe: impl FnOnce() -> Guid) -> Option<RaumAusbau> {
        let s = vorlage(model, nutzung(model, &roh.form))
            .or_else(|| vorlage(model, DEFAULT_SPACE_GUID))?;
        Some(RaumAusbau::nach(model, roh, &s, vorgabe))
    }

    /// Führt das Projekt einen Typ dieses Ausbaus? Ohne keinen ist der Raum
    /// einer ohne Ausbau: Er hat keine Mengen (alte Dateien, Haus ohne
    /// Raumbuch), und seine Grundfläche rechnet der Grundriss mit dem Putz
    /// der Vorgabe ([`dicken_der_form`]).
    pub fn mit_typen(&self) -> bool {
        self.boden.is_some()
            || self.estrich.is_some()
            || self.decke.is_some()
            || self.seiten.iter().any(SeitenAusbau::mit_typ)
    }

    /// Ausbau nach der Raumvorlage `s` mit den Abweichungen des Raums (RB6,
    /// [`sk_model::model::Space::ausbau`]): an einer Wandseite gilt ihre
    /// eigene Abweichung, sonst die für alle Wandseiten, sonst die Vorlage,
    /// für Typ und Fliesenhöhe je für sich. Wo der Wandtyp
    /// selbst Putz zum Raum hat, setzt das Raumbuch nur Anstrich oder Tapete
    /// dazu, wo er auch die Beschichtung hat, nichts (§5.6); wo Wandfliesen
    /// ab Boden liegen, keine Sockelleiste (§5.7). Typen, die das Projekt
    /// nicht führt, bleiben leer.
    pub fn nach_vorlage(model: &Model, form: &Rohbau, s: &SpaceType) -> RaumAusbau {
        RaumAusbau::nach(model, form, s, || {
            estrich_vorgabe(model, form.form.geschoss)
        })
    }

    /// [`RaumAusbau::nach_vorlage`] mit der Vorgabe des Estrichs aus
    /// `vorgabe`.
    fn nach(
        model: &Model,
        form: &Rohbau,
        s: &SpaceType,
        vorgabe: impl FnOnce() -> Guid,
    ) -> RaumAusbau {
        let typ = |g: Option<Guid>| g.and_then(|g| model.type_by_guid(g));
        let abw = abweichungen(model, &form.form);
        let estrich = s.screed.or_else(|| Some(vorgabe()));
        RaumAusbau {
            boden: typ(wahl([bei(abw, AusbauStelle::Boden), None], s.floor)),
            estrich: typ(wahl([bei(abw, AusbauStelle::Estrich), None], estrich)),
            decke: typ(wahl([bei(abw, AusbauStelle::Decke), None], s.ceiling)),
            seiten: seiten(model, s, abw, form.kanten()),
            aufbau: AUFBAU,
        }
    }
}

/// Ausbautypen jedes gespeicherten Raums, ohne seine Form (Objekt-Panel,
/// Jörn 16.10.): die der Vorlage mit den Abweichungen des Raums wie
/// [`RaumAusbau::nach_vorlage`], an den Wandseiten der für alle und jeder,
/// den eine Wandseite für sich wählt. Ob ein Wandtyp selbst Putz zum Raum
/// hat oder Fliesen die Sockelleiste ersetzen, weiß erst die Form; das
/// Panel ordnet nur zu, gezeichnet und gerechnet wird nach der Form. Typen,
/// die das Projekt nicht führt, fehlen; Räume nach Nummer.
pub fn typen_der_raeume(model: &Model) -> Vec<(ElementId, Vec<LayerSetId>)> {
    let mut vorgaben: Vec<(StoreyId, Guid)> = Vec::new();
    let mut out = Vec::new();
    for (id, e) in model.elements().iter() {
        let Some(r) = model.space(id) else { continue };
        let Some(s) = vorlage(model, r.nutzung.unwrap_or(DEFAULT_SPACE_GUID))
            .or_else(|| vorlage(model, DEFAULT_SPACE_GUID))
        else {
            continue;
        };
        let abw = r.ausbau.as_slice();
        let estrich = s
            .screed
            .unwrap_or_else(|| match vorgaben.iter().find(|v| v.0 == e.storey) {
                Some(v) => v.1,
                None => {
                    let g = estrich_vorgabe(model, e.storey);
                    vorgaben.push((e.storey, g));
                    g
                }
            });
        let mut g = vec![
            wahl([bei(abw, AusbauStelle::Boden), None], s.floor),
            wahl([bei(abw, AusbauStelle::Estrich), None], Some(estrich)),
            wahl([bei(abw, AusbauStelle::Decke), None], s.ceiling),
        ];
        let stellen = [
            (AusbauStelle::Wand(None), s.wall),
            (AusbauStelle::Fliesen(None), s.tiles.map(|t| t.set)),
            (AusbauStelle::Sockel(None), s.skirting),
        ];
        for (st, v) in stellen {
            g.push(wahl([None, bei(abw, st)], v));
            for a in abw.iter().filter(|a| a.stelle.seite().is_some()) {
                if a.stelle.art() == st.art() {
                    g.push(wahl([Some(a), None], None));
                }
            }
        }
        let mut typen: Vec<LayerSetId> = Vec::new();
        for t in g
            .into_iter()
            .flatten()
            .filter_map(|g| model.type_by_guid(g))
        {
            if !typen.contains(&t) {
                typen.push(t);
            }
        }
        out.push((e.number.as_str(), id, typen));
    }
    out.sort_by(|a, b| a.0.cmp(b.0));
    out.into_iter().map(|(_, id, t)| (id, t)).collect()
}

/// Abweichungen des gespeicherten Raums der Form `f` (RB6), leer ohne Raum.
fn abweichungen<'a>(model: &'a Model, f: &RaumForm) -> &'a [AusbauAbweichung] {
    f.raum
        .and_then(|id| model.space(id))
        .map_or(&[], |r| r.ausbau.as_slice())
}

/// Abweichung an der Stelle `st`.
fn bei(abw: &[AusbauAbweichung], st: AusbauStelle) -> Option<&AusbauAbweichung> {
    abw.iter().find(|a| a.stelle == st)
}

/// Typ an einer Stelle: die erste Abweichung mit Typ (Wandseite, dann alle
/// Wandseiten) gibt ihn oder keinen, sonst die Vorlage.
fn wahl(kette: [Option<&AusbauAbweichung>; 2], vorlage: Option<Guid>) -> Option<Guid> {
    match kette.into_iter().flatten().find_map(|a| a.wahl) {
        Some(AusbauWahl::Keiner) => None,
        Some(AusbauWahl::Typ(g)) => Some(g),
        None => vorlage,
    }
}

/// Ausbau der Wandseiten an den Kanten `kanten` nach der Vorlage `s` mit
/// den Abweichungen `abw` des Raums.
fn seiten<'a>(
    model: &Model,
    s: &SpaceType,
    abw: &[AusbauAbweichung],
    kanten: impl Iterator<Item = &'a Kante>,
) -> Vec<SeitenAusbau> {
    let typ = |g: Option<Guid>| g.and_then(|g| model.type_by_guid(g));
    let seite = |k: &Kante| {
        let ws = wand_seite(model, k);
        let an = |st: fn(Option<WandSeite>) -> AusbauStelle| {
            [ws.and_then(|w| bei(abw, st(Some(w)))), bei(abw, st(None))]
        };
        let f = an(AusbauStelle::Fliesen);
        let fliesen = wahl(f, s.tiles.map(|t| t.set)).and_then(|g| {
            let h = f
                .into_iter()
                .flatten()
                .find_map(|a| a.hoehe)
                .or(s.tiles.map(|t| t.height))
                .unwrap_or(TILES_DEFAULT);
            Some((model.type_by_guid(g)?, h))
        });
        SeitenAusbau {
            wand: if k.ausbau_typ.iter().any(|l| l.0 <= 0.0) {
                None
            } else {
                typ(wahl(an(AusbauStelle::Wand), s.wall))
            },
            fliesen,
            sockel: if fliesen.is_some() {
                None
            } else {
                typ(wahl(an(AusbauStelle::Sockel), s.skirting))
            },
        }
    };
    kanten.map(seite).collect()
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
    dicken_an(model, form.kanten(), &a.seiten)
}

fn dicken_an<'a>(
    model: &Model,
    kanten: impl Iterator<Item = &'a Kante>,
    seiten: &[SeitenAusbau],
) -> Vec<f64> {
    kanten
        .zip(seiten)
        .map(|(k, s)| putz(model, k, s) + s.fliesen.map_or(0.0, |f| dicke(model, Some(f.0))))
        .collect()
}

/// [`dicken`] nur aus der Form `f`, ohne Rohbau (Öffnungen, Höhen) und
/// ohne Platten, für die Grundfläche des Grundrisses
/// ([`crate::flaechen::grundflaeche_fertig`]). Liest außer der Form die
/// Nutzung und die Abweichungen (RB6) des gespeicherten Raums, ihre
/// Raumvorlage (die des Projekts, sonst die des Werks; ohne Nutzung
/// Wohnzimmer), deren Wandbekleidung und Wandfliesen aus den Typen des
/// Projekts und an den Kanten den Ausbau der Wandtypen
/// (`Kante::ausbau_typ`); ob der Raum Ausbau hat, dazu die
/// Typen von Boden, Decke und Estrich (ohne Estrich in der Vorlage der des
/// Geschosses, nur wenn sonst keiner da ist). `None` bei einem Raum ohne
/// Ausbau ([`RaumAusbau::mit_typen`]), wie ihn das Mengenblatt weglässt.
pub fn dicken_der_form(model: &Model, f: &RaumForm) -> Option<Vec<f64>> {
    let s = vorlage(model, nutzung(model, f)).or_else(|| vorlage(model, DEFAULT_SPACE_GUID))?;
    let im_projekt = |g: Option<Guid>| g.and_then(|g| model.type_by_guid(g)).is_some();
    let kanten = || std::iter::once(&f.umriss).chain(&f.loecher).flatten();
    let abw = abweichungen(model, f);
    let seiten = seiten(model, &s, abw, kanten());
    let mit_typen = im_projekt(wahl([bei(abw, AusbauStelle::Boden), None], s.floor))
        || im_projekt(wahl([bei(abw, AusbauStelle::Decke), None], s.ceiling))
        || seiten.iter().any(SeitenAusbau::mit_typ)
        || im_projekt(wahl(
            [bei(abw, AusbauStelle::Estrich), None],
            Some(
                s.screed
                    .unwrap_or_else(|| estrich_vorgabe(model, f.geschoss)),
            ),
        ));
    mit_typen.then(|| dicken_an(model, kanten(), &seiten))
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

#[cfg(test)]
#[path = "ausbau_tests.rs"]
mod tests;

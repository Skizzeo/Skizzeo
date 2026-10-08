//! Fenster „Verwaltung …“ am Einzelplatz (KA-3a2, paket-ka3a §1 und §3,
//! Einstellungen §3 KA-3, soll-ka-3-verwaltung): links Suchfeld und Baum mit
//! Anzahl, rechts die Grundstufe des gewählten Eintrags, „Mehr ▸“ für die
//! Tiefe, unten links die Wirkzeile, rechts Abbrechen und OK.
//!
//! Jede Eingabe ist eine benannte Operation ([`sk_cost::Op`]). Angezeigt
//! wird der Firmenkatalog mit allen gesammelten Operationen, rein im
//! Speicher ([`sk_cost::verwaltung::mit_ops`], derselbe Weg wie beim OK).
//! OK schreibt alles als einen neuen Stand über `Scene::fuer_firma`
//! (Bausteingrenze §5, die App führt es aus), Abbrechen verwirft; bis dahin
//! ist nichts geschrieben. Ist ein Verwaltungskennwort gesetzt und an diesem
//! Platz nicht eingegeben, steht statt des Fensters erst die Abfrage
//! „Verwaltung öffnen“ ([`kennwort`]). Mit Kennwort ändert die Verwaltung
//! einen Entwurf: jede Eingabe wird gleich dorthin geschrieben, die
//! Vorschau gibt ihn frei ([`entwurf`]).

#[cfg(test)]
mod abnahme_ka3a2;
#[cfg(test)]
mod abnahme_ka3a34;
#[cfg(test)]
mod abnahme_ka3a7;
mod baum;
mod entwurf;
#[cfg(test)]
mod entwurf_tests;
mod felder;
mod kennwort;
#[cfg(test)]
mod kennwort_tests;
#[cfg(test)]
mod tests;
mod wirkung;

pub use baum::Knoten;

/// Sätze von Entwurf und Vorschau für die Satzprüfung.
#[cfg(test)]
pub fn entwurf_saetze() -> [&'static str; 2] {
    entwurf::SAETZE
}

/// Sätze des Verwaltungskennworts für die Satzprüfung.
#[cfg(test)]
pub fn kennwort_saetze() -> [&'static str; 5] {
    kennwort::SAETZE
}

use crate::catalog::Company;
use crate::catalog_view::Frame;
use crate::prefs::Win;
use crate::scene::Scene;
use crate::window_kit::{label, rounded};
use felder::{Feld, Teil};
use sk_cost::katalog::{Einheit, Katalog};
use sk_cost::{Befund, Dez, Op, SatzId};
use sk_model::{Guid, Library, Model};
use sk_paint::{Canvas, Path};
use sk_platform::{Cursor, Event, Key, Modifiers, MouseButton};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};
use std::collections::HashSet;

/// Rückgängig-Schritt im offenen Haus beim OK.
pub const STEP: &str = "Firmenkatalog geändert";
/// Rückgängig-Schritt im offenen Haus bei „Freigeben“ (KA-3b3).
pub const FREIGEGEBEN: &str = "Firmenkatalog freigegeben";

/// Fenstergröße und Teile (dip, soll-ka-3).
const W: f32 = 1120.0;
const H: f32 = 760.0;
const HEAD: f32 = 56.0;
const FOOT: f32 = 64.0;
const LISTE: f32 = 290.0;
const ZEILE: f32 = 28.0;
/// Oberkante der Baumzeilen unter dem Suchfeld.
const BAUM_Y: f32 = HEAD + 58.0;
/// Linker Rand der Grundstufe.
const SEITE_X: f32 = LISTE + 24.0;
const SEITE_Y: f32 = HEAD + 16.0;

/// Was ein Klick in der Grundstufe auslöst.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Aktion {
    Mehr,
    /// „+ Anteil“: Gerät und Sonstiges zeigen (Bedienbarkeit 2.16).
    Anteil,
    Bestaetigen(SatzId),
    Ausmustern(SatzId),
    Wiederherstellen(SatzId),
    /// Bauteilkatalog mit diesem Typ öffnen.
    TypOeffnen(Guid),
    /// „Diese Änderung zurücknehmen“ für einen Stand (KA-3a3).
    Zuruecknehmen(u32),
    /// „Aktuelles Haus als Referenzhaus“ (KA-3a4): Dateikopie in den Ordner.
    AlsReferenz,
    /// Referenzhaus im Baum wählen.
    HausOeffnen(usize),
    /// Preis des gewählten Artikels in dieser Einheit eingeben (KA-3a7).
    Je(Einheit),
    /// Vorgeschlagene Stück je Einheit bestätigen (Regel 108).
    ConvBestaetigen(Guid),
    /// Blatt „Verwaltungskennwort setzen“ öffnen (KA-3b1).
    Kennwort,
}

/// Ziel unter der Maus.
#[derive(Clone, Debug, PartialEq)]
enum Ziel {
    Schliessen,
    Suche,
    Zeile(usize),
    Feld(Feld),
    Aktion(Aktion),
    Ok,
    Abbrechen,
    Kopf,
    /// Rückfrage „n Änderungen verwerfen?“ (Bedienbarkeit 13.3).
    Zurueck,
    Verwerfen,
    /// Text mit Tooltip.
    Tipp(&'static str),
    /// Pille „Entwurf · n Änderungen“ und Knopf „Vorschau …“ (KA-3b3).
    Vorschau,
}

/// Was die App nach einem Ereignis tun muss.
#[derive(Clone, Debug, Default)]
pub struct Out {
    pub repaint: bool,
    pub moved: bool,
    /// Fenster zu (Abbrechen, Schließen, OK ohne Änderung).
    pub closed: bool,
    /// OK mit Änderungen: die App schreibt [`Verwaltung::ops`] über
    /// `Scene::fuer_firma` und schließt bei Erfolg ([`Verwaltung::fehler`]).
    pub ok: bool,
    /// Bauteilkatalog mit diesem Typ öffnen.
    pub open_type: Option<Guid>,
    /// Stand zurücknehmen: die App holt die Operationen über
    /// `Company::umkehr` und gibt sie an [`Verwaltung::zuruecknehmen`].
    pub zurueck: Option<u32>,
    /// Verwaltungskennwort eingegeben: Die App führt diesen Platz ab jetzt
    /// als Verwaltung (KA-3b1).
    pub frei: bool,
    /// Mit Kennwort: [`Verwaltung::ops`] in den Entwurf schreiben
    /// (`Company::fuer_entwurf`), danach [`Verwaltung::entwurf_gespeichert`]
    /// bzw. [`Verwaltung::fehler`] (KA-3b2).
    pub entwurf: bool,
    /// Vorschau: „Freigeben als Stand n+1“ (`Company::freigeben`).
    pub freigeben: bool,
    /// Vorschau: den ganzen Entwurf verwerfen (`Company::entwurf_verwerfen`).
    pub entwurf_verwerfen: bool,
    /// Vorschau: diese Änderung im Entwurf verwerfen
    /// (`Company::entwurf_satz_verwerfen`).
    pub satz_verwerfen: Option<SatzId>,
}

pub struct Ctx<'a> {
    pub fonts: &'a Fonts,
    pub win: Win,
}

/// Preiseingabe in anderer Einheit am gewählten Artikel (KA-3a7, Regel
/// 108); gilt nur für diesen Artikel.
#[derive(Clone, Debug, PartialEq)]
struct Je {
    artikel: Guid,
    einheit: Einheit,
    /// Letzte Eingabe in `einheit`, wie getippt.
    text: String,
    /// Geänderte Zahl im Vorschlag „Stück je m²“.
    conv: Option<String>,
}

/// Feld in Bearbeitung.
struct Edit {
    feld: Feld,
    te: TextEdit,
}

pub struct Verwaltung {
    /// Firmenkatalog beim Öffnen: Dateiinhalt (Grundlage der Vorschau) und
    /// Bibliothek.
    basis: String,
    lib0: Library,
    /// Kein Firmenkatalog geladen oder nicht lesbar: OK schreibt nicht.
    ohne_firma: bool,
    titel: String,
    /// Kopie des offenen Hauses: Baustoffnamen und EP an der Schicht.
    m: Model,
    vorher: Katalog,
    ops: Vec<Op>,
    /// Firmenkatalog mit den gesammelten Operationen und sein Katalog.
    lib: Library,
    jetzt: Katalog,
    /// Befunde, die OK sperren, und das Feld, dessen Eingabe sie auslöste.
    befunde: Vec<Befund>,
    fehler_feld: Option<Feld>,
    wahl: Knoten,
    offen: HashSet<Knoten>,
    suche: String,
    such_edit: Option<TextEdit>,
    edit: Option<Edit>,
    /// Gerät und Sonstiges gezeigt, auch wenn sie 0 sind.
    anteil: bool,
    mehr: bool,
    hover: Option<Ziel>,
    pressed: Option<Ziel>,
    scroll_baum: f32,
    scroll_seite: f32,
    pos: Option<(f32, f32)>,
    drag: Option<(f64, f64, f32, f32)>,
    wirkung: wirkung::Wirkung,
    /// Meldung im Fuß (z. B. OK gescheitert).
    meldung: Option<String>,
    /// Stand, dessen Rücknahme in den gesammelten Änderungen steckt.
    zurueck: Option<u32>,
    /// Die Operationen dieser Rücknahme (gegen den Stand, auf dem sie
    /// berechnet wurde; Befund E).
    umkehr_ops: Vec<Op>,
    /// Befund aus [`Verwaltung::neu_grundlage`], der zur Meldung des
    /// gescheiterten OK dazukommt.
    nach_grundlage: Option<String>,
    /// Ordner der eigenen Referenzhäuser (ohne Firmenkatalog keiner).
    ordner: Option<std::path::PathBuf>,
    /// Name des offenen Hauses für „Aktuelles Haus als Referenzhaus“.
    haus_name: String,
    /// Die von den gesammelten Änderungen geänderten Stammsätze.
    saetze: Vec<SatzId>,
    /// Lücken nach Regel 97 beim Öffnen; sperren nur neue.
    luecken0: Vec<String>,
    /// Rückfrage vor dem Verwerfen (Esc, ×) steht im Fuß.
    frage: bool,
    je: Option<Je>,
    /// Abfrage „Verwaltung öffnen“ (Kennwort gesetzt, an diesem Platz noch
    /// nicht eingegeben).
    abfrage: Option<kennwort::Abfrage>,
    /// Blatt „Verwaltungskennwort setzen“.
    setz: Option<kennwort::Setzen>,
    /// Mit Verwaltungskennwort: der freigegebene Stand neben dem Entwurf
    /// (KA-3b2). `basis` ist dann der Entwurf, solange einer offen ist.
    freigabe: Option<entwurf::Freigabe>,
    /// Die Operationen, deren Schreiben in den Entwurf zuletzt angestoßen
    /// wurde (nach einem Fehler nicht noch einmal dieselben).
    versucht: Vec<Op>,
    /// Blatt „Vorschau · Entwurf → Stand n+1“ (KA-3b3).
    vorschau: Option<entwurf::Vorschau>,
}

/// „0,55“ statt „0.55“.
pub(crate) fn komma(d: Dez) -> String {
    d.text().replace('.', ",")
}

/// Geldbetrag im Feld: mindestens zwei Nachkommastellen („29,40“), mehr nur,
/// wenn sie gesetzt sind („5,6695“).
fn geld(d: Dez) -> String {
    let t = komma(d);
    match t.split_once(',') {
        None => format!("{t},00"),
        Some((_, n)) if n.len() == 1 => format!("{t}0"),
        _ => t,
    }
}

/// Zahl aus einer Eingabe „0,5“ oder „1.234,50“; leer: `None`.
fn zahl(text: &str, stellen: u32) -> Option<Option<Dez>> {
    let t = text.trim().replace(['€', ' '], "");
    if t.is_empty() {
        return Some(None);
    }
    let t = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else {
        t
    };
    let t = t.replace('−', "-");
    Dez::lesen(&t, stellen).map(Some)
}

/// Kopfzeile: Name und Datum des Firmenkatalogs bzw. Werksbestand.
fn titel(vorher: &Katalog, ohne_firma: bool) -> String {
    match &vorher.kopf {
        _ if ohne_firma => "Firmenkatalog nicht erreichbar · nur ansehen".to_string(),
        _ if matches!(vorher.quelle, sk_cost::katalog::Quelle::Werk { .. }) => format!(
            "Firmenkatalog · Werksbestand {}",
            sk_cost::lesen::werksstand()
        ),
        Some(k) => {
            let datum = baum::zeit_text(k.satz.text("date").unwrap_or_default());
            // Kopfzeilen nennen den Stand (Bedienbarkeit 2.6)
            match k.stand {
                0 => format!("Firmenkatalog „{}“ · vom {datum}", k.name),
                n => format!("Firmenkatalog „{}“ · Stand {n} vom {datum}", k.name),
            }
        }
        None => format!(
            "Firmenkatalog · Werksbestand {}",
            sk_cost::lesen::werksstand()
        ),
    }
}

/// Stück je Einheit aus der Eingabe: > 0, ≤ 10 000, höchstens 4 Stellen
/// (BIM §3.2 `conv`).
fn conv_lesen(text: &str) -> Option<Dez> {
    zahl(text, 4)?.filter(|c| *c > Dez::NULL && *c <= Dez::ganz(10_000))
}

/// Kennung einer Operation zum Zusammenfassen: eine spätere Eingabe am
/// selben Satz ersetzt die frühere.
fn op_schluessel(op: &Op) -> Option<String> {
    Some(match op {
        Op::PreisSetzen { artikel, .. } => format!("preis {}", artikel.to_ifc()),
        Op::UmrechnungSetzen { artikel, .. } => format!("conv {}", artikel.to_ifc()),
        Op::BauleistungAendern { bauleistung, .. } => format!("bl {}", bauleistung.to_ifc()),
        Op::StoffanteilSetzen {
            bauleistung, nr, ..
        } => format!("anteil {} {nr}", bauleistung.to_ifc()),
        Op::FirmenwertSetzen { schluessel, .. } => format!("wert {schluessel}"),
        Op::Ausmustern { satz } | Op::Wiederherstellen { satz } => {
            format!("ruhe {} {}", satz.abschnitt, satz.kennung)
        }
        Op::HerkunftBestaetigen { satz } => format!("herkunft {} {}", satz.abschnitt, satz.kennung),
        Op::FolgeSetzen {
            bauleistung, nr, ..
        } => format!("folge {} {nr}", bauleistung.to_ifc()),
        _ => return None,
    })
}

impl Verwaltung {
    /// Öffnet das Fenster auf dem Firmenkatalog `company` (ohne: Werksbestand,
    /// OK schreibt nicht), auf Wunsch mit `wahl` gewählt.
    pub fn open(scene: &Scene, company: Option<&Company>, wahl: Option<Knoten>) -> Verwaltung {
        let m = scene.model().clone();
        // Mit Verwaltungskennwort arbeitet die Verwaltung am Entwurf
        let freigabe = company
            .filter(|c| sk_cost::verwaltung::hat_kennwort(c.library()))
            .map(|c| entwurf::Freigabe::neu(&m, c));
        let (basis, lib0, kaputt) = entwurf::grundlage(company, freigabe.is_some());
        let ohne_firma = company.is_none() || kaputt;
        let vorher = sk_cost::lesen::firma_oder_werk(&m, Some(&lib0));
        let titel = titel(freigabe.as_ref().map_or(&vorher, |f| &f.kat), ohne_firma);
        let ordner = company.and_then(|c| c.path().parent().map(|p| p.join(wirkung::ORDNER)));
        let wirkung = wirkung::Wirkung::laden(&lib0, &m, ordner.as_deref());
        let mut v = Verwaltung {
            basis,
            lib: lib0.clone(),
            lib0,
            ohne_firma,
            titel,
            m,
            jetzt: vorher.clone(),
            vorher,
            ops: Vec::new(),
            befunde: Vec::new(),
            fehler_feld: None,
            wahl: Knoten::Firmenwerte,
            offen: HashSet::new(),
            suche: String::new(),
            such_edit: None,
            edit: None,
            anteil: false,
            mehr: false,
            hover: None,
            pressed: None,
            scroll_baum: 0.0,
            scroll_seite: 0.0,
            pos: None,
            drag: None,
            wirkung,
            meldung: None,
            zurueck: None,
            umkehr_ops: Vec::new(),
            nach_grundlage: None,
            ordner,
            haus_name: "Haus".into(),
            saetze: Vec::new(),
            luecken0: Vec::new(),
            frage: false,
            je: None,
            abfrage: None,
            setz: None,
            freigabe,
            versucht: Vec::new(),
            vorschau: None,
        };
        let gibt_es = |k: &Knoten| match k {
            Knoten::Leistung(g) => v.jetzt.leistung(*g).is_some(),
            Knoten::ArtikelSatz(g) => v.jetzt.artikel(*g).is_some(),
            _ => true,
        };
        let w = wahl.filter(gibt_es).unwrap_or_else(|| {
            // Wie im Sollbild: die erste Bauleistung
            let mut ls: Vec<_> = v.jetzt.leistungen.iter().filter(|l| !l.retired).collect();
            ls.sort_by_key(|l| (v.jetzt.oz_voll(l), l.pos));
            ls.first()
                .map_or(Knoten::Firmenwerte, |l| Knoten::Leistung(l.guid))
        });
        v.waehlen(w);
        v.luecken0 = v.luecken_jetzt();
        if kaputt {
            v.meldung = Some(entwurf::KAPUTT.into());
        }
        v.entwurf_zaehlen();
        v
    }

    /// Lücken nach Regel 97 der Grundlage (sperren nur neue).
    fn luecken_jetzt(&self) -> Vec<String> {
        self.wirkung
            .luecken(&self.vorher)
            .into_iter()
            .map(|b| b.satz)
            .collect()
    }

    /// Hat ein anderer Platz den Firmenkatalog geändert, lädt OK ihn neu
    /// (`Company::fuer_firma`). Die Vorschau setzt dann auf dem neuen Stand
    /// auf: die Eingaben bleiben, „vorher“ zeigt die Werte des anderen
    /// Platzes. Sonst überschriebe das nächste OK sie ungesehen (Review 3ar).
    pub fn neu_grundlage(&mut self, company: &Company) {
        // Der freigegebene Stand neben dem Entwurf (ein anderer Platz hat
        // freigegeben)
        if self
            .freigabe
            .as_ref()
            .is_some_and(|f| f.text != company.geladen())
        {
            self.freigabe = Some(entwurf::Freigabe::neu(&self.m, company));
        }
        let (basis, lib0, kaputt) = entwurf::grundlage(Some(company), self.freigabe.is_some());
        if basis == self.basis {
            self.entwurf_zaehlen();
            return;
        }
        self.ohne_firma |= kaputt;
        self.basis_setzen(basis, lib0, true);
        // Eine Rücknahme ist gegen den alten Stand gerechnet: gegen den
        // neuen neu rechnen, sonst schriebe das nächste OK die alten Werte
        // über die Änderung des anderen Platzes (Befund E)
        if let Some(n) = self.zurueck.take() {
            let alt = std::mem::take(&mut self.umkehr_ops);
            self.ops.retain(|o| !alt.contains(o));
            // Was der Nutzer nach der Rücknahme selbst anders gesetzt hat,
            // bleibt (Review 3at); die neu gerechnete Rücknahme ersetzt es
            // nicht
            let eigen: Vec<String> = self.ops.iter().filter_map(op_schluessel).collect();
            match company.umkehr(&self.m, n) {
                Ok(ops) => {
                    let ops = ops
                        .into_iter()
                        .filter(|o| op_schluessel(o).is_none_or(|k| !eigen.contains(&k)))
                        .collect();
                    self.zuruecknehmen(n, Ok(ops));
                    return;
                }
                Err(m) => self.nach_grundlage = Some(m.to_string()),
            }
        }
        self.neu_rechnen();
    }

    /// Neue Grundlage `basis` (Text) und `lib0`; `ganz`: Referenzhäuser neu
    /// lesen, sonst nur neu rechnen.
    fn basis_setzen(&mut self, basis: String, lib0: Library, ganz: bool) {
        self.basis = basis;
        self.vorher = sk_cost::lesen::firma_oder_werk(&self.m, Some(&lib0));
        self.titel = titel(
            self.freigabe.as_ref().map_or(&self.vorher, |f| &f.kat),
            self.ohne_firma,
        );
        if ganz {
            self.wirkung = wirkung::Wirkung::laden(&lib0, &self.m, self.ordner.as_deref());
        } else {
            self.wirkung.grundlage(&lib0);
        }
        self.lib0 = lib0;
        self.luecken0 = self.luecken_jetzt();
    }

    /// Die gesammelten Operationen (für OK).
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// OK gesperrt: sperrende Befunde (paket-ka3a §1).
    pub fn gesperrt(&self) -> bool {
        !self.befunde.is_empty()
    }

    /// OK ist gescheitert: Meldung im Fuß, alle Eingaben bleiben.
    pub fn fehler(&mut self, text: String) {
        self.meldung = Some(match self.nach_grundlage.take() {
            Some(m) => format!("{text} {m}"),
            None => text,
        });
    }

    pub fn waehlen(&mut self, k: Knoten) {
        self.aufklappen_bis(&k);
        if self.wahl != k {
            self.scroll_seite = 0.0;
            self.je = None;
        }
        self.wahl = k;
    }

    /// Nach jeder Änderung der Operationen: Vorschau, Befunde, Wirkzeile.
    fn neu_rechnen(&mut self) {
        self.meldung = None;
        let mut saetze = Vec::new();
        if self.ops.is_empty() {
            self.zurueck = None;
            self.umkehr_ops.clear();
            self.lib = self.lib0.clone();
            self.jetzt = self.vorher.clone();
            self.befunde.clear();
            self.fehler_feld = None;
        } else {
            let entwurf = self.freigabe.is_some();
            match sk_cost::verwaltung::mit_ops_in(&self.basis, &self.ops, entwurf) {
                Ok((lib, s)) => {
                    saetze = s;
                    // Der Entwurf rechnet hier wie ein Firmenkatalog
                    let lib = if entwurf {
                        sk_cost::verwaltung::wie_freigegeben(&lib)
                    } else {
                        lib
                    };
                    self.jetzt = sk_cost::lesen::firma_oder_werk(&self.m, Some(&lib));
                    self.lib = lib;
                    self.fehler_feld = None;
                    // Regel 97: was beim Öffnen gedeckt war, muss gedeckt bleiben
                    let alt = &self.luecken0;
                    self.befunde = self
                        .wirkung
                        .luecken(&self.jetzt)
                        .into_iter()
                        .filter(|b| !alt.contains(&b.satz))
                        .collect();
                }
                Err(b) => {
                    // Anzeigen, was eingegeben wurde, auch wenn es sperrt
                    if let Some(k) = self.vorher.mit(&self.ops) {
                        self.jetzt = k;
                    }
                    self.befunde = b;
                }
            }
        }
        self.wirkung.rechnen(&self.lib, &saetze);
        self.saetze = saetze;
        self.entwurf_zaehlen();
    }

    /// Name des offenen Hauses (Dateiname ohne Endung).
    pub fn set_haus_name(&mut self, name: &str) {
        let n = name.strip_suffix(".szo").unwrap_or(name).trim();
        if !n.is_empty() {
            self.haus_name = n.to_string();
        }
    }

    /// „Aktuelles Haus als Referenzhaus“ (KA-3a4, Regel 106): nur eine
    /// Dateikopie des offenen Hauses in den Ordner, keine Operation, kein
    /// `[log]`. Danach die Häuser neu laden und rechnen.
    fn als_referenz(&mut self) {
        let Some(ordner) = self.ordner.clone() else {
            self.meldung =
                Some("Ohne Firmenkatalog gibt es keinen Ordner für Referenzhäuser.".into());
            return;
        };
        let pfad = wirkung::frei(&ordner, &self.haus_name);
        let r = std::fs::create_dir_all(&ordner)
            .map_err(|e| {
                crate::meldung::Meldung::aus_io(
                    "Referenzhaus nicht abgelegt",
                    "Ordner für Referenzhäuser anlegen",
                    &ordner,
                    &e,
                )
            })
            .and_then(|_| crate::document::save(&self.m, &pfad));
        match r {
            Ok(()) => {
                self.wirkung = wirkung::Wirkung::laden(&self.lib0, &self.m, Some(ordner.as_path()));
                self.wirkung.rechnen(&self.lib, &self.saetze);
                let name = pfad
                    .file_stem()
                    .map_or(String::new(), |n| n.to_string_lossy().into_owned());
                if let Some(i) = self.wirkung.haeuser.iter().position(|h| h.name == name) {
                    self.waehlen(Knoten::Haus(i));
                }
            }
            Err(m) => self.meldung = Some(m.to_string()),
        }
    }

    /// Ist `op` gleich dem Stand beim Öffnen (dann entfällt sie)?
    fn wie_vorher(&self, op: &Op) -> bool {
        let v = &self.vorher;
        match op {
            Op::PreisSetzen { artikel, preis, .. } => {
                v.artikel(*artikel).is_some_and(|a| a.preis == *preis)
            }
            Op::UmrechnungSetzen { artikel, conv } => {
                v.artikel(*artikel).is_some_and(|a| a.conv == *conv)
            }
            Op::BauleistungAendern { bauleistung, daten } => v
                .leistung(*bauleistung)
                .is_some_and(|l| sk_cost::preis::bauleistung(l) == *daten),
            Op::FirmenwertSetzen { schluessel, wert } => firmenwert(v, schluessel) == Some(*wert),
            Op::StoffanteilSetzen {
                bauleistung,
                nr,
                anteil,
            } => {
                let alt =
                    v.anteile_von(*bauleistung)
                        .find(|a| a.nr == *nr)
                        .map(|a| match a.artikel {
                            Some(g) => sk_cost::op::Stoff::Artikel {
                                artikel: g,
                                menge: a.menge,
                            },
                            None => sk_cost::op::Stoff::Schicht { faktor: a.menge },
                        });
                alt == *anteil
            }
            Op::Ausmustern { satz } => ruhestand(v, satz) == Some(true),
            Op::Wiederherstellen { satz } => ruhestand(v, satz) == Some(false),
            _ => false,
        }
    }

    /// Nimmt eine Operation auf: ersetzt eine frühere am selben Satz und
    /// entfällt, wenn der Satz damit wieder wie beim Öffnen ist.
    pub fn setzen(&mut self, op: Op) {
        if let Some(s) = op_schluessel(&op) {
            self.ops.retain(|o| op_schluessel(o).as_deref() != Some(&s));
        }
        if !self.wie_vorher(&op) {
            self.ops.push(op);
        }
        // `conv` vor dem Preis: sonst ersetzt seine Herkunft die Eingabe in
        // `[origin] source` (Regel 108); stabil, sonst bleibt die Folge
        self.ops
            .sort_by_key(|o| !matches!(o, Op::UmrechnungSetzen { .. }));
        self.neu_rechnen();
    }

    /// Eingabe in `feld` des gewählten Eintrags übernehmen; `false`, wenn
    /// sie sich nicht lesen lässt (Feld bleibt, nichts geändert).
    pub fn eingeben(&mut self, feld: &Feld, text: &str) -> bool {
        if let Feld::Conv(_) = feld {
            // Nur die Zahl des Vorschlags; geschrieben wird mit „Bestätigen“
            let ok = conv_lesen(text).is_some();
            match (&mut self.je, ok) {
                (Some(je), true) => je.conv = Some(text.trim().to_string()),
                _ => self.fehler_feld = Some(feld.clone()),
            }
            return ok;
        }
        if let (Feld::Preis(g), Some(je)) = (feld, self.je.as_mut()) {
            if je.artikel == *g {
                je.text = text.trim().to_string();
            }
        }
        let op = match self.op_fuer(feld, text) {
            Some(op) => op,
            None => {
                self.fehler_feld = Some(feld.clone());
                self.meldung = self.je_fehler(feld, text);
                return false;
            }
        };
        self.setzen(op);
        if !self.befunde.is_empty() {
            self.fehler_feld = Some(feld.clone());
        }
        true
    }

    fn op_fuer(&self, feld: &Feld, text: &str) -> Option<Op> {
        let k = &self.jetzt;
        let leistung = || match &self.wahl {
            Knoten::Leistung(g) => k.leistung(*g),
            _ => None,
        };
        Some(match feld {
            Feld::Kurz | Feld::Stunden | Feld::Geraet | Feld::Sonst | Feld::Nu => {
                let l = leistung()?;
                let mut d = sk_cost::preis::bauleistung(l);
                match feld {
                    Feld::Kurz => {
                        let t = text.trim();
                        if t.is_empty() {
                            return None;
                        }
                        d.kurz = t.to_string();
                    }
                    Feld::Stunden => d.stunden = zahl(text, 4)?.unwrap_or(Dez::NULL),
                    Feld::Geraet => d.geraet = zahl(text, 4)?.unwrap_or(Dez::NULL),
                    Feld::Sonst => d.sonst = zahl(text, 4)?.unwrap_or(Dez::NULL),
                    _ => d.nu = zahl(text, 4)?,
                }
                Op::BauleistungAendern {
                    bauleistung: l.guid,
                    daten: d,
                }
            }
            Feld::Menge(nr) => {
                let l = leistung()?;
                let a = k.anteile_von(l.guid).find(|a| a.nr == *nr)?;
                let menge = zahl(text, 4)??;
                Op::StoffanteilSetzen {
                    bauleistung: l.guid,
                    nr: *nr,
                    anteil: Some(match a.artikel {
                        Some(artikel) => sk_cost::op::Stoff::Artikel { artikel, menge },
                        None => sk_cost::op::Stoff::Schicht { faktor: menge },
                    }),
                }
            }
            Feld::Preis(g) => {
                let a = k.artikel(*g)?;
                let eingabe = zahl(text, 4)?;
                let (preis, eingabe) = match (self.je_von(a), eingabe) {
                    (Some(je), Some(e)) => {
                        let u = sk_cost::einheit::umrechnen(e, je, a, None).ok()?;
                        (Some(u.preis), u.eingabe)
                    }
                    (_, e) => (e, String::new()),
                };
                Op::PreisSetzen {
                    artikel: a.guid,
                    preis,
                    stand: crate::preis_blatt::stand_jetzt(),
                    quelle: "Verwaltung".into(),
                    eingabe,
                }
            }
            Feld::Conv(_) => return None,
            Feld::Wert(s) => Op::FirmenwertSetzen {
                schluessel: s.clone(),
                wert: zahl(text, 4)??,
            },
        })
    }

    /// Text eines Felds, wie er gerade gilt (mit den Änderungen).
    fn feld_text(&self, k: &Katalog, feld: &Feld) -> Option<String> {
        let l = match &self.wahl {
            Knoten::Leistung(g) => k.leistung(*g),
            _ => None,
        };
        Some(match feld {
            Feld::Kurz => l?.kurz.clone(),
            Feld::Stunden => komma(l?.stunden),
            Feld::Geraet => geld(l?.geraet),
            Feld::Sonst => geld(l?.sonst),
            Feld::Nu => l?.nu.map(geld).unwrap_or_default(),
            Feld::Menge(nr) => komma(k.anteile_von(l?.guid).find(|a| a.nr == *nr)?.menge),
            Feld::Preis(g) => k.artikel(*g)?.preis.map(geld).unwrap_or_default(),
            Feld::Conv(g) => {
                let a = k.artikel(*g)?;
                let v = sk_cost::einheit::vorschlag(a)?;
                match self.je.as_ref().and_then(|j| j.conv.clone()) {
                    Some(t) => t,
                    None => komma(v.conv),
                }
            }
            Feld::Wert(s) if s == "wage" => geld(firmenwert(k, s)?),
            Feld::Wert(s) => komma(firmenwert(k, s)?),
        })
    }

    /// „vorher 0,55“, wenn das Feld anders als beim Öffnen ist; bei einer
    /// Eingabe in anderer Einheit mit der Einheit des Artikels.
    fn vorher_text(&self, feld: &Feld) -> Option<String> {
        if let Feld::Conv(_) = feld {
            return None;
        }
        let alt = self.feld_text(&self.vorher, feld)?;
        let neu = self.feld_text(&self.jetzt, feld)?;
        let einheit = match feld {
            Feld::Preis(g) => self
                .jetzt
                .artikel(*g)
                .filter(|a| self.je_von(a).is_some())
                .map(|a| format!(" €/{}", a.einheit.zeichen())),
            _ => None,
        }
        .unwrap_or_default();
        (alt != neu).then(|| {
            if alt.is_empty() {
                "vorher leer".into()
            } else {
                format!("vorher {alt}{einheit}")
            }
        })
    }

    /// Was das Feld zeigt: bei Eingabe in anderer Einheit die letzte
    /// Eingabe, sonst der gültige Wert.
    pub(crate) fn anzeige(&self, feld: &Feld) -> String {
        if let (Feld::Preis(g), Some(je)) = (feld, &self.je) {
            if je.artikel == *g
                && self
                    .jetzt
                    .artikel(*g)
                    .is_some_and(|a| a.einheit != je.einheit)
            {
                return je.text.clone();
            }
        }
        self.feld_text(&self.jetzt, feld).unwrap_or_default()
    }

    /// Warum sich ein Preis in anderer Einheit nicht umrechnen lässt
    /// (Befund 108), für den Fuß.
    fn je_fehler(&self, feld: &Feld, text: &str) -> Option<String> {
        let Feld::Preis(g) = feld else {
            return None;
        };
        let a = self.jetzt.artikel(*g)?;
        let je = self.je_von(a)?;
        let e = zahl(text, 4)??;
        sk_cost::einheit::umrechnen(e, je, a, None)
            .err()
            .map(|b| b.satz)
    }

    /// Gewählte Eingabeeinheit am Artikel `a`, wenn sie nicht seine ist.
    fn je_von(&self, a: &sk_cost::katalog::Artikel) -> Option<Einheit> {
        self.je
            .as_ref()
            .filter(|j| j.artikel == a.guid && j.einheit != a.einheit)
            .map(|j| j.einheit)
    }

    /// Segment „je m² | je m³ | je Stück“ (KA-3a7).
    fn je_waehlen(&mut self, e: Einheit) {
        let Knoten::ArtikelSatz(g) = self.wahl else {
            return;
        };
        self.ende_edit(true);
        let conv = self.je.take().and_then(|j| j.conv);
        self.je = Some(Je {
            artikel: g,
            einheit: e,
            text: String::new(),
            conv,
        });
        self.fehler_feld = None;
    }

    /// „Bestätigen“ am Vorschlag: `conv` geht mit `kind=manual` ins OK.
    fn conv_bestaetigen(&mut self, g: Guid) {
        self.ende_edit(true);
        let Some(a) = self.jetzt.artikel(g) else {
            return;
        };
        let Some(v) = sk_cost::einheit::vorschlag(a) else {
            return;
        };
        let conv = match self.je.as_ref().and_then(|j| j.conv.as_deref()) {
            Some(t) => match conv_lesen(t) {
                Some(c) => c,
                None => return,
            },
            None => v.conv,
        };
        self.setzen(Op::UmrechnungSetzen {
            artikel: g,
            conv: Some(conv),
        });
        if let Some(j) = self.je.as_mut() {
            j.conv = None;
        }
    }

    /// Die Operationen, die Stand `stand` umkehren (aus `Company::umkehr`),
    /// kommen zu den gesammelten; OK schreibt sie als neuen Stand. Ein
    /// Befund steht im Fuß, nichts ändert sich.
    pub fn zuruecknehmen(&mut self, stand: u32, r: Result<Vec<Op>, String>) {
        match r {
            Ok(ops) => {
                for op in ops {
                    if let Some(s) = op_schluessel(&op) {
                        self.ops.retain(|o| op_schluessel(o).as_deref() != Some(&s));
                    }
                    if !self.wie_vorher(&op) && !self.ops.contains(&op) {
                        self.umkehr_ops.push(op.clone());
                        self.ops.push(op);
                    }
                }
                self.neu_rechnen();
                self.zurueck = Some(stand);
            }
            Err(m) => self.meldung = Some(m),
        }
    }

    pub fn aktion(&mut self, a: Aktion) -> Option<Guid> {
        match a {
            Aktion::Mehr => self.mehr = !self.mehr,
            Aktion::Anteil => self.anteil = true,
            Aktion::Bestaetigen(satz) => self.setzen(Op::HerkunftBestaetigen { satz }),
            Aktion::Ausmustern(satz) => self.setzen(Op::Ausmustern { satz }),
            Aktion::Wiederherstellen(satz) => self.setzen(Op::Wiederherstellen { satz }),
            Aktion::TypOeffnen(g) => return Some(g),
            // Geht über die App (Archiv der Stände)
            Aktion::Zuruecknehmen(_) => {}
            Aktion::AlsReferenz => self.als_referenz(),
            Aktion::HausOeffnen(i) => self.waehlen(Knoten::Haus(i)),
            Aktion::Je(e) => self.je_waehlen(e),
            Aktion::ConvBestaetigen(g) => self.conv_bestaetigen(g),
            Aktion::Kennwort => self.setz = Some(kennwort::Setzen::default()),
        }
        None
    }

    // --- Lage ---------------------------------------------------------------

    fn size(&self, w: &Win) -> (f32, f32) {
        let s = w.scale;
        if self.abfrage.is_some() {
            return ((kennwort::ABF_W * s).round(), (kennwort::ABF_H * s).round());
        }
        let ww = (W * s).min(w.w as f32);
        let hh = (H * s).min((w.h - w.top) as f32);
        (ww.round(), hh.round())
    }

    fn frame(&self, w: &Win) -> Rect {
        let (ww, hh) = self.size(w);
        let (x, y) = self.pos.unwrap_or((
            (w.w as f32 - ww) * 0.5,
            w.top as f32 + (w.h as f32 - w.top as f32 - hh) * 0.5,
        ));
        let x = x.min(w.w as f32 - ww).max(0.0);
        let y = y.min(w.h as f32 - hh).max(w.top as f32);
        Rect::new(x.round(), y.round(), ww, hh)
    }

    pub fn origin(&self, t: &Theme, w: &Win) -> (i32, i32) {
        let f = self.frame(w);
        let m = (t.size.panel_shadow * w.scale).round();
        ((f.x - m) as i32, (f.y - m) as i32)
    }

    /// Fensterbreite und -höhe in dip.
    fn dip(&self, w: &Win) -> (f32, f32) {
        let (ww, hh) = self.size(w);
        (ww / w.scale, hh / w.scale)
    }

    /// Rechteck in dip ab der linken oberen Fensterecke, in Pixeln.
    fn r(&self, w: &Win, x: f32, y: f32, ww: f32, hh: f32) -> Rect {
        let f = self.frame(w);
        let s = w.scale;
        Rect::new(
            (f.x + x * s).round(),
            (f.y + y * s).round(),
            (ww * s).round(),
            (hh * s).round(),
        )
    }

    fn close_rect(&self, w: &Win) -> Rect {
        let (ww, _) = self.dip(w);
        self.r(w, ww - 46.0, 14.0, 28.0, 28.0)
    }

    fn such_rect(&self, w: &Win) -> Rect {
        self.r(w, 14.0, HEAD + 14.0, LISTE - 28.0, 30.0)
    }

    /// Unterkante des Baums und der Grundstufe (dip).
    fn unten(&self, w: &Win) -> f32 {
        self.dip(w).1 - FOOT
    }

    fn zeile_rect(&self, w: &Win, i: usize) -> Rect {
        let y = BAUM_Y + i as f32 * ZEILE - self.scroll_baum;
        self.r(w, 10.0, y, LISTE - 20.0, ZEILE)
    }

    fn knoepfe(&self, w: &Win) -> Vec<(Ziel, Rect, &'static str)> {
        let (ww, hh) = self.dip(w);
        let (links, rechts) = if self.frage {
            ((Ziel::Zurueck, "Zurück"), (Ziel::Verwerfen, "Verwerfen"))
        } else if self.freigabe.is_some() {
            // Im Entwurf ist jede Eingabe gleich geschrieben: kein OK, nur
            // die Vorschau (soll-ka-3b)
            return match self.entwurf_anzahl() {
                0 => Vec::new(),
                _ => vec![(
                    Ziel::Vorschau,
                    self.r(w, ww - 190.0, hh - 48.0, 170.0, 32.0),
                    "Vorschau …",
                )],
            };
        } else {
            ((Ziel::Abbrechen, "Abbrechen"), (Ziel::Ok, "OK"))
        };
        vec![
            (
                links.0,
                self.r(w, ww - 260.0, hh - 48.0, 120.0, 32.0),
                links.1,
            ),
            (
                rechts.0,
                self.r(w, ww - 130.0, hh - 48.0, 110.0, 32.0),
                rechts.1,
            ),
        ]
    }

    /// Esc oder ×: mit gesammelten Änderungen erst fragen (Bedienbarkeit
    /// 13.3); „Abbrechen“ verwirft ohne Frage, weil es ausdrücklich ist.
    fn schliessen(&mut self, out: &mut Out) {
        if self.ops.is_empty() {
            out.closed = true;
        } else {
            self.frage = true;
        }
    }

    /// Breite der Grundstufe (dip).
    fn seite_w(&self, w: &Win) -> f32 {
        self.dip(w).0 - SEITE_X - 24.0
    }

    /// Teile der Grundstufe in Pixeln (verschoben um den Bildlauf).
    fn teile_px(&self, w: &Win, fonts: &Fonts) -> Vec<(Rect, Teil)> {
        let s = w.scale;
        let f = self.frame(w);
        let x0 = f.x + SEITE_X * s;
        let y0 = f.y + (SEITE_Y - self.scroll_seite) * s;
        self.seite(fonts, self.seite_w(w))
            .into_iter()
            .map(|t| {
                let r = Rect::new(
                    (x0 + t.r.x * s).round(),
                    (y0 + t.r.y * s).round(),
                    (t.r.w * s).round(),
                    (t.r.h * s).round(),
                );
                (r, t)
            })
            .collect()
    }

    fn hit(&self, w: &Win, fonts: &Fonts, x: f64, y: f64) -> Option<Ziel> {
        if self.close_rect(w).contains(x, y) {
            return Some(Ziel::Schliessen);
        }
        for (z, r, _) in self.knoepfe(w) {
            if r.contains(x, y) {
                return Some(z);
            }
        }
        let f = self.frame(w);
        let s = w.scale;
        let (_, hh) = self.dip(w);
        let dy = (y as f32 - f.y) / s;
        let dx = (x as f32 - f.x) / s;
        if !f.contains(x, y) {
            return None;
        }
        if self.pille_rect(w, fonts).is_some_and(|r| r.contains(x, y)) {
            return Some(Ziel::Vorschau);
        }
        if dy < HEAD {
            return Some(Ziel::Kopf);
        }
        if dy >= hh - FOOT {
            return None;
        }
        if dx < LISTE {
            if self.such_rect(w).contains(x, y) {
                return Some(Ziel::Suche);
            }
            if dy < BAUM_Y {
                return None;
            }
            let n = self.zeilen().len();
            return (0..n)
                .find(|i| self.zeile_rect(w, *i).contains(x, y))
                .map(Ziel::Zeile);
        }
        for (r, t) in self.teile_px(w, fonts) {
            if let Some(z) = &t.ziel {
                if r.contains(x, y) {
                    return Some(z.clone());
                }
            }
        }
        None
    }

    // --- Ereignisse -----------------------------------------------------------

    pub fn handle(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        let mut out = self.handle_innen(e, cx);
        out.entwurf = self.entwurf_faellig();
        out
    }

    fn handle_innen(&mut self, e: &Event, cx: &mut Ctx) -> Out {
        if self.abfrage.is_some() {
            return self.abfrage_handle(e, cx);
        }
        if self.setz.is_some() {
            return self.setz_handle(e, cx);
        }
        if self.vorschau.is_some() {
            return self.vorschau_handle(e, cx);
        }
        let mut out = Out::default();
        match *e {
            Event::MouseMove { x, y, .. } => {
                if let Some((sx, sy, px, py)) = self.drag {
                    self.pos = Some((px + (x - sx) as f32, py + (y - sy) as f32));
                    out.moved = true;
                    return out;
                }
                let h = self.hit(&cx.win, cx.fonts, x, y);
                if h != self.hover {
                    self.hover = h;
                    out.repaint = true;
                }
            }
            Event::MouseLeave => {
                if self.hover.take().is_some() {
                    out.repaint = true;
                }
            }
            Event::MouseDown {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => self.mouse_down(x, y, cx, &mut out),
            Event::MouseUp {
                button: MouseButton::Left,
                x,
                y,
                ..
            } => {
                self.drag = None;
                let p = self.pressed.take();
                let h = self.hit(&cx.win, cx.fonts, x, y);
                if p.is_some() && p == h {
                    match p {
                        Some(Ziel::Ok) => self.ok(&mut out),
                        Some(Ziel::Abbrechen) | Some(Ziel::Verwerfen) => out.closed = true,
                        Some(Ziel::Schliessen) => self.schliessen(&mut out),
                        Some(Ziel::Zurueck) => self.frage = false,
                        Some(Ziel::Vorschau) => self.vorschau_oeffnen(),
                        _ => {}
                    }
                }
                out.repaint = true;
            }
            Event::Wheel { delta, x, .. } => {
                let f = self.frame(&cx.win);
                let dx = (x as f32 - f.x) / cx.win.scale;
                let schritt = -(delta as f32) * 3.0 * ZEILE;
                if dx < LISTE {
                    let n = self.zeilen().len() as f32;
                    let max = (n * ZEILE - (self.unten(&cx.win) - BAUM_Y)).max(0.0);
                    self.scroll_baum = (self.scroll_baum + schritt).clamp(0.0, max);
                } else {
                    let h = felder::hoehe(&self.seite(cx.fonts, self.seite_w(&cx.win)));
                    let max = (h - (self.unten(&cx.win) - SEITE_Y) + 16.0).max(0.0);
                    self.scroll_seite = (self.scroll_seite + schritt).clamp(0.0, max);
                }
                out.repaint = true;
            }
            Event::Key {
                key,
                down: true,
                mods,
                ..
            } => self.key(key, mods, &mut out),
            Event::Text(c) => {
                if let Some(ed) = self.edit.as_mut() {
                    if !c.is_control() {
                        // Kurztext nimmt höchstens 70 Zeichen an (Regel 79)
                        let voll = ed.feld == Feld::Kurz
                            && ed.te.text.chars().count() - ed.te.selected().chars().count()
                                >= sk_cost::verwaltung::KURZ_MAX;
                        if !voll {
                            ed.te.insert(&c.to_string());
                        }
                    }
                } else if let Some(te) = self.such_edit.as_mut() {
                    if !c.is_control() {
                        te.insert(&c.to_string());
                        self.suche = te.text.clone();
                        self.scroll_baum = 0.0;
                    }
                }
                out.repaint = true;
            }
            _ => {}
        }
        out
    }

    fn ok(&mut self, out: &mut Out) {
        self.ende_edit(true);
        // Im Entwurf gibt es kein OK: geschrieben wird je Eingabe
        if self.gesperrt() || self.freigabe.is_some() {
            return;
        }
        if self.ops.is_empty() {
            out.closed = true;
        } else if self.ohne_firma {
            self.meldung = Some("Kein Firmenkatalog geladen; nichts gespeichert.".into());
        } else {
            out.ok = true;
        }
    }

    fn mouse_down(&mut self, x: f64, y: f64, cx: &mut Ctx, out: &mut Out) {
        out.repaint = true;
        let h = self.hit(&cx.win, cx.fonts, x, y);
        // Feld verlassen übernimmt die Eingabe (wie in den Einstellungen)
        let im_feld = matches!((&h, &self.edit), (Some(Ziel::Feld(f)), Some(e)) if *f == e.feld);
        if !im_feld {
            self.ende_edit(true);
        }
        if h != Some(Ziel::Suche) {
            self.such_edit = None;
        }
        // Rückfrage offen: nur ihre Knöpfe; jeder andere Klick nimmt sie zurück
        if self.frage && !matches!(h, Some(Ziel::Zurueck | Ziel::Verwerfen)) {
            self.frage = false;
        }
        let nur_ansehen = |a: &Aktion| {
            matches!(
                a,
                Aktion::Bestaetigen(_)
                    | Aktion::Ausmustern(_)
                    | Aktion::Wiederherstellen(_)
                    | Aktion::Zuruecknehmen(_)
                    | Aktion::AlsReferenz
                    | Aktion::ConvBestaetigen(_)
                    | Aktion::Kennwort
            )
        };
        match h {
            Some(Ziel::Feld(_)) if self.ohne_firma => {}
            Some(Ziel::Tipp(_)) => {}
            Some(Ziel::Aktion(a)) if self.ohne_firma && nur_ansehen(&a) => {}
            Some(Ziel::Kopf) => {
                let f = self.frame(&cx.win);
                self.drag = Some((x, y, f.x, f.y));
            }
            Some(Ziel::Suche) => {
                if self.such_edit.is_none() {
                    self.such_edit = Some(TextEdit::new(&self.suche));
                }
            }
            Some(Ziel::Zeile(i)) => {
                if let Some(z) = self.zeilen().get(i).cloned() {
                    if z.ast {
                        if z.offen {
                            self.offen.remove(&z.knoten);
                        } else {
                            self.offen.insert(z.knoten.clone());
                        }
                    }
                    self.waehlen(z.knoten);
                }
            }
            Some(Ziel::Feld(f)) => {
                if !im_feld {
                    let text = self.anzeige(&f);
                    self.edit = Some(Edit {
                        feld: f,
                        te: TextEdit::new(&text),
                    });
                }
            }
            Some(Ziel::Aktion(Aktion::Zuruecknehmen(n))) => {
                if self.zurueck != Some(n) {
                    out.zurueck = Some(n);
                }
            }
            Some(Ziel::Aktion(a)) => {
                if let Some(g) = self.aktion(a) {
                    out.open_type = Some(g);
                }
            }
            Some(
                z @ (Ziel::Ok
                | Ziel::Vorschau
                | Ziel::Abbrechen
                | Ziel::Schliessen
                | Ziel::Zurueck
                | Ziel::Verwerfen),
            ) => {
                let gesperrt = z == Ziel::Ok && self.gesperrt();
                if !gesperrt {
                    self.pressed = Some(z);
                }
            }
            None => {}
        }
    }

    /// Feld schließen; `uebernehmen`: Eingabe als Operation.
    fn ende_edit(&mut self, uebernehmen: bool) {
        if let Some(ed) = self.edit.take() {
            let alt = self.anzeige(&ed.feld);
            if uebernehmen && ed.te.text != alt {
                self.eingeben(&ed.feld, &ed.te.text);
            }
        }
    }

    fn key(&mut self, key: Key, mods: Modifiers, out: &mut Out) {
        out.repaint = true;
        let such = self.edit.is_none() && self.such_edit.is_some();
        if self.edit.is_none() && !such {
            match key {
                // Rückfrage: Esc und Enter bleiben im Fenster
                Key::Escape | Key::Enter if self.frage => self.frage = false,
                Key::Escape => self.schliessen(out),
                Key::Enter => self.ok(out),
                _ => {}
            }
            return;
        }
        match key {
            Key::Escape if such => {
                self.such_edit = None;
                self.suche.clear();
                return;
            }
            Key::Escape => {
                self.ende_edit(false);
                return;
            }
            Key::Enter | Key::Tab if such => {
                self.such_edit = None;
                return;
            }
            Key::Enter | Key::Tab => {
                self.ende_edit(true);
                return;
            }
            _ => {}
        }
        let te = match (self.edit.as_mut(), self.such_edit.as_mut()) {
            (Some(ed), _) => &mut ed.te,
            (None, Some(te)) => te,
            (None, None) => return,
        };
        let sel = mods.shift;
        match key {
            Key::Backspace => te.backspace(),
            Key::Delete => te.delete(),
            Key::Left => te.left(sel),
            Key::Right => te.right(sel),
            Key::Home => te.home(sel),
            Key::End => te.end(sel),
            Key::Char('A') if mods.ctrl => te.select_all(),
            _ => return,
        }
        if such {
            self.suche = te.text.clone();
            self.scroll_baum = 0.0;
        }
    }

    pub fn busy(&self) -> bool {
        self.edit.is_some() || self.such_edit.is_some() || self.drag.is_some()
    }

    pub fn cursor(&self) -> Cursor {
        match &self.hover {
            Some(Ziel::Feld(_) | Ziel::Suche) => Cursor::IBeam,
            Some(Ziel::Aktion(_) | Ziel::Zeile(_)) => Cursor::Hand,
            _ => Cursor::Arrow,
        }
    }

    // --- Zeichnen -------------------------------------------------------------

    pub fn paint_frame(&mut self, t: &Theme, fonts: &Fonts, w: &Win) -> Frame {
        let (c, x, y) = self.paint(t, fonts, w);
        Frame::Full {
            x,
            y,
            w: c.width as u32,
            h: c.height as u32,
            px: c.to_premul_rgba8(),
        }
    }

    pub fn paint(&self, t: &Theme, fonts: &Fonts, w: &Win) -> (Canvas, i32, i32) {
        let f = self.frame(w);
        let s = w.scale;
        let m = (t.size.panel_shadow * s).round();
        let (cw, ch) = ((f.w + 2.0 * m) as usize, (f.h + 2.0 * m) as usize);
        let mut c = Canvas::new(cw, ch);
        c.set_origin(f.x - m, f.y - m);
        widgets::panel(&mut c, f, s, t);
        self.paint_into(&mut c, t, fonts, w);
        c.set_origin(0.0, 0.0);
        let (x, y) = self.origin(t, w);
        (c, x, y)
    }

    fn paint_into(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        if self.abfrage.is_some() {
            self.abfrage_malen(c, t, fonts, w);
            return;
        }
        let f = self.frame(w);
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let line = s.round().max(1.0);
        let (_, hh) = self.dip(w);
        // Grundstufe zuerst; Kopf und Fuß decken ab, was hinausragt
        felder::malen(
            c,
            t,
            fonts,
            self,
            &self.teile_px(w, fonts),
            s,
            f.y + HEAD * s,
            f.y + (hh - FOOT) * s,
        );
        // Baum
        let baum_x = f.x;
        c.fill_rect(baum_x, f.y, (LISTE * s).round(), f.h, u.bg);
        let zeilen = self.zeilen();
        for (i, z) in zeilen.iter().enumerate() {
            let r = self.zeile_rect(w, i);
            if r.y + r.h < f.y + BAUM_Y * s || r.y > f.y + (hh - FOOT) * s {
                continue;
            }
            let gewaehlt = z.knoten == self.wahl;
            if gewaehlt {
                rounded(c, r, 6.0 * s, u.pressed);
                c.fill_rect(
                    r.x - 6.0 * s,
                    r.y + 3.0 * s,
                    3.0 * s,
                    r.h - 6.0 * s,
                    u.accent,
                );
            } else if self.hover == Some(Ziel::Zeile(i)) {
                rounded(c, r, 6.0 * s, u.hover);
            }
            let x = r.x + (8.0 + 16.0 * f32::from(z.tiefe)) * s;
            let base = r.y + r.h * 0.5 + 4.5 * s;
            if z.ast {
                widgets::disclosure(c, x + 4.0 * s, r.y + r.h * 0.5, z.offen, u.text_dim, s);
            }
            let tx = x + 18.0 * s;
            let font = if z.tiefe == 0 || gewaehlt {
                bold
            } else {
                regular
            };
            let px = t.size.font * s;
            let zahl_w = z.anzahl.map_or(0.0, |_| 34.0 * s);
            let text = widgets::ellipsize(font, &z.text, px, r.x + r.w - tx - zahl_w - 6.0 * s);
            // Unlesbares Referenzhaus grau (Regel 106)
            let farbe = if z.grau { u.text_disabled } else { u.text };
            label(c, font, &text, px, tx, base, farbe);
            // Punkt an Einträgen, die der Entwurf ändert (soll-ka-3b)
            if self.entwurf_geaendert(&z.knoten) {
                let tw = font.map_or(0.0, |f| f.width(&text, px));
                let d = 3.0 * s;
                rounded(
                    c,
                    Rect {
                        x: (tx + tw + 8.0 * s).round(),
                        y: (r.y + r.h * 0.5 - d).round(),
                        w: 2.0 * d,
                        h: 2.0 * d,
                    },
                    d,
                    u.accent,
                );
            }
            if let (Some(n), Some(fr)) = (z.anzahl, regular) {
                let ns = n.to_string();
                let pxs = t.size.font_small * s;
                let nw = fr.width(&ns, pxs);
                label(
                    c,
                    regular,
                    &ns,
                    pxs,
                    r.x + r.w - 8.0 * s - nw,
                    base,
                    u.text_dim,
                );
            }
        }
        // Suchfeld über dem Baum
        c.fill_rect(
            f.x,
            f.y + HEAD * s,
            (LISTE * s).round(),
            (BAUM_Y - HEAD - 4.0) * s,
            u.bg,
        );
        let sr = self.such_rect(w);
        let (stext, caret, select) = match &self.such_edit {
            Some(te) => (te.text.as_str(), Some(te.caret), Some(te.selection())),
            None => (self.suche.as_str(), None, None),
        };
        let leer = stext.is_empty() && self.such_edit.is_none();
        widgets::text_field(
            c,
            fonts,
            sr,
            &FieldState {
                text: if leer { "" } else { stext },
                hover: self.hover == Some(Ziel::Suche),
                focus: self.such_edit.is_some(),
                caret,
                select,
                ..Default::default()
            },
            s,
            t,
        );
        if leer {
            label(
                c,
                regular,
                "Suche …",
                t.size.font_small * s,
                widgets::text_field_x(sr, s, t),
                sr.y + sr.h * 0.5 + 4.0 * s,
                u.text_disabled,
            );
        }
        // Kopf
        c.fill_rect(f.x, f.y, f.w, HEAD * s, u.bg);
        label(
            c,
            bold,
            "Verwaltung",
            t.size.font_title * s,
            f.x + 20.0 * s,
            f.y + 35.0 * s,
            u.text,
        );
        let tw = bold.map_or(0.0, |b| b.width("Verwaltung", t.size.font_title * s));
        label(
            c,
            regular,
            &self.titel,
            t.size.font_small * s,
            f.x + 20.0 * s + tw + 10.0 * s,
            f.y + 34.0 * s,
            u.text_dim,
        );
        self.pille_malen(c, t, fonts, w);
        let cr = self.close_rect(w);
        if self.hover == Some(Ziel::Schliessen) {
            rounded(c, cr, 6.0 * s, u.hover);
        }
        let (cx0, cy0, d) = (cr.x + cr.w * 0.5, cr.y + cr.h * 0.5, 5.5 * s);
        let mut p = Path::new();
        p.segment((cx0 - d, cy0 - d), (cx0 + d, cy0 + d), 1.4 * s);
        p.segment((cx0 - d, cy0 + d), (cx0 + d, cy0 - d), 1.4 * s);
        c.fill(&p, u.text_dim);
        // Fuß
        let fy = f.y + (hh - FOOT) * s;
        c.fill_rect(f.x, fy, f.w, FOOT * s, u.bg);
        c.fill_rect(f.x, f.y + HEAD * s, f.w, line, u.border);
        c.fill_rect(f.x, fy, f.w, line, u.border);
        c.fill_rect(
            (f.x + LISTE * s).round(),
            f.y + HEAD * s,
            line,
            (hh - HEAD - FOOT) * s,
            u.border,
        );
        self.paint_fuss(c, t, fonts, w, fy);
        for (z, r, text) in self.knoepfe(w) {
            let disabled = z == Ziel::Ok && self.gesperrt();
            let st = ButtonState {
                hover: self.hover.as_ref() == Some(&z) && !disabled,
                pressed: self.pressed.as_ref() == Some(&z),
                active: z == Ziel::Ok && !disabled,
                disabled,
            };
            widgets::button(c, fonts, r, text, st, s, t);
        }
        // Tooltip über allem, unter dem Text, im Fenster gehalten
        if let Some(Ziel::Tipp(tipp)) = &self.hover {
            let ziel = Some(Ziel::Tipp(tipp));
            if let Some((r, _)) = self.teile_px(w, fonts).iter().find(|(_, x)| x.ziel == ziel) {
                let tt = widgets::tooltip(fonts, tipp, s, t);
                let x = r.x.min(f.x + f.w - tt.width as f32 - 8.0 * s).max(f.x);
                let y = r.y + r.h + 6.0 * s;
                c.blit(&tt, x.round() as i32, y.round() as i32);
            }
        }
        self.setz_malen(c, t, fonts, w);
        self.vorschau_malen(c, t, fonts, w);
    }

    /// Fuß links: Wirkzeile, sonst der sperrende Befund oder die Meldung.
    fn paint_fuss(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win, fy: f32) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let px = t.size.font_small * s;
        // Zwei Zeilen: oben Wirkzeile, Befund oder Rückfrage, darunter für
        // wen die Änderung gilt (Bedienbarkeit 13.1)
        let base = fy + 27.0 * s;
        let f = self.frame(w);
        let mut x = f.x + 20.0 * s;
        let rand = self
            .knoepfe(w)
            .first()
            .map_or(f.x + f.w - 20.0 * s, |k| k.1.x - 16.0 * s);
        let gilt = if self.ohne_firma {
            "Ohne Firmenkatalog lässt sich hier nichts speichern."
        } else if self.freigabe.is_some() {
            if self.entwurf_fuss(c, t, fonts, w, fy, rand) {
                return;
            }
            ""
        } else {
            "Gilt für neue Häuser und dieses Haus, außer wo es eigene Werte hat. Gespeicherte Häuser zeigen oben „Für neue Häuser gilt …“."
        };
        let unten = widgets::ellipsize(regular, gilt, px, rand - x);
        label(c, regular, &unten, px, x, base + 20.0 * s, u.text_dim);
        if self.frage {
            let n = self.ops.len();
            let frage = if n == 1 {
                "Eine Änderung verwerfen?".to_string()
            } else {
                format!("{n} Änderungen verwerfen?")
            };
            let text = widgets::ellipsize(bold, &frage, px, rand - x);
            label(c, bold, &text, px, x, base, u.text);
            return;
        }
        let satz = self
            .meldung
            .clone()
            .or_else(|| self.befunde.first().map(|b| b.satz.clone()));
        if let Some(satz) = satz {
            let n = self.befunde.len();
            let satz = if n > 1 {
                format!("{satz} (+{} weitere)", n - 1)
            } else {
                satz
            };
            let text = widgets::ellipsize(regular, &satz, px, rand - x);
            label(c, regular, &text, px, x, base, u.field_invalid);
            return;
        }
        let teile = wirkung::zeile(self.wirkung.alle());
        let breite =
            |font: Option<&sk_paint::font::Font>, t: &str| font.map_or(0.0, |f| f.width(t, px));
        for (i, (h, (name, alt, neu))) in self.wirkung.alle().zip(&teile).enumerate() {
            let p = wirkung::prozent(h.vorher, h.nachher).filter(|_| alt.is_some());
            // Passt der Eintrag nicht mehr, steht dort „+ n weitere“
            let gesamt = breite(regular, "·")
                + breite(regular, name)
                + alt.as_ref().map_or(0.0, |a| {
                    breite(regular, a) + breite(regular, "→") + 16.0 * s
                })
                + breite(bold, neu)
                + p.as_ref().map_or(0.0, |p| breite(regular, p) + 8.0 * s)
                + 24.0 * s;
            if i > 0 && x + gesamt > rand {
                let rest = format!("+ {} weitere", teile.len() - i);
                label(c, regular, &rest, px, x, base, u.text_dim);
                break;
            }
            if i > 0 {
                label(c, regular, "·", px, x, base, u.text_dim);
                x += breite(regular, "·") + 8.0 * s;
            }
            label(c, regular, name, px, x, base, u.text_dim);
            x += breite(regular, name) + 8.0 * s;
            if let Some(alt) = alt {
                label(c, regular, alt, px, x, base, u.text_disabled);
                let aw = breite(regular, alt);
                c.fill_rect(
                    x,
                    (base - 4.0 * s).round(),
                    aw,
                    s.round().max(1.0),
                    u.text_disabled,
                );
                x += aw + 8.0 * s;
                label(c, regular, "→", px, x, base, u.text);
                x += breite(regular, "→") + 8.0 * s;
            }
            label(c, bold, neu, px, x, base, u.text);
            x += breite(bold, neu) + 8.0 * s;
            if let Some(p) = p {
                label(c, regular, &p, px, x, base, u.text_dim);
                x += breite(regular, &p) + 8.0 * s;
            }
            x += 8.0 * s;
        }
    }
}

/// Firmenwert `schluessel` im Katalog (Werkswert als Rückfall).
fn firmenwert(k: &Katalog, schluessel: &str) -> Option<Dez> {
    let w = &k.werte;
    match schluessel {
        "wage" => Some(w.lohn),
        "surcharge" => Some(w.zuschlag),
        "vat" => Some(w.mwst),
        s => s.strip_prefix("steel.").and_then(|art| {
            w.stahl(art).or_else(|| {
                sk_cost::katalog::RATEN
                    .iter()
                    .find(|r| r.0 == s)
                    .map(|r| Dez::ganz(r.3))
            })
        }),
    }
}

/// Ausgemustert? `None`, wenn es den Satz nicht gibt.
fn ruhestand(k: &Katalog, satz: &SatzId) -> Option<bool> {
    let g = Guid::from_ifc(&satz.kennung)?;
    match satz.abschnitt {
        "article" => k.artikel(g).map(|a| a.retired),
        "service" => k.leistung(g).map(|l| l.retired),
        "lot" => k.los(g).map(|l| l.retired),
        _ => None,
    }
}

//! Blatt AVA im Mengenfenster (KA-4b, architektur/paket-ka4.md §1 und §4,
//! Einstellungen §3 KA-4): das Leistungsverzeichnis eines Loses im Umfang
//! der Geschoss-Chips. Links der LV-Baum (Lose mit ihren Titeln,
//! Zusammenstellung, Prüfen), rechts die Tabelle, unten nach Klick auf eine
//! Position Mengenansatz, Preisanteile und Eigenschaften.
//!
//! Das Blatt rechnet nichts: Das LV kommt aus [`Scene::lv`] auf dem
//! Kostenblatt desselben Umfangs.

use crate::kosten_view::{euro, geschoss_name, gewerk_name, kg_name, prozent, tausender};
use crate::picking::Picking;
use crate::scene::Scene;
use crate::schedule_view::ListOut;
use crate::umfang_view::{self, Leiste};
use sk_cost::befund::{Ort, Schwere};
use sk_cost::lv::{Lv, LvPosition, LvWahl};
use sk_cost::{Cent, Dez, Katalog};
use sk_model::{ElementId, Guid, Model};
use sk_paint::font::Font;
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::text_edit::TextEdit;
use sk_ui::theme::Theme;
use sk_ui::widgets::Fonts;
use std::rc::Rc;
use std::time::Instant;

/// Kopf (dip ab Unterkante der Kartenleiste) wie im Reiter Kosten.
const TITLE_Y: f32 = 34.0;
const SUB_Y: f32 = 52.0;
const CHIP_TOP: f32 = 62.0;
const SWITCH_TOP: f32 = CHIP_TOP + umfang_view::ROW + 4.0;
const SWITCH_H: f32 = 24.0;
/// Oberkante von Baum und Tabelle, darüber die Linie.
const BODY: f32 = SWITCH_TOP + SWITCH_H + 16.0;
/// LV-Baum: Breite und Zeilenhöhe; Luft bis zur Tabelle.
const TREE_W: f32 = 268.0;
const TREE_ROW: f32 = 26.0;
const TREE_GAP: f32 = 40.0;
/// Tabelle: Spaltenkopf und Zeilen.
const HEAD_ROW: f32 = 26.0;
const ROW_TITEL: f32 = 32.0;
const ROW_UNTER: f32 = 26.0;
const ROW_POS: f32 = 23.0;
/// Detailbereich unten und Hinweiszeile ohne Auswahl.
const DETAIL_H: f32 = 196.0;
const HINT_H: f32 = 30.0;
const BOTTOM: f32 = 12.0;
/// OZ-Spalte und Spalten von rechts (dip): GP rechtsbündig am Rand, EP
/// 130 davor, Einheit linksbündig 290 davor, Menge rechtsbündig 340 davor.
const OZ_W: f32 = 86.0;
const EP_R: f32 = 130.0;
const EINHEIT_L: f32 = 290.0;
const MENGE_R: f32 = 340.0;

pub const HINWEIS: &str =
    "Klick auf eine Position öffnet unten Mengenansatz, Preisanteile und Eigenschaften.";

type Rect = (f32, f32, f32, f32);

fn inside((rx, ry, rw, rh): Rect, x: f32, y: f32) -> bool {
    x >= rx && x < rx + rw && y >= ry && y < ry + rh
}

/// Was rechts steht.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ansicht {
    #[default]
    Lv,
    Zusammenstellung,
    Pruefen,
}

/// Reiter des Detailbereichs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reiter {
    #[default]
    Ansatz,
    Anteile,
    Eigenschaften,
}

impl Reiter {
    const ALLE: [Reiter; 3] = [Reiter::Ansatz, Reiter::Anteile, Reiter::Eigenschaften];

    fn label(self) -> &'static str {
        match self {
            Reiter::Ansatz => "Mengenansatz",
            Reiter::Anteile => "Preisanteile",
            Reiter::Eigenschaften => "Eigenschaften",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hot {
    Umfang(umfang_view::Hot),
    /// Schalter „Für Anfrage (leer) | Mit Preisen“.
    Preise(bool),
    Baum(usize),
    Zeile(usize),
    Reiter(Reiter),
    Schliessen,
    /// „Kopf und Vorbemerkungen“ auf- und zuklappen.
    Kopf,
    /// „Bauherr fehlt“: öffnet den Kopf mit dem Feld Bauherr.
    BauherrFehlt,
    /// „Mehr“ und darin „Geschosse als Untertitel“.
    Mehr,
    Untertitel,
    Feld(kopf::Feld),
    /// „LV … als Tabelle speichern“.
    Knopf,
}

/// Punkt im Baum: grün Preis da, Akzent Preis fehlt, grau leer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Punkt {
    Da,
    Fehlt,
    Leer,
}

fn punkt<'a>(ps: impl Iterator<Item = &'a LvPosition>) -> Punkt {
    let mut leer = true;
    for p in ps {
        if p.preis_fehlt {
            return Punkt::Fehlt;
        }
        leer = false;
    }
    if leer {
        Punkt::Leer
    } else {
        Punkt::Da
    }
}

/// Eine Zeile des LV-Baums.
#[derive(Clone, Debug, PartialEq)]
pub enum Knoten {
    Los {
        guid: Guid,
        name: String,
        anzahl: usize,
        punkt: Punkt,
        offen: bool,
    },
    Titel {
        los: Guid,
        guid: Guid,
        nr: String,
        name: String,
        anzahl: usize,
        punkt: Punkt,
    },
    Trenner,
    Zusammenstellung,
    Pruefen(usize),
}

impl Knoten {
    fn hoehe(&self) -> f32 {
        match self {
            Knoten::Trenner => 16.0,
            _ => TREE_ROW,
        }
    }
}

/// Art einer Tabellenzeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Art {
    Titel,
    Untertitel,
    Position,
    /// Netto, MwSt., brutto der Zusammenstellung.
    Summe,
    /// Leise Zeile (Hinweis, „nicht ausgeschrieben“).
    Leise,
    /// Punkt im Prüfen.
    Befund(Schwere),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Zeile {
    pub art: Art,
    pub oz: String,
    pub text: String,
    pub menge: String,
    pub einheit: String,
    pub ep: String,
    pub gp: String,
    pub elemente: Vec<ElementId>,
    /// Titel der Zeile (Sprung aus dem Baum).
    titel: Option<Guid>,
    /// Ziel eines Befunds: OZ ohne Los.
    ziel: Option<String>,
}

impl Zeile {
    fn neu(art: Art, oz: impl Into<String>, text: impl Into<String>) -> Zeile {
        Zeile {
            art,
            oz: oz.into(),
            text: text.into(),
            menge: String::new(),
            einheit: String::new(),
            ep: String::new(),
            gp: String::new(),
            elemente: Vec::new(),
            titel: None,
            ziel: None,
        }
    }

    fn hoehe(&self) -> f32 {
        match self.art {
            Art::Titel => ROW_TITEL,
            Art::Untertitel => ROW_UNTER,
            Art::Summe => 26.0,
            Art::Befund(_) => 26.0,
            Art::Position | Art::Leise => ROW_POS,
        }
    }
}

/// Inhalt des Detailbereichs einer Position, beim `sync` gebaut.
#[derive(Clone, Debug, PartialEq)]
pub struct Detail {
    pub oz: String,
    pub kurztext: String,
    /// Geschoss, Bauteile, Menge, Herkunft.
    pub ansatz: Vec<[String; 4]>,
    pub summe: String,
    /// Kurzform der Preisanteile und Gewerk, Kostengruppe (Spalte „Preis“).
    pub preis: Vec<String>,
    pub anteile: Vec<(String, String)>,
    pub eigenschaften: Vec<(String, String)>,
    pub elemente: Vec<ElementId>,
}

/// Sprung nach dem nächsten Aufbau der Zeilen.
#[derive(Clone, Debug, PartialEq)]
enum Sprung {
    Titel(Guid),
    Oz(String),
}

pub struct AvaView {
    pub leiste: Leiste,
    pub w: u32,
    pub h: u32,
    pub scale: f32,
    /// Oberkante des Blatts unter Titelleiste und Karten (dip).
    pub top: f32,
    /// „Mit Preisen“; sonst „Für Anfrage (leer)“.
    pub preise: bool,
    los: Option<Guid>,
    offen: Vec<Guid>,
    ansicht: Ansicht,
    baum: Vec<Knoten>,
    lv: Option<Rc<Lv>>,
    zeilen: Vec<Zeile>,
    gebaut: Option<(*const Lv, Ansicht)>,
    sprung: Option<Sprung>,
    scroll: f32,
    /// Position im Detailbereich (OZ ohne Los).
    gewaehlt: Option<String>,
    detail: Option<Detail>,
    detail_von: Option<(*const Lv, String)>,
    reiter: Reiter,
    hot: Option<Hot>,
    hover: Vec<ElementId>,
    selected: Vec<ElementId>,
    pub subtitle: String,
    /// Zahl der Karte: „LV Rohbau · 7 Pos.“
    pub karte: String,
    /// Dateiname („haus.szo“): Bauvorhaben ohne `Project.site`.
    pub datei: String,
    projekt: Option<sk_model::Project>,
    /// `[costproject] lvstorey`.
    untertitel: bool,
    kopf_offen: bool,
    mehr_offen: bool,
    /// Feld im Kopf in Bearbeitung.
    feld: Option<(kopf::Feld, TextEdit)>,
    /// Umfang und Stand für den aufgeklappten Kopf.
    kopf_umfang: (String, String),
    /// „Referenzpreise 10/2026“ bzw. „Preise Firmenkatalog“; leer ohne
    /// Preise.
    preisquelle: String,
    knopf_down: bool,
}

impl Default for AvaView {
    fn default() -> Self {
        AvaView::new()
    }
}

/// Detailbereich, Reiterknöpfe und Inhalt darunter.
type DetailLage = (Rect, Vec<(Reiter, Rect)>, Rect);

impl AvaView {
    pub fn new() -> AvaView {
        AvaView {
            leiste: Leiste::default(),
            w: 0,
            h: 0,
            scale: 1.0,
            top: 0.0,
            preise: true,
            los: None,
            offen: Vec::new(),
            ansicht: Ansicht::Lv,
            baum: Vec::new(),
            lv: None,
            zeilen: Vec::new(),
            gebaut: None,
            sprung: None,
            scroll: 0.0,
            gewaehlt: None,
            detail: None,
            detail_von: None,
            reiter: Reiter::Ansatz,
            hot: None,
            hover: Vec::new(),
            selected: Vec::new(),
            subtitle: String::new(),
            karte: String::new(),
            datei: String::new(),
            projekt: None,
            untertitel: false,
            kopf_offen: false,
            mehr_offen: false,
            feld: None,
            kopf_umfang: (String::new(), String::new()),
            preisquelle: String::new(),
            knopf_down: false,
        }
    }

    /// An Modell, Katalog und Umfang angleichen. `true`, wenn neu
    /// gezeichnet werden muss.
    pub fn sync(&mut self, s: &mut Scene, firma: Option<(&sk_model::Library, u64)>) -> bool {
        let mut changed = self.leiste.sync(s.model());
        let kat = s.katalog(firma);
        let mut lose: Vec<(Guid, String, String)> = kat
            .lose
            .iter()
            .filter(|l| l.parent.is_none() && !l.retired)
            .map(|l| (l.guid, l.nr.clone(), l.name.clone()))
            .collect();
        lose.sort_by(|a, b| (a.1.len(), &a.1).cmp(&(b.1.len(), &b.1)));
        if !lose.iter().any(|l| Some(l.0) == self.los) {
            self.los = lose.first().map(|l| l.0);
            self.offen = self.los.into_iter().collect();
            self.gewaehlt = None;
            self.scroll = 0.0;
            changed = true;
        }
        let (j, mo, ..) = sk_platform::local_date_time();
        let untertitel = kat.kopie.as_ref().is_some_and(|k| k.lvstorey);
        let mut baum = Vec::new();
        let mut gezeigt = None;
        for (g, _, name) in &lose {
            let w = LvWahl {
                los: *g,
                untertitel,
                preise: self.preise,
                heute: Some((j, mo)),
            };
            let lv = s.lv(firma, &self.leiste.umfang, &w);
            let offen = self.offen.contains(g);
            baum.push(Knoten::Los {
                guid: *g,
                name: format!("Los {name}"),
                anzahl: lv.anzahl(),
                punkt: punkt(lv.titel.iter().flat_map(|t| &t.positionen)),
                offen,
            });
            if offen {
                for t in &lv.titel {
                    baum.push(Knoten::Titel {
                        los: *g,
                        guid: t.guid,
                        nr: t.nr.clone(),
                        name: t.name.clone(),
                        anzahl: t.positionen.len(),
                        punkt: punkt(t.positionen.iter()),
                    });
                }
            }
            if Some(*g) == self.los {
                gezeigt = Some(lv);
            }
        }
        let Some(lv) = gezeigt else {
            changed |= self.lv.take().is_some() || !self.zeilen.is_empty();
            self.zeilen.clear();
            self.baum = baum;
            return changed;
        };
        baum.push(Knoten::Trenner);
        baum.push(Knoten::Zusammenstellung);
        baum.push(Knoten::Pruefen(lv.zaehlen().0));
        if baum != self.baum {
            self.baum = baum;
            changed = true;
        }
        let karte = format!("LV {} · {} Pos.", lv.kopf.los, lv.anzahl());
        if karte != self.karte {
            self.karte = karte;
            changed = true;
        }
        let key = (Rc::as_ptr(&lv), self.ansicht);
        if self.gebaut != Some(key) {
            self.zeilen = match self.ansicht {
                Ansicht::Lv => lv_zeilen(&lv),
                Ansicht::Zusammenstellung => zusammenstellung(&lv),
                Ansicht::Pruefen => pruefen(&lv),
            };
            self.gebaut = Some(key);
            changed = true;
        }
        if let Some(oz) = &self.gewaehlt {
            let von = (Rc::as_ptr(&lv), oz.clone());
            if self.detail_von.as_ref() != Some(&von) {
                self.detail = detail(s.model(), &kat, &lv, oz);
                if self.detail.is_none() {
                    self.gewaehlt = None;
                }
                self.detail_von = Some(von);
                changed = true;
            }
        } else if self.detail.take().is_some() {
            self.detail_von = None;
            changed = true;
        }
        if let Some(sp) = self.sprung.take() {
            changed |= self.springe(&sp);
        }
        self.projekt = Some(s.model().project().clone());
        self.preisquelle = if self.preise {
            sk_cost::lesen::preisquelle(&kat)
        } else {
            String::new()
        };
        if self.untertitel != untertitel {
            self.untertitel = untertitel;
            changed = true;
        }
        let mut text = format!("LV {}", lv.kopf.los);
        let b = self.bauvorhaben(&lv);
        if !b.is_empty() {
            text += &format!(" · Bauvorhaben {b}");
        }
        if let Some(a) = lv.kopf.aufsteller.as_deref().filter(|a| !a.is_empty()) {
            text += &format!(" · Aufsteller {a}");
        }
        let ut = umfang_view::umfang_text(
            s.model(),
            &self.leiste.umfang,
            sk_platform::local_date_time(),
        );
        text += " · ";
        text += &ut;
        self.kopf_umfang = match ut.split_once(" · Stand ") {
            Some((a, b)) => (a.to_string(), b.to_string()),
            None => (ut.clone(), String::new()),
        };
        if changed || self.subtitle.is_empty() {
            self.subtitle = text;
        }
        let neu = self.lv.as_ref().is_none_or(|x| !Rc::ptr_eq(x, &lv));
        self.lv = Some(lv);
        self.clamp();
        changed || neu
    }

    /// Hover und Auswahl aus dem Modell übernehmen; `true`, wenn sich das
    /// Bild ändert.
    pub fn follow(&mut self, p: &Picking) -> bool {
        let hover: Vec<ElementId> = p.hovered().collect();
        let changed = hover != self.hover || p.selected != self.selected;
        self.hover = hover;
        self.selected = p.selected.clone();
        changed
    }

    /// Detailbereich schließen; `true`, wenn er offen war.
    pub fn schliessen(&mut self) -> bool {
        self.detail = None;
        self.detail_von = None;
        self.gewaehlt.take().is_some()
    }

    fn springe(&mut self, sp: &Sprung) -> bool {
        let mut y = 0.0;
        for z in &self.zeilen {
            let passt = match sp {
                Sprung::Titel(g) => z.art == Art::Titel && z.titel == Some(*g),
                Sprung::Oz(oz) => z.art == Art::Position && z.oz == *oz,
            };
            if passt {
                let ziel = match sp {
                    Sprung::Titel(_) => y,
                    Sprung::Oz(_) => (y - 2.0 * ROW_POS).max(0.0),
                };
                self.scroll = ziel;
                self.clamp();
                return true;
            }
            y += z.hoehe();
        }
        false
    }

    // --- Lage ----------------------------------------------------------------

    fn content_x(&self, t: &Theme) -> (f32, f32) {
        let s = self.scale;
        let pad = t.size.sheet_pad * s;
        (pad, (self.w as f32 - 2.0 * pad).max(0.0))
    }

    fn top_px(&self) -> f32 {
        self.top * self.scale
    }

    fn leiste_lage(&self, t: &Theme) -> umfang_view::Lage {
        umfang_view::Lage {
            x0: self.content_x(t).0,
            y: self.top_px() + CHIP_TOP * self.scale,
            s: self.scale,
        }
    }

    fn body_top(&self) -> f32 {
        self.top_px() + (BODY + self.kopf_h()) * self.scale
    }

    fn bottom(&self) -> f32 {
        self.h as f32 - BOTTOM * self.scale
    }

    /// Linker Rand der Tabelle und rechter Rand (px).
    fn tabelle_x(&self, t: &Theme) -> (f32, f32) {
        let (x0, cw) = self.content_x(t);
        (x0 + (TREE_W + TREE_GAP) * self.scale, x0 + cw)
    }

    /// Oberkante und Unterkante der Tabellenzeilen (px).
    fn liste_y(&self) -> (f32, f32) {
        let s = self.scale;
        let oben = self.body_top() + (HEAD_ROW + 6.0) * s;
        let unten = if self.detail.is_some() {
            self.bottom() - DETAIL_H * s
        } else {
            self.bottom() - HINT_H * s
        };
        (oben, unten.max(oben))
    }

    fn inhalt_h(&self) -> f32 {
        self.zeilen.iter().map(Zeile::hoehe).sum()
    }

    fn clamp(&mut self) {
        let (oben, unten) = self.liste_y();
        let sicht = (unten - oben) / self.scale;
        let max = (self.inhalt_h() - sicht).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// Sichtbare Zeilen: Index, Oberkante und Höhe (px).
    fn sichtbar(&self) -> Vec<(usize, f32, f32)> {
        let s = self.scale;
        let (oben, unten) = self.liste_y();
        let mut y = oben - self.scroll * s;
        let mut v = Vec::new();
        for (i, z) in self.zeilen.iter().enumerate() {
            let h = z.hoehe() * s;
            if y + h > oben && y < unten {
                v.push((i, y, h));
            }
            y += h;
        }
        v
    }

    /// Baumzeilen: Index, Oberkante und Höhe (px).
    fn baum_lage(&self, t: &Theme) -> Vec<(usize, Rect)> {
        let s = self.scale;
        let (x0, _) = self.content_x(t);
        let mut y = self.body_top() + TREE_ROW * s;
        self.baum
            .iter()
            .enumerate()
            .map(|(i, k)| {
                let h = k.hoehe() * s;
                let r = (x0, y, TREE_W * s, h);
                y += h;
                (i, r)
            })
            .collect()
    }

    /// Schalter „Preise“: Beschriftung links und die beiden Segmente.
    fn schalter(&self, t: &Theme, fonts: &Fonts) -> (f32, [(bool, Rect); 2]) {
        let s = self.scale;
        let (_, r) = self.tabelle_x(t);
        let px = 10.5 * s;
        let regular = fonts.regular.as_ref();
        let bold = fonts.bold.as_ref().or(regular);
        let w = |text: &str| {
            bold.map_or(text.chars().count() as f32 * px * 0.6, |f| {
                f.width(text, px)
            }) + 20.0 * s
        };
        let label_w = regular.map_or(40.0 * s, |f| f.width("Preise", px)) + 10.0 * s;
        let y = self.top_px() + SWITCH_TOP * s;
        let inset = 2.0 * s;
        let mit = w("Mit Preisen");
        let ohne = w("Für Anfrage (leer)");
        let x_mit = r - inset - mit;
        let x_ohne = x_mit - ohne;
        let h = SWITCH_H * s - 2.0 * inset;
        (
            x_ohne - inset - label_w,
            [
                (false, (x_ohne, y + inset, ohne, h)),
                (true, (x_mit, y + inset, mit, h)),
            ],
        )
    }

    /// Detailbereich: Fläche, Reiter und Schließen (px).
    fn detail_lage(&self, t: &Theme, fonts: &Fonts) -> Option<DetailLage> {
        self.detail.as_ref()?;
        let s = self.scale;
        let (tx, r) = self.tabelle_x(t);
        let oben = self.bottom() - DETAIL_H * s + 8.0 * s;
        let fl = (tx - 8.0 * s, oben, r - tx + 16.0 * s, self.bottom() - oben);
        let px = 10.5 * s;
        let bold = fonts.bold.as_ref().or(fonts.regular.as_ref());
        let zu = (r - 22.0 * s, oben + 8.0 * s, 22.0 * s, 22.0 * s);
        let mut x = zu.0 - 12.0 * s;
        let mut reiter = Vec::new();
        for re in Reiter::ALLE.into_iter().rev() {
            let w = bold.map_or(80.0 * s, |f| f.width(re.label(), px)) + 20.0 * s;
            x -= w;
            reiter.push((re, (x, oben + 10.0 * s, w, 20.0 * s)));
        }
        reiter.reverse();
        Some((fl, reiter, zu))
    }

    fn hit(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<Hot> {
        let (x, y) = (x as f32, y as f32);
        if inside(self.knopf_rect(t, fonts), x, y) {
            return Some(Hot::Knopf);
        }
        if let Some(h) = self.kopf_hit(t, fonts, x, y) {
            return h;
        }
        if let Some(h) = self.leiste.hit(fonts, self.leiste_lage(t), x, y) {
            return Some(Hot::Umfang(h));
        }
        let (_, segs) = self.schalter(t, fonts);
        if let Some((b, _)) = segs.iter().find(|(_, r)| inside(*r, x, y)) {
            return Some(Hot::Preise(*b));
        }
        if let Some((fl, reiter, zu)) = self.detail_lage(t, fonts) {
            if inside(zu, x, y) {
                return Some(Hot::Schliessen);
            }
            if let Some((re, _)) = reiter.iter().find(|(_, r)| inside(*r, x, y)) {
                return Some(Hot::Reiter(*re));
            }
            if inside(fl, x, y) {
                return None;
            }
        }
        if let Some((i, _)) = self
            .baum_lage(t)
            .into_iter()
            .find(|(i, r)| inside(*r, x, y) && self.baum[*i] != Knoten::Trenner)
        {
            return Some(Hot::Baum(i));
        }
        let (tx, r) = self.tabelle_x(t);
        if x >= tx - 8.0 * self.scale && x < r + 8.0 * self.scale {
            let (oben, unten) = self.liste_y();
            if y >= oben && y < unten {
                if let Some((i, ..)) = self
                    .sichtbar()
                    .into_iter()
                    .find(|(_, zy, zh)| y >= *zy && y < zy + zh)
                {
                    return Some(Hot::Zeile(i));
                }
            }
        }
        None
    }

    // --- Ereignisse ----------------------------------------------------------

    pub fn mouse_move(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        x: f64,
        y: f64,
    ) -> Option<ListOut> {
        let hot = self.hit(t, fonts, x, y);
        let repaint = hot != self.hot;
        self.hot = hot;
        self.leiste.hot = match hot {
            Some(Hot::Umfang(h)) => Some(h),
            _ => None,
        };
        let group = match hot {
            Some(Hot::Zeile(i)) => self.zeilen[i].elemente.clone(),
            _ => Vec::new(),
        };
        if p.set_hover(None, group) {
            self.hover = p.hovered().collect();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_leave(&mut self, p: &mut Picking) -> Option<ListOut> {
        let repaint = self.hot.take().is_some();
        self.leiste.hot = None;
        if p.set_hover(None, Vec::new()) {
            self.hover.clear();
            return Some(ListOut::Picking { selection: false });
        }
        repaint.then_some(ListOut::Repaint)
    }

    pub fn mouse_down(
        &mut self,
        t: &Theme,
        fonts: &Fonts,
        p: &mut Picking,
        (x, y): (f64, f64),
        mods: sk_platform::Modifiers,
    ) -> Option<ListOut> {
        let hot = self.hit(t, fonts, x, y);
        // Klick neben das Feld schreibt es; neben „Mehr“ schließt es
        let mut out = None;
        let im_feld = matches!((&self.feld, hot), (Some((f, _)), Some(Hot::Feld(g))) if *f == g);
        if self.feld.is_some() && !im_feld {
            out = self.feld_schliessen(true);
        }
        if self.mehr_offen && !matches!(hot, Some(Hot::Mehr | Hot::Untertitel)) {
            self.mehr_offen = false;
            out = out.or(Some(ListOut::Repaint));
        }
        if let Some(o) = self.kopf_klick(t, fonts, hot, x) {
            return Some(match out {
                Some(ListOut::Kosten(w)) => ListOut::Kosten(w),
                _ => o,
            });
        }
        if out.is_some() {
            return out;
        }
        if let Some(Hot::Umfang(h)) = hot {
            self.leiste.click(h, mods.ctrl);
            return Some(ListOut::Repaint);
        }
        if self.leiste.close_field() {
            return Some(ListOut::Repaint);
        }
        match hot? {
            Hot::Umfang(_)
            | Hot::Kopf
            | Hot::BauherrFehlt
            | Hot::Mehr
            | Hot::Untertitel
            | Hot::Feld(_) => None,
            Hot::Knopf => {
                self.knopf_down = true;
                Some(ListOut::Repaint)
            }
            Hot::Preise(b) => (b != self.preise).then(|| {
                self.preise = b;
                ListOut::Repaint
            }),
            Hot::Reiter(r) => (r != self.reiter).then(|| {
                self.reiter = r;
                ListOut::Repaint
            }),
            Hot::Schliessen => {
                self.schliessen();
                self.clamp();
                Some(ListOut::Repaint)
            }
            Hot::Baum(i) => {
                match self.baum.get(i)?.clone() {
                    Knoten::Los { guid, .. } => {
                        if self.los == Some(guid) && self.ansicht == Ansicht::Lv {
                            // Auf- und zuklappen
                            if let Some(j) = self.offen.iter().position(|g| *g == guid) {
                                self.offen.remove(j);
                            } else {
                                self.offen.push(guid);
                            }
                        } else {
                            self.waehle_los(guid);
                            if !self.offen.contains(&guid) {
                                self.offen.push(guid);
                            }
                        }
                    }
                    Knoten::Titel { los, guid, .. } => {
                        self.waehle_los(los);
                        self.sprung = Some(Sprung::Titel(guid));
                    }
                    Knoten::Zusammenstellung => self.zeige(Ansicht::Zusammenstellung),
                    Knoten::Pruefen(_) => self.zeige(Ansicht::Pruefen),
                    Knoten::Trenner => return None,
                }
                Some(ListOut::Repaint)
            }
            Hot::Zeile(i) => {
                let z = self.zeilen.get(i)?.clone();
                match z.art {
                    Art::Position if self.ansicht == Ansicht::Lv => {
                        self.gewaehlt = Some(z.oz.clone());
                        p.selected = z.elemente.clone();
                        self.selected = p.selected.clone();
                        Some(ListOut::Picking { selection: true })
                    }
                    Art::Befund(_) => {
                        let oz = z.ziel?;
                        self.zeige(Ansicht::Lv);
                        self.gewaehlt = Some(oz.clone());
                        self.sprung = Some(Sprung::Oz(oz));
                        Some(ListOut::Repaint)
                    }
                    _ => None,
                }
            }
        }
    }

    fn waehle_los(&mut self, los: Guid) {
        if self.los != Some(los) {
            self.los = Some(los);
            self.gewaehlt = None;
            self.scroll = 0.0;
        }
        self.zeige(Ansicht::Lv);
    }

    fn zeige(&mut self, a: Ansicht) {
        if self.ansicht != a {
            self.ansicht = a;
            self.scroll = 0.0;
            if a != Ansicht::Lv {
                self.schliessen();
            }
        }
    }

    pub fn wheel(&mut self, delta: f64, t: &Theme) -> Option<ListOut> {
        let vorher = self.scroll;
        self.scroll -= delta as f32 * 3.0 * t.size.qto_row;
        self.clamp();
        (self.scroll != vorher).then_some(ListOut::Repaint)
    }

    pub fn tick(&mut self, t: &Theme, now: Instant) -> bool {
        self.leiste.tick(t, now)
    }

    pub fn overlay_open(&self) -> bool {
        self.leiste.overlay_open()
    }

    pub fn tip_at(&self, t: &Theme, fonts: &Fonts, x: f64, y: f64) -> Option<String> {
        match self.hit(t, fonts, x, y)? {
            Hot::Umfang(h) => self.leiste.tip(h),
            Hot::Zeile(i) => {
                let z = &self.zeilen[i];
                (z.art == Art::Position && z.text.chars().count() > 60).then(|| z.text.clone())
            }
            Hot::Baum(i) => match self.baum.get(i)? {
                Knoten::Pruefen(n) if *n > 0 => Some(format!("{n} offene Punkte im LV")),
                _ => None,
            },
            _ => None,
        }
    }

    // --- Zeichnen ------------------------------------------------------------

    pub fn paint(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, now: Instant) {
        let s = self.scale;
        let u = &t.ui;
        let top = self.top_px();
        c.fill_rect(0.0, top, self.w as f32, self.h as f32 - top, u.sheet_bg);
        let Some(regular) = fonts.regular.as_ref() else {
            return;
        };
        let bold = fonts.bold.as_ref().unwrap_or(regular);
        let (x0, cw) = self.content_x(t);
        // Zuerst die Zeilen, dann darüber und darunter die Fläche (gerollte
        // Zeilen verschwinden unter Kopf und Detail)
        self.paint_zeilen(c, t, regular, bold);
        let (tx, r) = self.tabelle_x(t);
        let (oben, unten) = self.liste_y();
        c.fill_rect(
            tx - 10.0 * s,
            top,
            r - tx + 20.0 * s,
            oben - top,
            u.sheet_bg,
        );
        c.fill_rect(
            tx - 10.0 * s,
            unten,
            r - tx + 20.0 * s,
            self.h as f32 - unten,
            u.sheet_bg,
        );
        bold.draw(
            c,
            "Leistungsverzeichnis",
            19.0 * s,
            x0,
            top + TITLE_Y * s,
            u.sheet_text,
        );
        let sub = sk_ui::widgets::ellipsize(Some(regular), &self.subtitle, 10.5 * s, cw);
        regular.draw(c, &sub, 10.5 * s, x0, top + SUB_Y * s, u.sheet_text_dim);
        self.leiste.paint(c, t, fonts, self.leiste_lage(t), now);
        self.paint_schalter(c, t, regular, bold);
        self.paint_kopf(c, t, regular, bold);
        self.paint_knopf(c, t, fonts);
        let linie = self.body_top() - 8.0 * s;
        c.fill_rect(x0, linie, cw, s.max(1.0), u.sheet_rule);
        self.paint_baum(c, t, regular, bold);
        self.paint_tabelle(c, t, regular, bold);
        self.paint_detail(c, t, fonts, regular, bold);
        self.leiste
            .paint_field_list(c, t, fonts, self.leiste_lage(t));
        self.paint_mehr(c, t, fonts);
    }

    fn paint_schalter(&self, c: &mut Canvas, t: &Theme, regular: &Font, bold: &Font) {
        let s = self.scale;
        let u = &t.ui;
        let px = 10.5 * s;
        let (lx, segs) = self.schalter_mit(t, regular, bold);
        let inset = 2.0 * s;
        let (first, last) = (segs[0].1, segs[1].1);
        let (x, y, h) = (first.0 - inset, first.1 - inset, first.3 + 2.0 * inset);
        let w = last.0 + last.2 + inset - x;
        let mut p = Path::new();
        p.rounded_rect(x, y, w, h, h * 0.5);
        c.fill(&p, u.sheet_tile);
        regular.draw(
            c,
            "Preise",
            px,
            lx,
            y + (h + regular.cap_height(px)) * 0.5,
            u.sheet_text_dim,
        );
        for (b, (sx, sy, sw, sh)) in segs {
            let text = if b {
                "Mit Preisen"
            } else {
                "Für Anfrage (leer)"
            };
            let (f, col) = if b == self.preise {
                let mut p = Path::new();
                p.rounded_rect(sx, sy, sw, sh, sh * 0.5);
                c.fill(&p, u.sheet_card);
                (bold, u.sheet_text)
            } else if self.hot == Some(Hot::Preise(b)) {
                (regular, u.sheet_text)
            } else {
                (regular, u.sheet_text_dim)
            };
            let tw = f.width(text, px);
            f.draw(
                c,
                text,
                px,
                sx + (sw - tw) * 0.5,
                sy + (sh + f.cap_height(px)) * 0.5,
                col,
            );
        }
    }

    /// [`AvaView::schalter`] mit vorhandenen Schriften.
    fn schalter_mit(&self, t: &Theme, regular: &Font, bold: &Font) -> (f32, [(bool, Rect); 2]) {
        let s = self.scale;
        let (_, r) = self.tabelle_x(t);
        let px = 10.5 * s;
        let w = |text: &str| bold.width(text, px) + 20.0 * s;
        let label_w = regular.width("Preise", px) + 10.0 * s;
        let y = self.top_px() + SWITCH_TOP * s;
        let inset = 2.0 * s;
        let mit = w("Mit Preisen");
        let ohne = w("Für Anfrage (leer)");
        let x_mit = r - inset - mit;
        let x_ohne = x_mit - ohne;
        let h = SWITCH_H * s - 2.0 * inset;
        (
            x_ohne - inset - label_w,
            [
                (false, (x_ohne, y + inset, ohne, h)),
                (true, (x_mit, y + inset, mit, h)),
            ],
        )
    }

    fn punkt_farbe(t: &Theme, p: Punkt) -> Rgba {
        match p {
            Punkt::Da => t.ui.text_same,
            Punkt::Fehlt => t.ui.accent,
            Punkt::Leer => t.ui.sheet_hint,
        }
    }

    fn paint_punkt(c: &mut Canvas, (x, y): (f32, f32), r: f32, col: Rgba) {
        let mut p = Path::new();
        p.rounded_rect(x - r, y - r, 2.0 * r, 2.0 * r, r);
        c.fill(&p, col);
    }

    fn paint_baum(&self, c: &mut Canvas, t: &Theme, regular: &Font, bold: &Font) {
        let s = self.scale;
        let u = &t.ui;
        let (x0, _) = self.content_x(t);
        let px = 11.0 * s;
        let mitte = |y: f32, h: f32, f: &Font| y + (h + f.cap_height(px)) * 0.5;
        regular.draw(
            c,
            "LV",
            10.0 * s,
            x0,
            self.body_top() + 16.0 * s,
            u.sheet_text_dim,
        );
        let rechts = x0 + TREE_W * s;
        for (i, (x, y, w, h)) in self.baum_lage(t) {
            let k = &self.baum[i];
            if self.hot == Some(Hot::Baum(i)) {
                c.fill_rect(x - 6.0 * s, y, w + 12.0 * s, h, u.sheet_hover);
            }
            let zahl = |c: &mut Canvas, n: usize, col: Rgba| {
                let z = n.to_string();
                let zw = regular.width(&z, 10.5 * s);
                regular.draw(
                    c,
                    &z,
                    10.5 * s,
                    rechts - 26.0 * s - zw,
                    mitte(y, h, regular),
                    col,
                );
            };
            match k {
                Knoten::Los {
                    name,
                    anzahl,
                    punkt,
                    offen,
                    guid,
                } => {
                    let gewaehlt = Some(*guid) == self.los && self.ansicht == Ansicht::Lv;
                    sk_ui::widgets::disclosure(
                        c,
                        x + 3.5 * s,
                        y + h * 0.5,
                        *offen,
                        u.sheet_text_dim,
                        s,
                    );
                    let col = if gewaehlt {
                        u.sheet_text
                    } else {
                        u.sheet_text_dim
                    };
                    bold.draw(c, name, px, x + 14.0 * s, mitte(y, h, bold), col);
                    zahl(c, *anzahl, u.sheet_text_dim);
                    Self::paint_punkt(
                        c,
                        (rechts - 8.0 * s, y + h * 0.5),
                        3.5 * s,
                        Self::punkt_farbe(t, *punkt),
                    );
                }
                Knoten::Titel {
                    nr,
                    name,
                    anzahl,
                    punkt,
                    ..
                } => {
                    let col = if *anzahl == 0 {
                        u.sheet_hint
                    } else {
                        u.sheet_text
                    };
                    let tx = x + 30.0 * s;
                    regular.draw(c, nr, px, tx, mitte(y, h, regular), u.sheet_text_dim);
                    let nw = 26.0 * s;
                    let name = sk_ui::widgets::ellipsize(
                        Some(regular),
                        name,
                        px,
                        rechts - 40.0 * s - (tx + nw),
                    );
                    regular.draw(c, &name, px, tx + nw, mitte(y, h, regular), col);
                    zahl(c, *anzahl, u.sheet_text_dim);
                    Self::paint_punkt(
                        c,
                        (rechts - 8.0 * s, y + h * 0.5),
                        3.5 * s,
                        Self::punkt_farbe(t, *punkt),
                    );
                }
                Knoten::Trenner => {
                    c.fill_rect(x, y + h * 0.5, w, s.max(1.0), u.sheet_rule);
                }
                Knoten::Zusammenstellung => {
                    let (f, col) = if self.ansicht == Ansicht::Zusammenstellung {
                        (bold, u.sheet_text)
                    } else {
                        (regular, u.sheet_text)
                    };
                    f.draw(c, "Zusammenstellung", px, x + 14.0 * s, mitte(y, h, f), col);
                }
                Knoten::Pruefen(n) => {
                    let (f, col) = if self.ansicht == Ansicht::Pruefen {
                        (bold, u.sheet_text)
                    } else {
                        (regular, u.sheet_text)
                    };
                    f.draw(c, "Prüfen", px, x + 14.0 * s, mitte(y, h, f), col);
                    if *n > 0 {
                        // Pille „⚠ n“: Dreieck gezeichnet, nicht aus der Schrift
                        let z = n.to_string();
                        let zpx = 10.0 * s;
                        let pw = bold.width(&z, zpx) + 26.0 * s;
                        let ph = 18.0 * s;
                        let (pxx, py) = (rechts - pw, y + (h - ph) * 0.5);
                        let mut p = Path::new();
                        p.rounded_rect(pxx, py, pw, ph, ph * 0.5);
                        c.fill(&p, Rgba(u.accent.0, u.accent.1, u.accent.2, 60));
                        let (dx, dy, d) = (pxx + 8.0 * s, py + 4.5 * s, 9.0 * s);
                        let col = crate::cards::verweis(u, false);
                        let mut tri = Path::new();
                        let st = s.max(1.0);
                        tri.segment((dx + d * 0.5, dy), (dx, dy + d), st);
                        tri.segment((dx, dy + d), (dx + d, dy + d), st);
                        tri.segment((dx + d, dy + d), (dx + d * 0.5, dy), st);
                        c.fill(&tri, col);
                        bold.draw(c, &z, zpx, pxx + 20.0 * s, mitte(py, ph, bold), col);
                    }
                }
            }
        }
        // Legende unten im Baum
        let ly = self.bottom() - 6.0 * s;
        let mut lx = x0;
        for (p, text) in [
            (Punkt::Da, "Preis da"),
            (Punkt::Fehlt, "Preis fehlt"),
            (Punkt::Leer, "leer"),
        ] {
            Self::paint_punkt(
                c,
                (lx + 3.5 * s, ly - 3.5 * s),
                3.5 * s,
                Self::punkt_farbe(t, p),
            );
            regular.draw(c, text, 10.0 * s, lx + 11.0 * s, ly, u.sheet_text_dim);
            lx += regular.width(text, 10.0 * s) + 30.0 * s;
        }
        // Senkrechte Linie zwischen Baum und Tabelle
        c.fill_rect(
            x0 + (TREE_W + TREE_GAP * 0.5) * s,
            self.body_top(),
            s.max(1.0),
            self.bottom() - self.body_top(),
            u.sheet_rule,
        );
    }

    fn paint_tabelle(&self, c: &mut Canvas, t: &Theme, regular: &Font, bold: &Font) {
        let s = self.scale;
        let u = &t.ui;
        let (tx, r) = self.tabelle_x(t);
        let px = 11.0 * s;
        let kopf = self.body_top() + 16.0 * s;
        let hpx = 10.0 * s;
        let rechts = |c: &mut Canvas, f: &Font, text: &str, px: f32, x: f32, y: f32, col: Rgba| {
            f.draw(c, text, px, x - f.width(text, px), y, col);
        };
        match self.ansicht {
            Ansicht::Lv => {
                regular.draw(c, "OZ", hpx, tx, kopf, u.sheet_text_dim);
                regular.draw(c, "Kurztext", hpx, tx + OZ_W * s, kopf, u.sheet_text_dim);
                rechts(
                    c,
                    regular,
                    "Menge",
                    hpx,
                    r - MENGE_R * s,
                    kopf,
                    u.sheet_text_dim,
                );
                regular.draw(c, "Einheit", hpx, r - EINHEIT_L * s, kopf, u.sheet_text_dim);
                rechts(c, regular, "EP", hpx, r - EP_R * s, kopf, u.sheet_text_dim);
                rechts(c, regular, "GP", hpx, r, kopf, u.sheet_text_dim);
            }
            Ansicht::Zusammenstellung => {
                bold.draw(c, "Zusammenstellung", px, tx, kopf, u.sheet_text);
                rechts(c, regular, "Betrag", hpx, r, kopf, u.sheet_text_dim);
            }
            Ansicht::Pruefen => {
                bold.draw(c, "Prüfen", px, tx, kopf, u.sheet_text);
            }
        }
        let linie = self.body_top() + HEAD_ROW * s;
        c.fill_rect(tx, linie, r - tx, s.max(1.0), u.sheet_rule);
        if self.detail.is_none() {
            regular.draw(
                c,
                HINWEIS,
                10.0 * s,
                tx,
                self.bottom() - 6.0 * s,
                u.sheet_hint,
            );
        }
    }

    fn paint_zeilen(&self, c: &mut Canvas, t: &Theme, regular: &Font, bold: &Font) {
        let s = self.scale;
        let u = &t.ui;
        let (tx, r) = self.tabelle_x(t);
        let px = 11.0 * s;
        let rechts = |c: &mut Canvas, f: &Font, text: &str, px: f32, x: f32, y: f32, col: Rgba| {
            f.draw(c, text, px, x - f.width(text, px), y, col);
        };
        let leer_strich = |c: &mut Canvas, x_r: f32, base: f32| {
            c.fill_rect(
                x_r - 56.0 * s,
                base + 2.0 * s,
                56.0 * s,
                s.max(1.0),
                u.sheet_rule,
            );
        };
        for (i, y, h) in self.sichtbar() {
            let z = &self.zeilen[i];
            let base = y + (h + regular.cap_height(px)) * 0.5;
            let auswahl =
                !z.elemente.is_empty() && z.elemente.iter().any(|e| self.selected.contains(e));
            // Schwebt die Maus über einer Zeile, nur diese; sonst die
            // Zeilen der Bauteile unter der Maus im Modell
            let schwebt = !matches!(self.hot, Some(Hot::Zeile(_)))
                && !z.elemente.is_empty()
                && z.elemente.iter().any(|e| self.hover.contains(e));
            let gewaehlt = z.art == Art::Position
                && self.ansicht == Ansicht::Lv
                && self.gewaehlt.as_deref() == Some(z.oz.as_str());
            if auswahl || gewaehlt {
                c.fill_rect(tx - 8.0 * s, y, r - tx + 16.0 * s, h, u.sheet_select);
            } else if schwebt || self.hot == Some(Hot::Zeile(i)) && z.art != Art::Titel {
                c.fill_rect(tx - 8.0 * s, y, r - tx + 16.0 * s, h, u.sheet_hover);
            }
            if gewaehlt {
                c.fill_rect(tx - 8.0 * s, y, 3.0 * s, h, u.accent);
            }
            match z.art {
                Art::Titel => {
                    let base = y + h - 9.0 * s;
                    bold.draw(c, &z.oz, 12.0 * s, tx, base, u.sheet_text);
                    bold.draw(c, &z.text, 12.0 * s, tx + OZ_W * s, base, u.sheet_text);
                    rechts(c, bold, &z.gp, 12.0 * s, r, base, u.sheet_text);
                }
                Art::Untertitel => {
                    bold.draw(c, &z.oz, px, tx, base, u.sheet_text_dim);
                    bold.draw(c, &z.text, px, tx + OZ_W * s, base, u.sheet_text_dim);
                    rechts(c, bold, &z.gp, px, r, base, u.sheet_text_dim);
                }
                Art::Position => {
                    regular.draw(c, &z.oz, px, tx, base, u.sheet_text);
                    let kx = tx + OZ_W * s;
                    let platz = r - MENGE_R * s - 90.0 * s - kx;
                    let text = sk_ui::widgets::ellipsize(Some(regular), &z.text, px, platz);
                    regular.draw(c, &text, px, kx, base, u.sheet_text);
                    rechts(
                        c,
                        regular,
                        &z.menge,
                        px,
                        r - MENGE_R * s,
                        base,
                        u.sheet_text,
                    );
                    regular.draw(c, &z.einheit, px, r - EINHEIT_L * s, base, u.sheet_text_dim);
                    if self.preise {
                        rechts(c, regular, &z.ep, px, r - EP_R * s, base, u.sheet_text);
                        rechts(c, regular, &z.gp, px, r, base, u.sheet_text);
                    } else {
                        // Anfrage: leere Felder für den Bieter
                        leer_strich(c, r - EP_R * s, base);
                        leer_strich(c, r, base);
                    }
                }
                Art::Summe => {
                    regular.draw(c, &z.text, px, tx + OZ_W * s, base, u.sheet_text);
                    rechts(c, bold, &z.gp, px, r, base, u.sheet_text);
                }
                Art::Leise => {
                    regular.draw(c, &z.text, 10.5 * s, tx + OZ_W * s, base, u.sheet_text_dim);
                    rechts(c, regular, &z.gp, 10.5 * s, r, base, u.sheet_text_dim);
                }
                Art::Befund(schwere) => {
                    let col = match schwere {
                        Schwere::Hinweis => u.sheet_hint,
                        _ => u.accent,
                    };
                    Self::paint_punkt(c, (tx + 4.0 * s, y + h * 0.5), 3.5 * s, col);
                    let platz = r - tx - 140.0 * s;
                    let text = sk_ui::widgets::ellipsize(Some(regular), &z.text, px, platz);
                    regular.draw(c, &text, px, tx + 16.0 * s, base, u.sheet_text);
                    if z.ziel.is_some() {
                        let hot = self.hot == Some(Hot::Zeile(i));
                        let col = crate::cards::verweis(u, hot);
                        rechts(c, bold, "zur Position", 10.5 * s, r, base, col);
                    }
                }
            }
        }
    }

    fn paint_detail(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, regular: &Font, bold: &Font) {
        let (Some(d), Some((fl, reiter, zu))) = (&self.detail, self.detail_lage(t, fonts)) else {
            return;
        };
        let s = self.scale;
        let u = &t.ui;
        let (fx, fy, fw, fh) = fl;
        let mut p = Path::new();
        p.rounded_rect(fx, fy, fw, fh, t.size.corner_radius * s);
        c.fill(&p, u.sheet_tile);
        let px = 11.0 * s;
        let x = fx + 12.0 * s;
        let kopf = fy + 24.0 * s;
        let titel = format!("{}  {}", d.oz, d.kurztext);
        let platz = reiter.first().map_or(fw, |r| r.1 .0) - x - 16.0 * s;
        let titel = sk_ui::widgets::ellipsize(Some(bold), &titel, 12.0 * s, platz);
        bold.draw(c, &titel, 12.0 * s, x, kopf, u.sheet_text);
        // Reiter als Segmente
        if let (Some(a), Some(b)) = (reiter.first(), reiter.last()) {
            let (rx, ry, rh) = (a.1 .0 - 2.0 * s, a.1 .1 - 2.0 * s, a.1 .3 + 4.0 * s);
            let rw = b.1 .0 + b.1 .2 + 2.0 * s - rx;
            let mut p = Path::new();
            p.rounded_rect(rx, ry, rw, rh, rh * 0.5);
            c.fill(&p, u.sheet_bg);
        }
        for (re, (sx, sy, sw, sh)) in &reiter {
            let on = *re == self.reiter;
            if on {
                let mut p = Path::new();
                p.rounded_rect(*sx, *sy, *sw, *sh, sh * 0.5);
                c.fill(&p, u.sheet_card);
            }
            let f = if on { bold } else { regular };
            let col = if on || self.hot == Some(Hot::Reiter(*re)) {
                u.sheet_text
            } else {
                u.sheet_text_dim
            };
            let tw = f.width(re.label(), 10.5 * s);
            f.draw(
                c,
                re.label(),
                10.5 * s,
                sx + (sw - tw) * 0.5,
                sy + (sh + f.cap_height(10.5 * s)) * 0.5,
                col,
            );
        }
        let zcol = if self.hot == Some(Hot::Schliessen) {
            u.sheet_text
        } else {
            u.sheet_text_dim
        };
        regular.draw(c, "×", 14.0 * s, zu.0 + 6.0 * s, zu.1 + 15.0 * s, zcol);
        let inhalt = fy + 50.0 * s;
        let zeile = 20.0 * s;
        let r = fx + fw - 12.0 * s;
        match self.reiter {
            Reiter::Ansatz => {
                // Links Mengenansatz, rechts die Spalte „Preis“
                let preis_x = r - 300.0 * s;
                let spalten = [x, x + 110.0 * s, x + 330.0 * s, x + 350.0 * s];
                let menge_r = spalten[2];
                let hpx = 10.0 * s;
                for (k, h) in ["Geschoss", "Bauteile", "Menge", "Herkunft"]
                    .iter()
                    .enumerate()
                {
                    if k == 2 {
                        let w = regular.width(h, hpx);
                        regular.draw(c, h, hpx, menge_r - w, inhalt, u.sheet_text_dim);
                    } else {
                        regular.draw(c, h, hpx, spalten[k], inhalt, u.sheet_text_dim);
                    }
                }
                let mut y = inhalt + zeile;
                let max_zeilen = ((fy + fh - y - zeile) / zeile).floor().max(0.0) as usize;
                let herkunft_w = preis_x - 16.0 * s - spalten[3];
                for a in d.ansatz.iter().take(max_zeilen) {
                    let ausgleich = a[0].is_empty();
                    regular.draw(c, &a[0], px, spalten[0], y, u.sheet_text);
                    let mw = regular.width(&a[2], px);
                    let bw = menge_r - mw - 12.0 * s - spalten[1];
                    let bt = sk_ui::widgets::ellipsize(Some(regular), &a[1], px, bw);
                    regular.draw(c, &bt, px, spalten[1], y, u.sheet_text_dim);
                    let mcol = if ausgleich {
                        u.sheet_text_dim
                    } else {
                        u.sheet_text
                    };
                    regular.draw(c, &a[2], px, menge_r - mw, y, mcol);
                    let hk = sk_ui::widgets::ellipsize(Some(regular), &a[3], px, herkunft_w);
                    regular.draw(c, &hk, px, spalten[3], y, u.sheet_text_dim);
                    y += zeile;
                }
                if d.ansatz.len() > max_zeilen {
                    let mehr = format!("+ {} weitere", d.ansatz.len() - max_zeilen);
                    regular.draw(c, &mehr, 10.0 * s, spalten[1], y, u.sheet_hint);
                    y += zeile;
                }
                c.fill_rect(
                    spalten[0],
                    y - 14.0 * s,
                    menge_r - spalten[0],
                    s.max(1.0),
                    u.sheet_rule,
                );
                bold.draw(c, "Summe", px, spalten[0], y, u.sheet_text);
                let sw = bold.width(&d.summe, px);
                bold.draw(c, &d.summe, px, menge_r - sw, y, u.sheet_text);
                regular.draw(c, "Preis", hpx, preis_x, inhalt, u.sheet_text_dim);
                let mut py = inhalt + zeile;
                for l in &d.preis {
                    let l = sk_ui::widgets::ellipsize(Some(regular), l, 10.5 * s, r - preis_x);
                    regular.draw(c, &l, 10.5 * s, preis_x, py, u.sheet_text);
                    py += 18.0 * s;
                }
            }
            Reiter::Anteile | Reiter::Eigenschaften => {
                let liste = if self.reiter == Reiter::Anteile {
                    &d.anteile
                } else {
                    &d.eigenschaften
                };
                let mut y = inhalt;
                for (k, v) in liste {
                    regular.draw(c, k, px, x, y, u.sheet_text_dim);
                    let v = sk_ui::widgets::ellipsize(Some(regular), v, px, r - x - 160.0 * s);
                    regular.draw(c, &v, px, x + 160.0 * s, y, u.sheet_text);
                    y += zeile;
                    if y > fy + fh - 8.0 * s {
                        break;
                    }
                }
            }
        }
    }
}

// --- Zeilen bauen ------------------------------------------------------------

/// Menge mit drei Stellen ohne Einheit: „1.234,560“.
fn menge_zahl(d: Dez) -> String {
    let milli = d.0 / 1000;
    let a = milli.unsigned_abs();
    format!(
        "{}{},{:03}",
        if milli < 0 { "−" } else { "" },
        tausender(&(a / 1000).to_string()),
        a % 1000
    )
}

/// Bauteilnummern kurz: eine durchgehende Reihe gleicher Art als
/// „AW-001 … 004“, sonst mit Komma.
fn nummern_kurz(n: &[String]) -> String {
    let teil = |x: &str| {
        let i = x.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
        let (a, z) = x.split_at(i);
        Some((a.to_string(), z.to_string(), z.parse::<u64>().ok()?))
    };
    let t: Option<Vec<_>> = n.iter().map(|x| teil(x)).collect();
    if let Some(t) = t.filter(|t| t.len() > 2) {
        let reihe = t
            .windows(2)
            .all(|w| w[0].0 == w[1].0 && w[1].2 == w[0].2 + 1);
        if reihe {
            return format!("{} … {}", n[0], t[t.len() - 1].1);
        }
    }
    n.join(", ")
}

fn betrag(c: Option<Cent>) -> String {
    c.map_or_else(String::new, euro)
}

/// Tabelle des LV: Titel mit Summe, Untertitel, Positionen. Leere Titel
/// stehen nur im Baum.
pub fn lv_zeilen(lv: &Lv) -> Vec<Zeile> {
    let mut v = Vec::new();
    for t in lv.titel.iter().filter(|t| !t.positionen.is_empty()) {
        let mut z = Zeile::neu(Art::Titel, t.nr.clone(), t.name.clone());
        z.gp = betrag(t.summe);
        z.titel = Some(t.guid);
        v.push(z);
        let mut uu = None;
        for p in &t.positionen {
            if p.untertitel.is_some() && p.untertitel != uu {
                uu = p.untertitel;
                if let Some(u) = t.untertitel.iter().find(|u| Some(u.nr) == uu) {
                    let mut z = Zeile::neu(Art::Untertitel, u.oz.clone(), u.name.clone());
                    z.gp = betrag(u.summe);
                    z.titel = Some(t.guid);
                    v.push(z);
                }
            }
            let mut z = Zeile::neu(Art::Position, p.oz.clone(), p.kurztext.clone());
            z.menge = menge_zahl(p.menge);
            z.einheit = p.einheit.zeichen().to_string();
            z.ep = p.ep.map_or_else(|| "–".to_string(), euro);
            z.gp = p.gp.map_or_else(|| "–".to_string(), euro);
            z.titel = Some(t.guid);
            for a in &p.ansatz {
                for e in &a.elemente {
                    if !z.elemente.contains(e) {
                        z.elemente.push(*e);
                    }
                }
            }
            v.push(z);
        }
    }
    v
}

/// Zusammenstellung: Titelsummen, netto, MwSt. und brutto; ohne Preise
/// bleiben die Beträge leer.
pub fn zusammenstellung(lv: &Lv) -> Vec<Zeile> {
    let z = &lv.zusammenstellung;
    let mut v: Vec<Zeile> = z
        .zeilen
        .iter()
        .map(|(nr, name, summe)| {
            let mut x = Zeile::neu(Art::Position, nr.clone(), name.clone());
            x.gp = betrag(*summe);
            x
        })
        .collect();
    let satz = z.mwst_satz.text().replace('.', ",");
    for (text, wert) in [
        ("Summe netto".to_string(), z.netto),
        (format!("MwSt. {satz} %"), z.mwst),
        ("Summe brutto".to_string(), z.brutto),
    ] {
        let mut x = Zeile::neu(Art::Summe, "", text);
        x.gp = betrag(wert);
        v.push(x);
    }
    if let Some(g) = z.geschaetzt {
        let mut x = Zeile::neu(
            Art::Leise,
            "",
            "nicht ausgeschrieben (geschätzt), nicht in der Summe",
        );
        x.gp = euro(g);
        v.push(x);
    }
    if z.unvollstaendig {
        v.push(Zeile::neu(
            Art::Leise,
            "",
            "Unvollständig: Nicht jede Position hat einen Preis (siehe Prüfen).",
        ));
    }
    v
}

/// Prüfen: offene Punkte zuerst, dann Hinweise; ein Punkt mit Position
/// führt dorthin.
pub fn pruefen(lv: &Lv) -> Vec<Zeile> {
    let (offen, hinweise): (Vec<_>, Vec<_>) = lv
        .befunde
        .iter()
        .partition(|b| b.schwere != Schwere::Hinweis);
    let mut v = Vec::new();
    for (titel, liste) in [("Offen", offen), ("Hinweise", hinweise)] {
        if liste.is_empty() {
            continue;
        }
        v.push(Zeile::neu(
            Art::Untertitel,
            "",
            format!("{titel} ({})", liste.len()),
        ));
        for b in liste {
            let mut z = Zeile::neu(Art::Befund(b.schwere), "", b.satz.clone());
            if let Ort::Position(oz) = &b.ort {
                z.ziel = Some(oz.clone());
            }
            v.push(z);
        }
    }
    if v.is_empty() {
        v.push(Zeile::neu(Art::Leise, "", "Nichts zu prüfen."));
    }
    v
}

/// Detailbereich der Position `oz`.
pub fn detail(m: &Model, kat: &Katalog, lv: &Lv, oz: &str) -> Option<Detail> {
    let p = lv
        .titel
        .iter()
        .flat_map(|t| &t.positionen)
        .find(|p| p.oz == oz)?;
    let einheit = p.einheit.zeichen();
    let mut elemente = Vec::new();
    let ansatz = p
        .ansatz
        .iter()
        .map(|a| {
            for e in &a.elemente {
                if !elemente.contains(e) {
                    elemente.push(*e);
                }
            }
            let menge = format!("{} {einheit}", menge_zahl(a.menge));
            if a.elemente.is_empty() && a.nummern.is_empty() {
                // Rundungsausgleich: ohne Geschoss, Text unter „Bauteile“
                return [String::new(), a.herkunft.clone(), menge, String::new()];
            }
            [
                geschoss_name(m, a.geschoss),
                nummern_kurz(&a.nummern),
                menge,
                a.herkunft.clone(),
            ]
        })
        .collect();
    let (gewerk, din, _) = gewerk_name(m, Some(p.gewerk));
    let gewerk = if din.is_empty() {
        gewerk
    } else {
        format!("{gewerk} ({din})")
    };
    let mut preis = Vec::new();
    let mut anteile = Vec::new();
    if let Some(a) = &p.anteile {
        let lohnsatz = kat.werte.lohn;
        let stunden = a.stunden.text().replace('.', ",");
        let mut teile = vec![format!(
            "Lohn {stunden} h × {} €/h = {} €",
            euro(lohnsatz.cent()),
            euro(a.lohn)
        )];
        teile.push(format!("Stoff {} €", euro(a.stoff)));
        if a.geraet != Cent::NULL {
            teile.push(format!("Gerät {} €", euro(a.geraet)));
        }
        if a.sonst != Cent::NULL {
            teile.push(format!("Sonstiges {} €", euro(a.sonst)));
        }
        preis.push(teile.join(" · "));
        if let Some(ep) = p.ep {
            preis.push(format!("EP {} €/{einheit}", euro(ep)));
        }
        let ep = p.ep.unwrap_or(Cent::NULL);
        let mut zeile = |name: &str, c: Cent| {
            let pct = prozent(c, ep).map_or_else(String::new, |x| format!(" ({x} %)"));
            anteile.push((name.to_string(), format!("{} €/{einheit}{pct}", euro(c))));
        };
        zeile("Lohn", a.lohn);
        zeile("Stoff", a.stoff);
        zeile("Gerät", a.geraet);
        zeile("Sonstiges", a.sonst);
        if let Some(nu) = a.nu {
            zeile("Nachunternehmer", nu);
        }
        anteile.push(("Aufwandswert".into(), format!("{stunden} h/{einheit}")));
    } else if p.mehrere_preise {
        preis.push("Mehrere Preise je Dicke, kein EP (siehe Prüfen).".into());
        anteile.push(("Preis".into(), "mehrere Preise je Dicke".into()));
    } else {
        preis.push("Ohne Preise (Anfrage).".into());
        anteile.push(("Preis".into(), "ohne Preise (Anfrage)".into()));
    }
    preis.push(format!("Gewerk {gewerk}"));
    if p.kg.is_some() {
        preis.push(format!("Kostengruppe {}", kg_name(p.kg)));
    }
    let bauleistung = kat
        .leistung(p.quelle)
        .map_or_else(String::new, |l| l.kurz.clone());
    let eigenschaften = vec![
        ("OZ".into(), p.oz.clone()),
        ("Positionsart".into(), p.art.name().to_string()),
        ("Einheit".into(), einheit.to_string()),
        ("Gewerk".into(), gewerk),
        ("Kostengruppe".into(), kg_name(p.kg)),
        ("Bauleistung".into(), bauleistung),
        ("Bauteile".into(), elemente.len().to_string()),
    ];
    Some(Detail {
        oz: p.oz.clone(),
        kurztext: p.kurztext.clone(),
        ansatz,
        summe: format!("{} {einheit}", menge_zahl(p.menge)),
        preis,
        anteile,
        eigenschaften,
        elemente,
    })
}

#[cfg(test)]
mod abnahme_ka4b;
#[cfg(test)]
mod abnahme_ka4cd;
#[cfg(test)]
mod bild;
mod csv;
mod kopf;
#[cfg(test)]
mod tests;

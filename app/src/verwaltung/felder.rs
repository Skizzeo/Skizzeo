//! Grundstufe der Verwaltung (KA-3a2, Einstellungen §3 KA-3 Punkte 4–6,
//! soll-ka-3): Feldzeilen je Satzart als Liste von Teilen in dip. Zeichnen
//! und Treffer nehmen dieselbe Liste.

use super::{baum::zeit_text, firmenwert, komma, Aktion, Knoten, Verwaltung, Ziel};
use sk_cost::katalog::{Artikel, Leistung, Los};
use sk_cost::preis::Aufbau;
use sk_cost::{Cent, SatzId};
use sk_model::Guid;
use sk_paint::{Canvas, Path, Rgba};
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, ButtonState, FieldState, Fonts, Rect};

/// Eingabefeld der Grundstufe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feld {
    Kurz,
    Stunden,
    Geraet,
    Sonst,
    Nu,
    Menge(u32),
    Preis(Guid),
    /// Vorgeschlagene Stück je Einheit (Regel 108), vor „Bestätigen“.
    Conv(Guid),
    Wert(String),
}

/// Farbrolle eines Teils (Einstellungen §3 KA-3 Punkt 5: keine neuen
/// Rollen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Farbe {
    Text,
    Dim,
    Leise,
    Akzent,
    Fehler,
    Lohn,
    Stoff,
    Geraet,
    Sonst,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Art {
    Text {
        text: String,
        px: f32,
        fett: bool,
        farbe: Farbe,
        durch: bool,
    },
    Feld {
        feld: Feld,
        text: String,
        einheit: String,
    },
    Lese(String),
    Knopf(String),
    Pille(String),
    /// Ein Teil eines Segments; `an`: gewählt.
    Seg {
        text: String,
        an: bool,
    },
    Flaeche(Farbe),
    /// Gezeichneter Pfeil hinter „Mehr“ (nicht jede Schrift hat ▸ und ▾).
    Klapp(bool),
    Linie,
}

/// Ein Teil der Grundstufe; `r` in dip ab der linken oberen Ecke.
#[derive(Clone, Debug, PartialEq)]
pub struct Teil {
    pub r: Rect,
    pub art: Art,
    pub ziel: Option<Ziel>,
}

/// Schriftgrößen (dip).
const PX: f32 = 13.0;
const KLEIN: f32 = 12.0;
const GROSS: f32 = 17.0;
/// Höhe einer Textzeile und Abstand der Feldzeilen.
const TZ: f32 = 20.0;
const ABSTAND: f32 = 40.0;
/// Linke Kante der Werte.
const WERT_X: f32 = 170.0;

struct Bau<'a> {
    f: &'a Fonts,
    teile: Vec<Teil>,
    w: f32,
}

impl Bau<'_> {
    fn breite(&self, text: &str, px: f32, fett: bool) -> f32 {
        let font = if fett {
            self.f.bold.as_ref().or(self.f.regular.as_ref())
        } else {
            self.f.regular.as_ref()
        };
        font.map_or(text.chars().count() as f32 * px * 0.55, |f| {
            f.width(text, px)
        })
    }

    fn kurz(&self, text: &str, px: f32, fett: bool, max: f32) -> String {
        let font = if fett {
            self.f.bold.as_ref().or(self.f.regular.as_ref())
        } else {
            self.f.regular.as_ref()
        };
        widgets::ellipsize(font, text, px, max.max(10.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn text_art(
        &mut self,
        x: f32,
        y: f32,
        text: String,
        px: f32,
        fett: bool,
        farbe: Farbe,
        durch: bool,
        ziel: Option<Ziel>,
    ) -> f32 {
        let w = self.breite(&text, px, fett);
        self.teile.push(Teil {
            r: Rect::new(x, y, w, TZ),
            art: Art::Text {
                text,
                px,
                fett,
                farbe,
                durch,
            },
            ziel,
        });
        x + w
    }

    /// Text ab `x`; liefert die rechte Kante.
    fn text(
        &mut self,
        x: f32,
        y: f32,
        text: impl Into<String>,
        px: f32,
        fett: bool,
        farbe: Farbe,
    ) -> f32 {
        self.text_art(x, y, text.into(), px, fett, farbe, false, None)
    }

    /// Text rechtsbündig an `rechts`.
    fn rechts(
        &mut self,
        rechts: f32,
        y: f32,
        text: impl Into<String>,
        px: f32,
        fett: bool,
        farbe: Farbe,
    ) {
        let text = text.into();
        let w = self.breite(&text, px, fett);
        self.text_art(rechts - w, y, text, px, fett, farbe, false, None);
    }

    fn verweis(&mut self, x: f32, y: f32, text: impl Into<String>, a: Aktion) -> f32 {
        self.text_art(
            x,
            y,
            text.into(),
            PX,
            true,
            Farbe::Akzent,
            false,
            Some(Ziel::Aktion(a)),
        )
    }

    /// Bezeichnung links in der Feldzeile.
    fn label(&mut self, y: f32, text: &str) {
        self.text(0.0, y, text, PX, false, Farbe::Dim);
    }

    fn feld(&mut self, x: f32, y: f32, w: f32, feld: Feld, text: String, einheit: String) {
        self.teile.push(Teil {
            r: Rect::new(x, y - 6.0, w, 32.0),
            ziel: Some(Ziel::Feld(feld.clone())),
            art: Art::Feld {
                feld,
                text,
                einheit,
            },
        });
    }

    fn lese(&mut self, x: f32, y: f32, w: f32, text: String) {
        self.teile.push(Teil {
            r: Rect::new(x, y - 6.0, w, 32.0),
            art: Art::Lese(text),
            ziel: None,
        });
    }

    fn knopf(&mut self, x: f32, y: f32, text: &str, a: Aktion) {
        let w = self.breite(text, PX, true) + 32.0;
        self.teile.push(Teil {
            r: Rect::new(x, y - 6.0, w, 32.0),
            art: Art::Knopf(text.into()),
            ziel: Some(Ziel::Aktion(a)),
        });
    }

    fn pille(&mut self, x: f32, y: f32, text: &str) -> f32 {
        let w = self.breite(text, 11.0, true) + 24.0;
        self.teile.push(Teil {
            r: Rect::new(x, y - 1.0, w, 22.0),
            art: Art::Pille(text.into()),
            ziel: None,
        });
        x + w
    }

    fn flaeche(&mut self, r: Rect, farbe: Farbe) {
        self.teile.push(Teil {
            r,
            art: Art::Flaeche(farbe),
            ziel: None,
        });
    }

    fn linie(&mut self, y: f32) {
        self.teile.push(Teil {
            r: Rect::new(0.0, y, self.w, 1.0),
            art: Art::Linie,
            ziel: None,
        });
    }

    /// Überschrift eines Eintrags mit Zeile darunter; liefert das nächste y.
    fn kopf(&mut self, titel: &str, unter: &str) -> f32 {
        let t = self.kurz(titel, GROSS, true, self.w);
        self.text(0.0, 2.0, t, GROSS, true, Farbe::Text);
        if !unter.is_empty() {
            let u = self.kurz(unter, KLEIN, false, self.w);
            self.text(0.0, 34.0, u, KLEIN, false, Farbe::Dim);
        }
        86.0
    }

    /// Mehrzeiliger Text ab `x` bis zum rechten Rand; liefert das nächste y.
    fn absatz(&mut self, x: f32, y: f32, text: &str, farbe: Farbe) -> f32 {
        let zeilen = widgets::wrap(self.f.regular.as_ref(), text, PX, self.w - x);
        let mut y = y;
        for z in zeilen {
            self.text(x, y, z, PX, false, farbe);
            y += TZ;
        }
        y
    }
}

/// Unterkante der Teile (dip).
pub fn hoehe(teile: &[Teil]) -> f32 {
    teile.iter().map(|t| t.r.y + t.r.h).fold(0.0, f32::max)
}

/// „30,00 €“
fn euro(c: Cent) -> String {
    format!("{} €", c.deutsch())
}

impl Verwaltung {
    /// Die Grundstufe des gewählten Eintrags; `w` ist die Breite (dip).
    pub(super) fn seite(&self, fonts: &Fonts, w: f32) -> Vec<Teil> {
        let mut b = Bau {
            f: fonts,
            teile: Vec::new(),
            w,
        };
        let k = &self.jetzt;
        match self.wahl.clone() {
            Knoten::Firmenwerte => self.firmenwerte(&mut b),
            Knoten::Kennwort => self.kennwort_seite(&mut b),
            Knoten::Leistung(g) => match k.leistung(g) {
                Some(l) => self.leistung(&mut b, l),
                None => self.fehlt(&mut b),
            },
            Knoten::ArtikelSatz(g) => match k.artikel(g) {
                Some(a) => self.artikel(&mut b, a),
                None => self.fehlt(&mut b),
            },
            Knoten::Los(g) | Knoten::Titel(g) | Knoten::LosSatz(g) => match k.los(g) {
                Some(l) => self.los(&mut b, l),
                None => self.fehlt(&mut b),
            },
            Knoten::Typ(g) => self.typ(&mut b, g),
            Knoten::Haus(i) => self.haus(&mut b, i),
            Knoten::Stand(n) => self.stand(&mut b, n),
            Knoten::Artikel => {
                let n = k.artikel.iter().filter(|a| !a.retired).count();
                let ohne = k
                    .artikel
                    .iter()
                    .filter(|a| !a.retired && a.preis.is_none())
                    .count();
                let unter = match ohne {
                    0 => format!("{n} Artikel, alle mit Preis"),
                    o => format!("{n} Artikel, {o} ohne Preis"),
                };
                let y = b.kopf("Baustoffe und Preise", &unter);
                b.absatz(
                    0.0,
                    y,
                    "Preise gelten für neue Häuser und für dieses Haus. Einen Artikel wählst du links.",
                    Farbe::Dim,
                );
            }
            Knoten::Leistungen => {
                let n = k.leistungen.iter().filter(|l| !l.retired).count();
                let lose = k
                    .lose
                    .iter()
                    .filter(|l| l.parent.is_none() && !l.retired)
                    .count();
                let y = b.kopf(
                    "Bauleistungen",
                    &format!("{n} Bauleistungen in {lose} Losen"),
                );
                b.absatz(
                    0.0,
                    y,
                    "Eine Bauleistung bestimmt den Einheitspreis: Aufwandswert mal Verrechnungslohn, dazu die Stoffanteile. Welche Bauleistung eine Schicht bekommt, steht unter „Passt auf“.",
                    Farbe::Dim,
                );
            }
            Knoten::Lose => {
                let n = k.lose.iter().filter(|l| !l.retired).count();
                let y = b.kopf("Lose und Titel", &format!("{n} Lose und Titel"));
                b.absatz(
                    0.0,
                    y,
                    "Lose und Titel gliedern das Leistungsverzeichnis.",
                    Farbe::Dim,
                );
            }
            Knoten::Typen => {
                let y = b.kopf(
                    "Bauteiltypen",
                    &format!("{} Bauteiltypen im Firmenkatalog", self.typen().len()),
                );
                b.absatz(
                    0.0,
                    y,
                    "Die Typen pflegst du im Bauteilkatalog. Hier siehst du, welche Bauleistung jede Schicht bekommt.",
                    Farbe::Dim,
                );
            }
            Knoten::Haeuser => self.haeuser(&mut b),
            Knoten::Ablaeufe => self.ablaeufe_seite(&mut b),
            Knoten::Ablauf(g) => self.ablauf_seite(&mut b, g),
            Knoten::Protokoll => {
                let n = sk_cost::verwaltung::protokoll(self.freigegeben()).len();
                let unter = match n {
                    0 => "Noch keine Änderung".to_string(),
                    1 => "Eine Änderung".to_string(),
                    n => format!("{n} Änderungen"),
                };
                let y = b.kopf("Protokoll", &unter);
                let satz = if self.freigabe.is_some() {
                    "Jedes „Freigeben“ schreibt einen neuen Stand des Firmenkatalogs. Um eine Änderung zurückzunehmen, wähle ihren Stand und klicke „Diese Änderung zurücknehmen“; das kommt in den Entwurf. Gespeicherte Häuser behalten ihre Werte und zeigen oben „Für neue Häuser gilt …“."
                } else {
                    "Jedes OK schreibt einen neuen Stand des Firmenkatalogs. Um eine Änderung zurückzunehmen, wähle ihren Stand und klicke „Diese Änderung zurücknehmen“. Gespeicherte Häuser behalten ihre Werte und zeigen oben „Für neue Häuser gilt …“."
                };
                b.absatz(0.0, y, satz, Farbe::Dim);
            }
            Knoten::Papierkorb => {
                let n = k.artikel.iter().filter(|a| a.retired).count()
                    + k.leistungen.iter().filter(|l| l.retired).count()
                    + k.lose.iter().filter(|l| l.retired).count();
                let unter = match n {
                    0 => "leer".to_string(),
                    1 => "ein Eintrag".to_string(),
                    n => format!("{n} Einträge"),
                };
                let y = b.kopf("Papierkorb", &unter);
                b.absatz(
                    0.0,
                    y,
                    "Was im Papierkorb liegt, bieten neue Häuser nicht mehr an; wo es noch verwendet wird, rechnet es weiter. Am Eintrag holst du ihn zurück.",
                    Farbe::Dim,
                );
            }
        }
        if !self.ohne_firma {
            return b.teile;
        }
        // Ohne Firmenkatalog nur ansehen (Bedienbarkeit 13.4): Felder
        // lesen, Änderungsknöpfe weg
        b.teile
            .into_iter()
            .filter(|t| {
                !matches!(
                    t.ziel,
                    Some(Ziel::Aktion(
                        Aktion::Bestaetigen(_)
                            | Aktion::Ausmustern(_)
                            | Aktion::Wiederherstellen(_)
                            | Aktion::Zuruecknehmen(_)
                            | Aktion::AlsReferenz
                            | Aktion::ConvBestaetigen(_)
                    ))
                )
            })
            .map(|t| match t.art {
                Art::Feld { text, einheit, .. } => Teil {
                    r: t.r,
                    art: Art::Lese(if einheit.is_empty() {
                        text
                    } else {
                        format!("{text} {einheit}")
                    }),
                    ziel: None,
                },
                _ => t,
            })
            .collect()
    }

    fn fehlt(&self, b: &mut Bau) {
        b.kopf("Eintrag nicht gefunden", "");
    }

    /// Herkunft eines Satzes: Text und, bei unbestätigtem Import oder
    /// KI-Vorschlag, der Satz für „Bestätigen“ (Regel 88).
    fn herkunft(&self, rec: &'static str, g: Guid, quelle: &str) -> (String, Option<SatzId>) {
        let key = g.to_ifc();
        let werk = || {
            if quelle.is_empty() {
                format!("Skizzeo-Werksbestand {}", sk_cost::lesen::werksstand())
            } else {
                quelle.to_string()
            }
        };
        let Some(u) = self.jetzt.herkunft_von(rec, &key) else {
            return (werk(), None);
        };
        let datum = zeit_text(u.satz.text("date").unwrap_or_default());
        let src = u.satz.text("source").unwrap_or_default();
        let text = match u.kind.as_str() {
            "manual" => format!("von Hand geändert am {datum}"),
            "import" if src.is_empty() => format!("importiert am {datum}"),
            "import" => format!("importiert: {src}"),
            "ai" if src.is_empty() => "Vorschlag".to_string(),
            "ai" => format!("Vorschlag: {src}"),
            _ => werk(),
        };
        let offen = matches!(u.kind.as_str(), "import" | "ai") && !u.bestaetigt;
        (text, offen.then(|| SatzId::neu(rec, key)))
    }

    /// Zeile „Herkunft“ mit Pille und „Bestätigen“.
    fn herkunft_zeile(&self, b: &mut Bau, y: f32, rec: &'static str, g: Guid, quelle: &str) {
        b.label(y, "Herkunft");
        let (text, offen) = self.herkunft(rec, g, quelle);
        let t = b.kurz(&text, PX, false, b.w - WERT_X - 220.0);
        let x = b.text(WERT_X, y, t, PX, false, Farbe::Text);
        if let Some(satz) = offen {
            let x = b.pille(x + 12.0, y, "unbestätigt");
            b.verweis(x + 12.0, y, "Bestätigen", Aktion::Bestaetigen(satz));
        }
    }

    /// Feld mit „vorher …“ daneben.
    #[allow(clippy::too_many_arguments)]
    fn feld_zeile(&self, b: &mut Bau, y: f32, label: &str, feld: Feld, fw: f32, einheit: String) {
        b.label(y, label);
        let text = self.anzeige(&feld);
        b.feld(WERT_X, y, fw, feld.clone(), text, einheit);
        if let Some(v) = self.vorher_text(&feld) {
            b.text(WERT_X + fw + 14.0, y, v, KLEIN, false, Farbe::Leise);
        }
    }

    /// Preis mit Segment „je m² | je m³ | je Stück“ (KA-3a7, Regel 108):
    /// nur was für den Artikel umrechenbar ist; darunter leise die Rechnung
    /// bzw. der Vorschlag „Stück je m²“ mit „Bestätigen“.
    fn preis_zeile(&self, b: &mut Bau, mut y: f32, a: &Artikel) -> f32 {
        use sk_cost::einheit;
        let angebot = einheit::angebot(a);
        let je = self
            .je
            .as_ref()
            .filter(|j| j.artikel == a.guid && angebot.contains(&j.einheit))
            .map_or(a.einheit, |j| j.einheit);
        let fw = 150.0;
        let feld = Feld::Preis(a.guid);
        b.label(y, "Preis");
        b.feld(
            WERT_X,
            y,
            fw,
            feld.clone(),
            self.anzeige(&feld),
            format!("€/{}", je.zeichen()),
        );
        let mut x = WERT_X + fw + 12.0;
        if angebot.len() > 1 {
            for e in &angebot {
                let t = einheit::je_text(*e);
                let w = b.breite(&t, KLEIN, true) + 16.0;
                b.teile.push(Teil {
                    r: Rect::new(x, y + 1.0, w, 22.0),
                    art: Art::Seg {
                        text: t,
                        an: *e == je,
                    },
                    ziel: Some(Ziel::Aktion(Aktion::Je(*e))),
                });
                x += w + 3.0;
            }
            x += 12.0;
        }
        if let Some(v) = self.vorher_text(&feld) {
            b.text(x, y, v, KLEIN, false, Farbe::Leise);
        }
        y += 34.0;
        // Rechnung der gesammelten Eingabe, sonst wohin umgerechnet wird
        let eingabe = self.ops.iter().find_map(|o| match o {
            sk_cost::Op::PreisSetzen {
                artikel,
                preis: Some(p),
                eingabe,
                ..
            } if *artikel == a.guid && !eingabe.is_empty() => Some((eingabe.clone(), *p)),
            _ => None,
        });
        let vorschlag = (je == sk_cost::katalog::Einheit::St)
            .then(|| einheit::vorschlag(a))
            .flatten();
        if let Some(v) = vorschlag {
            let feld = Feld::Conv(a.guid);
            b.feld(
                WERT_X,
                y,
                90.0,
                feld.clone(),
                self.anzeige(&feld),
                String::new(),
            );
            let t = format!("{} ·", v.wofuer(a.einheit));
            let x = b.text(WERT_X + 102.0, y, t, KLEIN, false, Farbe::Dim);
            b.verweis(x + 6.0, y, "Bestätigen", Aktion::ConvBestaetigen(a.guid));
            y += 34.0;
        } else if let Some((t, p)) = eingabe {
            let t = format!(
                "{} = {} €/{}",
                t.trim_start_matches("eingegeben "),
                einheit::zahl(p, 2),
                a.einheit.zeichen()
            );
            b.text(WERT_X, y, t, KLEIN, false, Farbe::Leise);
            y += 28.0;
        } else if je != a.einheit {
            let t = format!("wird in €/{} umgerechnet", a.einheit.zeichen());
            b.text(WERT_X, y, t, KLEIN, false, Farbe::Leise);
            y += 28.0;
        }
        y + ABSTAND - 34.0
    }

    fn mehr_verweis(&self, b: &mut Bau, y: f32) -> f32 {
        let t = if self.mehr { "Weniger" } else { "Mehr" };
        let x = b.verweis(0.0, y, t, Aktion::Mehr);
        b.teile.push(Teil {
            r: Rect::new(x, y, 16.0, TZ),
            art: Art::Klapp(self.mehr),
            ziel: Some(Ziel::Aktion(Aktion::Mehr)),
        });
        y + ABSTAND
    }

    /// Knopf „In den Papierkorb“ bzw. „Wiederherstellen“.
    fn korb_knopf(&self, b: &mut Bau, y: f32, rec: &'static str, g: Guid, retired: bool) {
        let satz = SatzId::neu(rec, g.to_ifc());
        if retired {
            b.knopf(0.0, y, "Wiederherstellen", Aktion::Wiederherstellen(satz));
        } else {
            b.knopf(0.0, y, "In den Papierkorb", Aktion::Ausmustern(satz));
        }
    }

    fn firmenwerte(&self, b: &mut Bau) {
        let mut y = b.kopf(
            "Firmenwerte",
            "Gelten für neue Häuser und jedes Haus, das mit dem Firmenkatalog rechnet",
        );
        let mut keys: Vec<String> = sk_cost::katalog::RATEN
            .iter()
            .map(|r| r.0.to_string())
            .collect();
        for (art, _) in &self.jetzt.werte.stahl {
            let k = format!("steel.{art}");
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        let mut bewehrung = false;
        for key in keys {
            let einheit = match key.as_str() {
                "wage" => "€/h",
                "surcharge" | "vat" => "%",
                _ => "kg/m³",
            };
            let mut name = match key.as_str() {
                "surcharge" => "Zuschlag auf Stoff".to_string(),
                k => sk_cost::wort::firmenwert(k),
            };
            if firmenwert(&self.jetzt, &key).is_none() {
                continue;
            }
            // Zwischentitel „Bewehrungsgrad“, darunter nur das Bauteil, damit
            // der Name vor das Feld passt (Bedienbarkeit 14.3)
            if let Some(teil) = name.strip_prefix("Bewehrungsgrad ") {
                if !bewehrung {
                    bewehrung = true;
                    y += 8.0;
                    b.text(0.0, y, "Bewehrungsgrad", PX, true, Farbe::Text);
                    y += ABSTAND - 8.0;
                }
                name = teil.to_string();
            }
            self.feld_zeile(b, y, &name, Feld::Wert(key), 130.0, einheit.into());
            y += ABSTAND;
        }
        self.vorschlaege_zeigen(b, y);
    }

    /// „n Vorschläge aus Projekten“ (KA-3b4, paket-ka3b §1): je Vorschlag
    /// Satz und Feld, alt → neu, Projekt und Datum, „In den Entwurf“ und
    /// „Ablehnen“. Kein Vorschlag verschwindet still.
    fn vorschlaege_zeigen(&self, b: &mut Bau, mut y: f32) {
        let n = self.vorschlaege.len();
        if n == 0 {
            return;
        }
        y += 8.0;
        b.linie(y - 12.0);
        let titel = match n {
            1 => "Ein Vorschlag aus einem Projekt".to_string(),
            n => format!("{n} Vorschläge aus Projekten"),
        };
        b.text(0.0, y, titel, PX, true, Farbe::Text);
        y += TZ + 4.0;
        y = b.absatz(
            0.0,
            y,
            "„In den Entwurf“ macht den Wert mit „Freigeben“ an allen Plätzen gültig, „Ablehnen“ streicht den Vorschlag. Beides steht im Protokoll.",
            Farbe::Dim,
        ) + 12.0;
        for v in &self.vorschlaege {
            let satz = sk_cost::verwaltung::satz_name(&self.jetzt, &v.rec, &v.of);
            let was = match v.rec.as_str() {
                "rate" => String::new(),
                rec => format!(" · {}", sk_cost::wort::feld(Some(rec), &v.feld)),
            };
            let x = b.text(
                0.0,
                y,
                b.kurz(&satz, PX, true, b.w * 0.6),
                PX,
                true,
                Farbe::Text,
            );
            b.text(x, y, was, PX, false, Farbe::Dim);
            y += TZ;
            let wert = |t: &str| self.vorschlag_wert(v, t);
            let mut x = match &v.alt {
                Some(a) => {
                    let x = b.text_art(0.0, y, wert(a), PX, false, Farbe::Dim, true, None);
                    b.text(x + 8.0, y, "→", PX, false, Farbe::Dim) + 8.0
                }
                None => 0.0,
            };
            x = b.text(x, y, wert(&v.neu), PX, true, Farbe::Text);
            let woher = match v.name.as_str() {
                "" => format!(" · {}", zeit_text(&v.datum)),
                name => format!(" · aus {name}, {}", zeit_text(&v.datum)),
            };
            let woher = b.kurz(&woher, PX, false, (b.w - x - 200.0).max(40.0));
            b.text(x, y, woher, PX, false, Farbe::Dim);
            let x = b.verweis(
                b.w - 190.0,
                y,
                // nicht „Übernehmen“: das heißt in der Abgleichzeile Firma →
                // Haus (Bedienbarkeit 17)
                "In den Entwurf",
                Aktion::VorschlagUebernehmen(v.key),
            );
            b.verweis(x + 16.0, y, "Ablehnen", Aktion::VorschlagAblehnen(v.key));
            y += TZ + 14.0;
        }
    }

    /// Wert eines Vorschlags wie im Feld, mit Einheit („65,00 €/h“).
    fn vorschlag_wert(&self, v: &sk_cost::katalog::Vorschlag, t: &str) -> String {
        let Some(d) = sk_cost::Dez::lesen(t, 4) else {
            return t.to_string();
        };
        let g = Guid::from_ifc(&v.of);
        match v.rec.as_str() {
            "rate" => match v.of.as_str() {
                "wage" => format!("{} €/h", super::geld(d)),
                "surcharge" | "vat" => format!("{} %", komma(d)),
                _ => format!("{} kg/m³", komma(d)),
            },
            "article" => match g.and_then(|g| self.jetzt.artikel(g)) {
                Some(a) => format!("{} €/{}", super::geld(d), a.einheit.zeichen()),
                None => format!("{} €", super::geld(d)),
            },
            "service" => match g.and_then(|g| self.jetzt.leistung(g)) {
                Some(l) => format!("{} h/{}", komma(d), l.einheit.zeichen()),
                None => format!("{} h", komma(d)),
            },
            _ => komma(d),
        }
    }

    /// „Abläufe“ (KA-3b5): je Ablauf Name, Einleitung bzw. Befund und
    /// „Starten …“.
    fn ablaeufe_seite(&self, b: &mut Bau) {
        let n = self.verwaltungs_ablaeufe().count();
        let unter = match n {
            1 => "Ein geführter Ablauf".to_string(),
            n => format!("{n} geführte Abläufe"),
        };
        let y = b.kopf("Abläufe", &unter);
        let mut y = b.absatz(
            0.0,
            y,
            "Ein Ablauf fragt Schritt für Schritt und schreibt erst am Ende, als eine Änderung. Abläufe für ein einzelnes Haus stehen im Reiter Kosten.",
            Farbe::Dim,
        ) + 20.0;
        for a in self.verwaltungs_ablaeufe() {
            let grau = a.befund.is_some();
            let farbe = if grau { Farbe::Leise } else { Farbe::Text };
            let name = b.kurz(&a.name, PX, true, b.w - 130.0);
            b.text(0.0, y, name, PX, true, farbe);
            if !grau {
                b.verweis(b.w - 110.0, y, "Starten …", Aktion::AblaufStarten(a.guid));
            }
            y += TZ;
            let (unter, farbe) = match &a.befund {
                Some(bf) => (bf.satz.as_str(), Farbe::Fehler),
                None => (a.ask.as_str(), Farbe::Dim),
            };
            y = b.absatz(0.0, y, unter, farbe) + 14.0;
        }
    }

    /// Ein Ablauf: Einleitung, Schritte, „Starten …“ oder der Befund.
    fn ablauf_seite(&self, b: &mut Bau, g: Guid) {
        let Some(a) = self.ablauf(g) else {
            return self.fehlt(b);
        };
        let seiten = sk_cost::ablauf::seiten(a);
        let unter = match seiten.len() {
            1 => "Eine Seite".to_string(),
            n => format!("{n} Seiten"),
        };
        let mut y = b.kopf(&a.name, &unter);
        if !a.ask.is_empty() {
            y = b.absatz(0.0, y, &a.ask, Farbe::Dim) + 12.0;
        }
        for (i, _) in seiten.iter().enumerate() {
            let label = self.seiten_name(a, &seiten, i, &sk_cost::ablauf::Antworten::new());
            b.text(
                0.0,
                y,
                format!("{}  {label}", i + 1),
                PX,
                false,
                Farbe::Text,
            );
            y += TZ + 4.0;
        }
        y += 12.0;
        match &a.befund {
            Some(bf) => {
                b.absatz(0.0, y, &bf.satz, Farbe::Fehler);
            }
            None => b.knopf(0.0, y, "Starten …", Aktion::AblaufStarten(a.guid)),
        }
    }

    /// „Verwaltungskennwort“ (soll-ka-3c): Satz zum Ist-Zustand und Knopf.
    fn kennwort_seite(&self, b: &mut Bau) {
        let gesetzt = sk_cost::verwaltung::hat_kennwort(&self.lib0);
        let geplant = self.ops.iter().find_map(|o| match o {
            sk_cost::Op::KennwortSetzen { pw } => Some(pw.ist_leer()),
            _ => None,
        });
        // Mit Kennwort steht eine Änderung erst im Entwurf (KA-3b2)
        let im_entwurf = self
            .freigabe
            .as_ref()
            .is_some_and(|f| f.saetze.iter().any(|s| s.abschnitt == "catalog"));
        let satz = match (geplant, gesetzt) {
            (None, true) if im_entwurf => "Im Entwurf geändert: Das neue Kennwort gilt nach „Freigeben“, bis dahin das bisherige.",
            (None, false) if im_entwurf => "Im Entwurf entfernt: Nach „Freigeben“ arbeitet Skizzeo wieder als Einzelplatz.",
            (Some(true), _) => "Wird mit OK entfernt: Danach arbeitet Skizzeo wieder als Einzelplatz.",
            (Some(false), _) => "Wird mit OK gesetzt.",
            (None, true) => "Gesetzt: Änderungen sammeln sich in einem Entwurf. Erst „Freigeben“ macht sie gültig, an allen Plätzen.",
            (None, false) => "Nicht gesetzt: Skizzeo arbeitet als Einzelplatz, jede Änderung gilt mit OK.",
        };
        let y = b.kopf("Verwaltungskennwort", "");
        let y = b.absatz(0.0, y - 46.0, satz, Farbe::Dim) + 20.0;
        let knopf = if gesetzt || geplant == Some(false) {
            "Kennwort ändern …"
        } else {
            "Verwaltungskennwort setzen …"
        };
        b.knopf(0.0, y, knopf, Aktion::Kennwort);
    }

    fn leistung(&self, b: &mut Bau, l: &Leistung) {
        let k = &self.jetzt;
        let w = b.w;
        let e = l.einheit.zeichen();
        let fw = (w - 90.0).min(560.0);
        let kurz = self.feld_text(k, &Feld::Kurz).unwrap_or_default();
        b.teile.push(Teil {
            r: Rect::new(0.0, 0.0, fw, 36.0),
            ziel: Some(Ziel::Feld(Feld::Kurz)),
            art: Art::Feld {
                feld: Feld::Kurz,
                text: kurz,
                einheit: String::new(),
            },
        });
        let n = self
            .edit
            .as_ref()
            .filter(|ed| ed.feld == Feld::Kurz)
            .map_or(l.kurz.chars().count(), |ed| ed.te.text.chars().count());
        let max = sk_cost::verwaltung::KURZ_MAX;
        let farbe = if n >= max {
            Farbe::Fehler
        } else {
            Farbe::Leise
        };
        b.text(fw + 12.0, 8.0, format!("{n} / {max}"), KLEIN, false, farbe);
        // Pfad: Los › Titel · Gewerk · Kostengruppe · OZ
        let titel = k.los(l.titel);
        let los = titel.and_then(|t| t.parent).and_then(|p| k.los(p));
        let mut pfad = vec!["Bauleistung".to_string()];
        match (los, titel) {
            (Some(lo), Some(t)) => pfad.push(format!("{} › {}", lo.name, t.name)),
            (None, Some(t)) => pfad.push(t.name.clone()),
            _ => {}
        }
        if let Some(t) = self.m.trades().iter().find(|t| t.guid == l.gewerk) {
            pfad.push(format!("Gewerk {} (DIN {})", t.name, t.code));
        }
        if let Some(kg) = l.kg {
            pfad.push(format!("Kostengruppe {kg}"));
        }
        pfad.push(format!("OZ {}", k.oz_voll(l)));
        let p = b.kurz(&pfad.join(" · "), KLEIN, false, w);
        b.text(0.0, 50.0, p, KLEIN, false, Farbe::Dim);
        if l.retired {
            b.text(0.0, 72.0, "liegt im Papierkorb", KLEIN, true, Farbe::Akzent);
        }
        let mut y = 100.0;
        b.label(y, "Einheit");
        b.lese(WERT_X, y, 80.0, e.into());
        y += ABSTAND;
        self.feld_zeile(b, y, "Aufwandswert", Feld::Stunden, 130.0, format!("h/{e}"));
        y += ABSTAND;
        // Stoffanteile
        let a = sk_cost::verwaltung::aufbau(&self.m, k, l);
        b.label(y, "Stoffanteile");
        let anteile: Vec<_> = k.anteile_von(l.guid).collect();
        if anteile.is_empty() {
            b.text(WERT_X, y, "keine", PX, false, Farbe::Leise);
            y += ABSTAND;
        }
        let mut benutzt = vec![false; a.stoffe.len()];
        let fest: Vec<Guid> = anteile.iter().filter_map(|x| x.artikel).collect();
        let name_w = ((w - WERT_X) * 0.42).max(120.0);
        let menge_x = WERT_X + name_w + 8.0;
        let preis_r = w - 110.0;
        for an in &anteile {
            let teil = a.stoffe.iter().enumerate().position(|(i, t)| {
                !benutzt[i]
                    && match an.artikel {
                        Some(g) => t.artikel == Some(g),
                        None => t.haupt && t.artikel.is_none_or(|g| !fest.contains(&g)),
                    }
            });
            let name = match (teil, an.artikel) {
                (Some(i), _) => a.stoffe[i].name.clone(),
                (None, Some(g)) => k
                    .artikel(g)
                    .map_or("Artikel fehlt".into(), |x| x.name.clone()),
                (None, None) => "Baustoff der Schicht".into(),
            };
            let name = b.kurz(&name, PX, false, name_w);
            b.text(WERT_X, y, name, PX, false, Farbe::Text);
            let einheit = match an.artikel {
                Some(g) => k.artikel(g).map_or(e, |x| x.einheit.zeichen()),
                None => e,
            };
            let feld = Feld::Menge(an.nr);
            let text = self.feld_text(k, &feld).unwrap_or_default();
            b.feld(menge_x, y, 100.0, feld.clone(), text, einheit.into());
            match teil {
                Some(i) => {
                    benutzt[i] = true;
                    let t = &a.stoffe[i];
                    b.rechts(
                        preis_r,
                        y,
                        format!("{} €/{}", t.preis.cent().deutsch(), t.einheit.zeichen()),
                        KLEIN,
                        false,
                        Farbe::Dim,
                    );
                    b.rechts(w, y, euro(Aufbau::betrag(t)), PX, false, Farbe::Text);
                }
                None => b.rechts(w, y, "Preis fehlt", PX, false, Farbe::Fehler),
            }
            if let Some(v) = self.vorher_text(&feld) {
                b.text(menge_x, y + 24.0, v, KLEIN, false, Farbe::Leise);
                y += 14.0;
            }
            y += ABSTAND - 4.0;
        }
        y += 4.0;
        b.linie(y);
        y += 24.0;
        y = self.ep_balken(b, y, l, &a);
        b.linie(y);
        y += 24.0;
        // Passt auf, Verwendet in, Herkunft
        b.label(y, "Passt auf");
        let baustoff = l.mat.and_then(|g| {
            self.m
                .materials()
                .iter()
                .find(|(_, x)| x.guid == g)
                .map(|(_, x)| x.name.clone())
        });
        let pa = sk_cost::verwaltung::passt_auf(l, baustoff.as_deref());
        let pa = b.kurz(&pa, PX, false, w - WERT_X);
        b.text(WERT_X, y, pa, PX, false, Farbe::Text);
        y += 28.0;
        b.label(y, "Verwendet in");
        let typen = sk_cost::verwaltung::verwendet_in(&self.lib, k, l.guid);
        let mut x = WERT_X;
        for (i, (g, name)) in typen.iter().enumerate() {
            if i > 0 {
                x = b.text(x, y, ", ", PX, false, Farbe::Dim);
            }
            if x > w - 140.0 {
                b.text(
                    x,
                    y,
                    format!("und {} weitere", typen.len() - i),
                    PX,
                    false,
                    Farbe::Dim,
                );
                x = w;
                break;
            }
            x = b.verweis(x, y, name.clone(), Aktion::TypOeffnen(*g));
        }
        let haeuser: Vec<&str> = self
            .wirkung
            .haeuser
            .iter()
            .filter(|h| h.genutzt.contains(&l.guid))
            .map(|h| h.name.as_str())
            .collect();
        if !haeuser.is_empty() && x < w - 100.0 {
            let sep = if typen.is_empty() { "" } else { " · " };
            b.text(
                x,
                y,
                format!("{sep}{}", haeuser.join(", ")),
                PX,
                false,
                Farbe::Dim,
            );
        } else if typen.is_empty() {
            b.text(WERT_X, y, "keinem Bauteiltyp", PX, false, Farbe::Leise);
        }
        y += 28.0;
        self.herkunft_zeile(b, y, "service", l.guid, "");
        y += ABSTAND;
        y = self.mehr_verweis(b, y);
        if self.mehr {
            b.label(y, "Kostengruppe");
            let kg = l.kg.map_or("nach Bauteil".to_string(), |k| k.to_string());
            b.text(WERT_X, y, kg, PX, false, Farbe::Text);
            y += 28.0;
            b.label(y, "Mengenbezug");
            b.text(
                WERT_X,
                y,
                sk_cost::wort::bezug(l.bezug),
                PX,
                false,
                Farbe::Text,
            );
            y += ABSTAND;
            self.feld_zeile(b, y, "NU-Preis", Feld::Nu, 130.0, format!("€/{e}"));
            y += ABSTAND;
            b.label(y, "Folgepositionen");
            let folgen: Vec<_> = k.folgen_von(l.guid).collect();
            if folgen.is_empty() {
                b.text(WERT_X, y, "keine", PX, false, Farbe::Leise);
                y += 28.0;
            }
            for fo in folgen {
                let name = k
                    .leistung(fo.folge)
                    .map_or("Bauleistung fehlt", |x| x.kurz.as_str());
                let t = b.kurz(
                    &format!("{name} × {}", komma(fo.faktor)),
                    PX,
                    false,
                    w - WERT_X,
                );
                b.text(WERT_X, y, t, PX, false, Farbe::Text);
                y += 28.0;
            }
            y += 12.0;
            self.korb_knopf(b, y, "service", l.guid, l.retired);
        }
    }

    /// EP-Balken (Einstellungen §3 KA-3 Punkt 5); liefert das nächste y.
    fn ep_balken(&self, b: &mut Bau, y: f32, l: &Leistung, a: &Aufbau) -> f32 {
        let w = b.w;
        let e = l.einheit.zeichen();
        b.label(y, "Einheitspreis");
        b.rechts(
            w,
            y - 4.0,
            format!("{} €/{e}", a.ep.deutsch()),
            GROSS,
            true,
            Farbe::Text,
        );
        let lohn_text = format!(
            "Lohn {} h × {} €/h",
            komma(a.stunden),
            a.lohnsatz.cent().deutsch()
        );
        b.rechts(w, y + 24.0, lohn_text, KLEIN, false, Farbe::Dim);
        if a.ep.0 > 0 && a.nu.is_none() {
            let anteil = (a.lohn.0 as f64 / a.ep.0 as f64 * 100.0).round();
            b.rechts(
                w,
                y + 42.0,
                format!("Lohnanteil {anteil} %"),
                KLEIN,
                false,
                Farbe::Dim,
            );
        }
        let bx = WERT_X;
        let bw = (w - WERT_X - 200.0).max(80.0);
        let teile = [
            ("Lohn", a.lohn, Farbe::Lohn),
            ("Stoff", a.stoff, Farbe::Stoff),
            ("Gerät", a.geraet, Farbe::Geraet),
            ("Sonstiges", a.sonst, Farbe::Sonst),
        ];
        let summe: i64 = teile.iter().map(|t| t.1 .0.max(0)).sum();
        if let Some(nu) = a.nu {
            b.text(
                bx,
                y,
                format!("NU-Preis {} gilt statt Lohn und Stoff", euro(nu)),
                PX,
                false,
                Farbe::Dim,
            );
        } else if summe > 0 {
            let mut x = bx;
            for (_, c, farbe) in teile {
                if c.0 <= 0 {
                    continue;
                }
                let tw = bw * c.0 as f32 / summe as f32;
                b.flaeche(Rect::new(x, y + 4.0, (tw - 2.0).max(1.0), 12.0), farbe);
                x += tw;
            }
        }
        let mut x = bx;
        for (name, c, farbe) in teile {
            if c.0 == 0 {
                continue;
            }
            b.flaeche(Rect::new(x, y + 28.0, 8.0, 8.0), farbe);
            b.text(x + 14.0, y + 22.0, name, KLEIN, false, Farbe::Dim);
            b.text(x + 14.0, y + 40.0, euro(c), PX, true, Farbe::Text);
            x += 130.0;
        }
        let zeige = self.anteil || a.geraet.0 != 0 || a.sonst.0 != 0;
        if !zeige && x < w - 200.0 {
            b.verweis(x + 6.0, y + 30.0, "+ Anteil", Aktion::Anteil);
        }
        let mut y = y + 76.0;
        if zeige {
            self.feld_zeile(b, y, "Gerät", Feld::Geraet, 130.0, format!("€/{e}"));
            y += ABSTAND;
            self.feld_zeile(b, y, "Sonstiges", Feld::Sonst, 130.0, format!("€/{e}"));
            y += ABSTAND;
        }
        y
    }

    fn artikel(&self, b: &mut Bau, a: &Artikel) {
        let k = &self.jetzt;
        let mut unter = vec!["Artikel".to_string()];
        if let Some(g) = a.mat {
            if let Some((_, x)) = self.m.materials().iter().find(|(_, x)| x.guid == g) {
                unter.push(x.name.clone());
            }
        } else {
            unter.push("Hilfsstoff".into());
        }
        if let Some(t) = a.t {
            unter.push(format!("Dicke {} mm", komma(t)));
        }
        for (f, w) in [("grade", "Güte"), ("format", "Format")] {
            if let Some(v) = a.satz.text(f).filter(|v| !v.is_empty()) {
                unter.push(format!("{w} {v}"));
            }
        }
        let mut y = b.kopf(&a.name, &unter.join(" · "));
        if a.retired {
            b.text(0.0, 56.0, "liegt im Papierkorb", KLEIN, true, Farbe::Akzent);
        }
        y = self.preis_zeile(b, y, a);
        let text = |f: &str| a.satz.text(f).unwrap_or_default().to_string();
        b.label(y, "Preisstand");
        b.text(WERT_X, y, text("date"), PX, false, Farbe::Text);
        y += 28.0;
        b.label(y, "Quelle");
        let q = b.kurz(&text("source"), PX, false, b.w - WERT_X);
        b.text(WERT_X, y, q, PX, false, Farbe::Text);
        y += 28.0;
        b.label(y, "Steckt in");
        let auch = sk_cost::preis::auch_fuer(k, a.guid, Guid(0));
        let t = if auch.is_empty() {
            "keine Bauleistung".to_string()
        } else {
            auch.join(", ")
        };
        let farbe = if auch.is_empty() {
            Farbe::Leise
        } else {
            Farbe::Text
        };
        // Gekürzt mit dem ganzen Text als Tooltip (Bedienbarkeit 14)
        let kurz = b.kurz(&t, PX, false, b.w - WERT_X);
        let ziel = (kurz != t).then(|| Ziel::Tipp(t.clone()));
        b.text_art(WERT_X, y, kurz, PX, false, farbe, false, ziel);
        // „Herkunft“ nur, wenn sie mehr sagt als „Quelle“ (Bedienbarkeit 14)
        let (herkunft, offen) = self.herkunft("article", a.guid, &text("source"));
        if herkunft != text("source") || offen.is_some() {
            y += 28.0;
            self.herkunft_zeile(b, y, "article", a.guid, &text("source"));
        }
        y += ABSTAND;
        y = self.mehr_verweis(b, y);
        if self.mehr {
            b.label(y, "Standardartikel");
            b.text(
                WERT_X,
                y,
                if a.std { "ja" } else { "nein" },
                PX,
                false,
                Farbe::Text,
            );
            y += 28.0;
            if let Some(c) = a.conv {
                b.label(y, &format!("Stück je {}", a.einheit.zeichen()));
                b.text(WERT_X, y, komma(c), PX, false, Farbe::Text);
                y += 28.0;
            }
            y += 12.0;
            self.korb_knopf(b, y, "article", a.guid, a.retired);
        }
    }

    fn los(&self, b: &mut Bau, l: &Los) {
        let k = &self.jetzt;
        let unter = match l.parent.and_then(|p| k.los(p)) {
            Some(p) => format!("Titel {} in Los {} {}", l.nr, p.nr, p.name),
            None => format!("Los {}", l.nr),
        };
        let mut y = b.kopf(&format!("{} {}", l.nr, l.name), &unter);
        let n = k
            .leistungen
            .iter()
            .filter(|x| {
                !x.retired
                    && (x.titel == l.guid || k.los(x.titel).and_then(|t| t.parent) == Some(l.guid))
            })
            .count();
        b.label(y, "Bauleistungen");
        b.text(WERT_X, y, n.to_string(), PX, false, Farbe::Text);
        y += 28.0;
        if let Some(pre) = l.pre.as_deref().filter(|p| !p.is_empty()) {
            b.label(y, "Vorbemerkung");
            y = b.absatz(WERT_X, y, pre, Farbe::Text) + 8.0;
        }
        y += 12.0;
        y = self.mehr_verweis(b, y);
        if self.mehr {
            self.korb_knopf(b, y, "lot", l.guid, l.retired);
        }
    }

    fn typ(&self, b: &mut Bau, g: Guid) {
        use sk_model::element::Category;
        use sk_model::library::TypeCategory;
        let lib = &self.lib;
        let Some((_, t)) = lib.types.iter().find(|(_, t)| t.guid == g) else {
            self.fehlt(b);
            return;
        };
        let kat = Category::ALL
            .into_iter()
            .find(|c| TypeCategory::of(*c) == Some(t.category));
        let art = kat.map_or("Bauteiltyp".to_string(), |c| {
            sk_model::kinds::spec(c).name.to_string()
        });
        let mut y = b.kopf(&t.name, &format!("Bauteiltyp · {art}"));
        for (i, l) in t.layers.iter().enumerate() {
            let mat = lib.materials.get(l.material);
            let name = mat.map_or("Baustoff fehlt", |m| m.name.as_str());
            let schicht = format!(
                "{}. {name} {} mm",
                i + 1,
                komma(sk_cost::zuordnung::dicke(l.thickness))
            );
            let s = b.kurz(&schicht, PX, false, (b.w * 0.45).max(120.0));
            b.text(0.0, y, s, PX, false, Farbe::Text);
            let z = kat.map(|c| sk_cost::zuordnung::zuordnen(&self.jetzt, c, l, mat));
            let (text, farbe) = match z
                .as_ref()
                .and_then(|z| z.leistung.map(|g| (g, z.geschaetzt())))
            {
                Some((g, geschaetzt)) => {
                    let kurz = self.jetzt.leistung(g).map_or("?", |x| x.kurz.as_str());
                    if geschaetzt {
                        (format!("geschätzt nach {kurz}"), Farbe::Dim)
                    } else {
                        (kurz.to_string(), Farbe::Text)
                    }
                }
                None => ("ohne Bauleistung".to_string(), Farbe::Leise),
            };
            let x = (b.w * 0.45).max(120.0) + 16.0;
            let tx = b.kurz(&text, PX, false, b.w - x);
            b.text(x, y, tx, PX, false, farbe);
            y += 28.0;
        }
        y += 16.0;
        b.verweis(0.0, y, "Im Bauteilkatalog öffnen", Aktion::TypOeffnen(g));
    }

    fn haus(&self, b: &mut Bau, i: usize) {
        use super::wirkung::{aenderung, euro_ganz, FLAECHE_TIPP};
        let Some(h) = self.wirkung.haeuser.get(i) else {
            self.fehlt(b);
            return;
        };
        let unter = match &h.datei {
            None => "Referenzhaus des Werks · Standardhaus 1b".to_string(),
            Some(d) => format!("Eigenes Referenzhaus · {d}"),
        };
        let mut y = b.kopf(&h.name, &unter);
        if let Some(f) = &h.fehler {
            y = b.absatz(0.0, y, f, Farbe::Fehler) + 12.0;
            b.absatz(
                0.0,
                y,
                "Skizzeo ändert die Datei nicht. Öffne sie in Skizzeo und speichere sie neu, oder nimm sie aus dem Ordner.",
                Farbe::Dim,
            );
            return;
        }
        b.label(y, "Summe netto");
        self.vorher_nachher(b, WERT_X, y, euro_ganz(h.vorher), euro_ganz(h.nachher));
        y += ABSTAND;
        b.label(y, "Änderung");
        let a = if h.vorher == h.nachher {
            "keine".to_string()
        } else {
            aenderung(h.vorher, h.nachher)
        };
        b.text(WERT_X, y, a, PX, h.vorher != h.nachher, Farbe::Text);
        y += ABSTAND;
        let x = b.text_art(
            0.0,
            y,
            "€/m² Grundfläche (außen an der tragenden Wand)".into(),
            PX,
            false,
            Farbe::Dim,
            false,
            Some(Ziel::Tipp(FLAECHE_TIPP.into())),
        );
        let m2 = |c| {
            h.je_m2(c)
                .map_or("–".to_string(), |e| format!("{} €/m²", tausend(e)))
        };
        self.vorher_nachher(b, (x + 16.0).max(WERT_X), y, m2(h.vorher), m2(h.nachher));
        y += ABSTAND;
        b.label(y, "Grundfläche");
        let f = if h.flaeche > 0.0 {
            format!("{} m²", komma_2(h.flaeche / 1e6))
        } else {
            "– (kein geschlossener Außenwandzug)".to_string()
        };
        b.text(WERT_X, y, f, PX, false, Farbe::Text);
        y += ABSTAND;
        if let Some(t) = h.hinweis() {
            y = b.absatz(0.0, y, &t, Farbe::Dim) + 8.0;
        }
        b.absatz(
            0.0,
            y,
            "Das Referenzhaus rechnet immer mit dem Firmenkatalog samt deinen Änderungen, auch wenn es selbst andere Werte gespeichert hat.",
            Farbe::Dim,
        );
    }

    /// „alt → neu“ mit altem Wert durchgestrichen; gleich: nur der Wert fett.
    fn vorher_nachher(&self, b: &mut Bau, x: f32, y: f32, alt: String, neu: String) {
        if alt == neu {
            b.text(x, y, neu, PX, true, Farbe::Text);
            return;
        }
        let x = b.text_art(x, y, alt, PX, false, Farbe::Leise, true, None);
        let x = b.text(x + 10.0, y, "→", PX, false, Farbe::Text);
        b.text(x + 10.0, y, neu, PX, true, Farbe::Text);
    }

    /// Übersicht „Referenzhäuser“: je Haus Summe, Änderung und €/m².
    fn haeuser(&self, b: &mut Bau) {
        use super::wirkung::{aenderung, euro_ganz, FLAECHE_TIPP, IN_DER_ZEILE};
        let n = self.wirkung.haeuser.len();
        let mut y = b.kopf(
            "Referenzhäuser",
            &if n == 1 {
                "Das Standardhaus des Werks".to_string()
            } else {
                format!("{n} Häuser, an denen du die Wirkung einer Änderung siehst")
            },
        );
        let sp = [0.0, b.w * 0.34, b.w * 0.56, b.w * 0.78];
        b.text(sp[0], y, "Haus", KLEIN, false, Farbe::Dim);
        b.text(sp[1], y, "Summe netto", KLEIN, false, Farbe::Dim);
        b.text(sp[2], y, "Änderung", KLEIN, false, Farbe::Dim);
        b.text_art(
            sp[3],
            y,
            "€/m² Grundfläche".into(),
            KLEIN,
            false,
            Farbe::Dim,
            false,
            Some(Ziel::Tipp(FLAECHE_TIPP.into())),
        );
        y += 24.0;
        b.linie(y - 4.0);
        y += 8.0;
        for (i, h) in self.wirkung.haeuser.iter().enumerate() {
            let t = b.kurz(&h.name, PX, true, sp[1] - 16.0);
            if h.fehler.is_some() {
                b.text(sp[0], y, t, PX, true, Farbe::Leise);
                b.text(sp[1], y, "nicht lesbar", PX, false, Farbe::Fehler);
                y += 28.0;
                continue;
            }
            b.text_art(
                sp[0],
                y,
                t,
                PX,
                true,
                Farbe::Akzent,
                false,
                Some(Ziel::Aktion(Aktion::HausOeffnen(i))),
            );
            b.text(sp[1], y, euro_ganz(h.nachher), PX, false, Farbe::Text);
            let a = if h.vorher == h.nachher {
                "–".to_string()
            } else {
                aenderung(h.vorher, h.nachher)
            };
            b.text(sp[2], y, a, PX, false, Farbe::Text);
            let m2 = h
                .je_m2(h.nachher)
                .map_or("–".to_string(), |e| format!("{} €/m²", tausend(e)));
            b.text(sp[3], y, m2, PX, false, Farbe::Text);
            y += 28.0;
        }
        y += 16.0;
        if self.ordner.is_some() {
            b.knopf(
                0.0,
                y,
                "Aktuelles Haus als Referenzhaus",
                Aktion::AlsReferenz,
            );
            y += ABSTAND + 8.0;
        }
        b.absatz(
            0.0,
            y,
            &format!(
                "Eigene Referenzhäuser liegen als Dateien im Ordner „referenzhaeuser“ neben dem Firmenkatalog. Jedes rechnet mit dem Firmenkatalog samt deinen Änderungen. Die Zeile unten zeigt dieses Haus und die ersten {IN_DER_ZEILE}."
            ),
            Farbe::Dim,
        );
    }

    fn stand(&self, b: &mut Bau, n: u32) {
        // Das Protokoll beim Öffnen: gesammelte Änderungen sind noch kein Stand,
        // auch nicht der Entwurf
        let k = self.freigegeben();
        let st = sk_cost::verwaltung::protokoll(k);
        let Some(s) = st.iter().find(|s| s.stand == n) else {
            self.fehlt(b);
            return;
        };
        let wer = if s.rolle == "ai" {
            "Vorschlag der KI"
        } else {
            "von Hand"
        };
        let anzahl = match s.eintraege.len() {
            1 => "eine Zeile".to_string(),
            z => format!("{z} Zeilen"),
        };
        let mut y = b.kopf(
            &format!("Stand {n} · {}", zeit_text(&s.zeit)),
            &format!("Änderung am Firmenkatalog · {wer} · {anzahl}"),
        );
        let w_satz = (b.w * 0.42).min(300.0);
        let w_was = 130.0;
        for e in &s.eintraege {
            let name = if e.op == "werk_uebernommen" {
                "Werksbestand".to_string()
            } else {
                sk_cost::verwaltung::satz_name(k, &e.rec, &e.of)
            };
            let t = b.kurz(&name, PX, true, w_satz - 12.0);
            b.text(0.0, y, t, PX, true, Farbe::Text);
            for (was, alt, neu) in self.aenderungen(e) {
                let was = b.kurz(&was, PX, false, w_was - 12.0);
                b.text(w_satz, y, was, PX, false, Farbe::Dim);
                let x = w_satz + w_was;
                let rest = b.w - x;
                match (alt.is_empty(), neu.is_empty()) {
                    (false, false)
                        if b.breite(&alt, PX, false) + b.breite(&neu, PX, true) + 40.0 > rest =>
                    {
                        // Zu lang für eine Zeile: untereinander und umbrochen,
                        // damit die Änderung sichtbar bleibt (Bedienbarkeit 14.2)
                        let regular = b.f.regular.as_ref();
                        let fett = b.f.bold.as_ref().or(regular);
                        for z in widgets::wrap(regular, &alt, PX, rest) {
                            b.text_art(x, y, z, PX, false, Farbe::Dim, true, None);
                            y += TZ;
                        }
                        let pw = b.text(x, y, "→", PX, false, Farbe::Dim) + 8.0 - x;
                        for z in widgets::wrap(fett, &neu, PX, rest - pw) {
                            b.text(x + pw, y, z, PX, true, Farbe::Text);
                            y += TZ;
                        }
                        y -= TZ;
                    }
                    (false, false) => {
                        let x = b.text_art(x, y, alt, PX, false, Farbe::Dim, true, None);
                        let x = b.text(x + 8.0, y, "→", PX, false, Farbe::Dim);
                        b.text(x + 8.0, y, neu, PX, true, Farbe::Text);
                    }
                    (true, false) => {
                        let nt = b.kurz(&neu, PX, true, rest);
                        b.text(x, y, nt, PX, true, Farbe::Text);
                    }
                    (false, true) => {
                        let at = b.kurz(&format!("vorher {alt}"), PX, false, rest);
                        b.text(x, y, at, PX, false, Farbe::Dim);
                    }
                    (true, true) => {}
                }
                y += 28.0;
            }
        }
        b.linie(y - 8.0);
        y += 20.0;
        if self.zurueck == Some(n) {
            b.text(0.0, y, "Wird mit OK zurückgenommen", PX, true, Farbe::Text);
            y += ABSTAND + 8.0;
            b.absatz(
                0.0,
                y,
                "Die alten Werte stehen in den gesammelten Änderungen; die Summen unten zeigen die Wirkung. OK schreibt sie als neuen Stand, Abbrechen verwirft sie.",
                Farbe::Dim,
            );
        } else {
            b.knopf(
                0.0,
                y,
                "Diese Änderung zurücknehmen",
                Aktion::Zuruecknehmen(n),
            );
            y += ABSTAND + 8.0;
            let satz = if self.freigabe.is_some() {
                "Setzt die alten Werte dieses Stands in den Entwurf. Mit „Freigeben“ entsteht daraus ein neuer Stand; dieser hier bleibt im Protokoll."
            } else {
                "Setzt die alten Werte dieses Stands wieder ein. Mit OK entsteht daraus ein neuer Stand; dieser hier bleibt im Protokoll."
            };
            b.absatz(0.0, y, satz, Farbe::Dim);
        }
    }
}

/// Wort einer Protokollzeile (`[log] op`).
fn op_wort(op: &str) -> &'static str {
    match op {
        "artikel_anlegen" | "bauleistung_anlegen" | "los_anlegen" => "angelegt",
        "preis_setzen" => "Preis",
        "bauleistung_aendern" => "geändert",
        "stoffanteil_setzen" => "Stoffanteil",
        "folge_setzen" => "Folgeposition",
        "firmenwert_setzen" => "Firmenwert",
        "ausmustern" => "in den Papierkorb",
        "wiederherstellen" => "wiederhergestellt",
        "herkunft_bestaetigen" => "Herkunft bestätigt",
        "stand_uebernehmen" => "übernommen",
        "werk_uebernommen" => "übernommen",
        _ => "geändert",
    }
}

/// Felder aus `[log] old`/`new` (`kurz=AW Porenbeton d=24cm hours=0.45`,
/// Werte ohne Anführungszeichen): ein Paar beginnt bei einem bekannten
/// Feldnamen, alles andere gehört zum Wert davor. Ein einzelner Wert ohne
/// Feldnamen hat den Schlüssel „“.
fn paare(rec: &str, v: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for wort in v.split(' ') {
        let neu = wort
            .split_once('=')
            .filter(|(k, _)| sk_cost::wort::feld(Some(rec), k) != "Angabe");
        match (neu, out.last_mut()) {
            (Some((k, w)), _) => out.push((k.to_string(), w.to_string())),
            (None, Some(p)) => {
                p.1.push(' ');
                p.1.push_str(wort);
            }
            (None, None) => out.push((String::new(), wort.to_string())),
        }
    }
    out.retain(|p| !(p.0.is_empty() && p.1.is_empty()));
    out
}

impl Verwaltung {
    /// Zeilen einer Protokollzeile: Feldwort, alt, neu; Werte wie in der
    /// Vorschau mit Einheit und Feldwörter wie dort (Bedienbarkeit 14).
    fn aenderungen(&self, e: &sk_cost::verwaltung::Eintrag) -> Vec<(String, String, String)> {
        let (a, n) = (paare(&e.rec, &e.alt), paare(&e.rec, &e.neu));
        // „Werksbestand · übernommen · Stand 7“
        if e.op == "werk_uebernommen" {
            let stand = n.first().map_or(String::new(), |p| {
                p.1.trim_matches('"')
                    .trim_start_matches("Werksbestand ")
                    .to_string()
            });
            return vec![(op_wort(&e.op).into(), String::new(), stand)];
        }
        let mut keys: Vec<&str> = a.iter().map(|p| p.0.as_str()).collect();
        for (k, _) in &n {
            if !keys.contains(&k.as_str()) {
                keys.push(k);
            }
        }
        let von = |v: &[(String, String)], k: &str| {
            v.iter().find(|p| p.0 == k).map_or(String::new(), |p| {
                self.vorschau_wert(&e.rec, &e.of, k, p.1.trim_matches('"'))
            })
        };
        if keys.is_empty() {
            return vec![(op_wort(&e.op).into(), String::new(), String::new())];
        }
        keys.iter()
            .map(|k| {
                let was = if k.is_empty() {
                    op_wort(&e.op).to_string()
                } else {
                    super::entwurf::wort_art(&e.rec, k)
                };
                (was, von(&a, k), von(&n, k))
            })
            .collect()
    }
}

/// Farbe einer Rolle.
fn farbe(t: &Theme, f: Farbe) -> Rgba {
    let u = &t.ui;
    match f {
        Farbe::Text => u.text,
        Farbe::Dim | Farbe::Lohn => u.text_dim,
        Farbe::Leise | Farbe::Sonst => u.text_disabled,
        Farbe::Akzent | Farbe::Stoff => u.accent,
        Farbe::Fehler => u.field_invalid,
        Farbe::Geraet => u.border,
    }
}

/// Zeichnet die Teile der Grundstufe (`r` schon in Pixeln).
/// `oben`/`unten` (Pixel): was ganz außerhalb liegt, bleibt weg; Kopf und
/// Fuß decken den Rest ab.
#[allow(clippy::too_many_arguments)]
pub fn malen(
    c: &mut Canvas,
    t: &Theme,
    fonts: &Fonts,
    v: &Verwaltung,
    teile: &[(Rect, Teil)],
    s: f32,
    oben: f32,
    unten: f32,
) {
    let u = &t.ui;
    for (r, teil) in teile {
        if r.y >= unten || r.y + r.h <= oben {
            continue;
        }
        let hover = teil.ziel.is_some() && v.hover.as_ref() == teil.ziel.as_ref();
        match &teil.art {
            Art::Text {
                text,
                px,
                fett,
                farbe: f,
                durch,
            } => {
                let font = if *fett {
                    fonts.bold.as_ref().or(fonts.regular.as_ref())
                } else {
                    fonts.regular.as_ref()
                };
                let Some(font) = font else { continue };
                let px = px * s;
                let mut col = farbe(t, *f);
                if *f == Farbe::Akzent && hover {
                    col = u.accent_hover;
                }
                let base = (r.y + (r.h + font.cap_height(px)) * 0.5).round();
                font.draw(c, text, px, r.x.round(), base, col);
                let line = s.round().max(1.0);
                if *durch {
                    c.fill_rect(
                        r.x,
                        (base - font.cap_height(px) * 0.45).round(),
                        r.w,
                        line,
                        col,
                    );
                }
                if hover && *f == Farbe::Akzent {
                    c.fill_rect(r.x, (base + 2.0 * s).round(), r.w, line, col);
                }
            }
            Art::Feld {
                feld,
                text,
                einheit,
            } => {
                let ed = v.edit.as_ref().filter(|e| e.feld == *feld);
                let (text, caret, select) = match ed {
                    Some(e) => (e.te.text.as_str(), Some(e.te.caret), Some(e.te.selection())),
                    None => (text.as_str(), None, None),
                };
                let st = FieldState {
                    text,
                    unit: einheit,
                    hover,
                    focus: ed.is_some(),
                    invalid: v.fehler_feld.as_ref() == Some(feld),
                    caret,
                    select,
                    disabled: false,
                };
                if *feld == Feld::Kurz {
                    widgets::text_field(c, fonts, *r, &st, s, t);
                } else {
                    widgets::field(c, fonts, *r, &st, s, t);
                }
            }
            Art::Lese(text) => widgets::field_readonly(c, fonts, *r, text, s, t),
            Art::Knopf(text) => {
                let st = ButtonState {
                    hover,
                    ..Default::default()
                };
                widgets::button(c, fonts, *r, text, st, s, t);
            }
            Art::Pille(text) => {
                let a = u.accent;
                let mut p = Path::new();
                p.rounded_rect(r.x, r.y, r.w, r.h, r.h * 0.5);
                c.fill(&p, Rgba(a.0, a.1, a.2, 48));
                if let Some(font) = fonts.bold.as_ref().or(fonts.regular.as_ref()) {
                    let px = 11.0 * s;
                    let x = r.x + (r.w - font.width(text, px)) * 0.5;
                    let base = (r.y + (r.h + font.cap_height(px)) * 0.5).round();
                    font.draw(c, text, px, x.round(), base, a);
                }
            }
            Art::Seg { text, an } => {
                let font = if *an {
                    fonts.bold.as_ref().or(fonts.regular.as_ref())
                } else {
                    fonts.regular.as_ref()
                };
                // Wie ein Feld: Rand, darin Feldgrund; gewählt in Akzent
                let b = s.max(1.0);
                let mut p = Path::new();
                p.rounded_rect(r.x, r.y, r.w, r.h, 5.0 * s);
                c.fill(&p, if *an { u.accent } else { u.field_border });
                let mut p = Path::new();
                p.rounded_rect(r.x + b, r.y + b, r.w - 2.0 * b, r.h - 2.0 * b, 5.0 * s - b);
                c.fill(&p, if *an { u.accent } else { u.field });
                if let Some(font) = font {
                    let px = KLEIN * s;
                    let x = r.x + (r.w - font.width(text, px)) * 0.5;
                    let base = (r.y + (r.h + font.cap_height(px)) * 0.5).round();
                    let col = if *an {
                        u.on_accent
                    } else if hover {
                        u.text
                    } else {
                        u.text_dim
                    };
                    font.draw(c, text, px, x.round(), base, col);
                }
            }
            Art::Flaeche(f) => {
                let mut p = Path::new();
                p.rounded_rect(r.x, r.y, r.w, r.h, (2.0 * s).min(r.h * 0.5));
                c.fill(&p, farbe(t, *f));
            }
            Art::Klapp(offen) => {
                let col = if hover { u.accent_hover } else { u.accent };
                let (ax, ay, d) = (r.x + 8.0 * s, r.y + r.h * 0.5, 3.5 * s);
                let mut p = Path::new();
                if *offen {
                    p.move_to(ax - d, ay - d * 0.6)
                        .line_to(ax + d, ay - d * 0.6)
                        .line_to(ax, ay + d * 0.6);
                } else {
                    p.move_to(ax - d * 0.6, ay - d)
                        .line_to(ax + d * 0.6, ay)
                        .line_to(ax - d * 0.6, ay + d);
                }
                p.close();
                c.fill(&p, col);
            }
            Art::Linie => c.fill_rect(r.x, r.y, r.w, s.round().max(1.0), u.border),
        }
    }
}

/// Ganze Zahl mit Tausenderpunkt: „1.234“.
fn tausend(n: i64) -> String {
    let t = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    if n < 0 {
        format!("−{out}")
    } else {
        out
    }
}

/// Zwei Nachkommastellen mit Komma: „150,08“.
fn komma_2(x: f64) -> String {
    format!("{x:.2}").replace('.', ",")
}

//! Freischaltung der Module (architektur.md §7b, ausbaustufen-freischaltung.md
//! §4): Skizzeo ist immer vollständig installiert, eine Ausbaustufe schaltet
//! Module frei. Die Freischaltung steuert nur, was die Oberfläche zeigt und
//! bedienen lässt, nie was gelesen, gespeichert oder gerechnet wird. Nur `app`
//! kennt sie.
//!
//! Jede Aufzählung von Eingängen (`menu::Command`, `ui::Id`, `cards::Blatt`,
//! `help::Topic`, `delete::Action`) ordnet jeden Eingang über ein vollständiges `match` ohne `_`
//! einem Modul oder keinem zu. Ein neuer Befehl, Knopf, ein neues Blatt oder
//! Hilfethema sagt so beim Anlegen, wozu er gehört; vergessen meldet der
//! Compiler.
//!
//! Gefiltert wird dort, wo Listen gezeichnet werden (Dateimenü, Paneele,
//! Kontextmenü, Kartenleiste, Liste der Hilfethemen), und noch einmal dort, wo ein Befehl
//! oder Knopf ausgeführt wird. So ist auch ein Tastenkürzel mitgesperrt.

use crate::cards::Blatt;
use crate::help::Topic;
use crate::menu::Command;
use crate::ui::{Id, Row};

/// Ein frei schaltbares Modul. Der Code fragt nie nach der Stufe, nur nach
/// dem Modul.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modul {
    Mengen,
    Kosten,
    Ava,
    Gelaende,
}

impl Modul {
    fn bit(self) -> u8 {
        1 << self as u8
    }

    /// Die einzige Stelle der Abhängigkeiten: Kosten braucht Mengen, AVA
    /// braucht Kosten. Gelände braucht nichts; seine Erdmengen erscheinen,
    /// sobald auch Mengen frei ist.
    fn braucht(self) -> Option<Modul> {
        match self {
            Modul::Kosten => Some(Modul::Mengen),
            Modul::Ava => Some(Modul::Kosten),
            Modul::Mengen | Modul::Gelaende => None,
        }
    }
}

/// Freie Module als Bitmenge. Heute steht sie fest auf [`Freischaltung::ALLES`];
/// woher sie später kommt (Lizenz), ist eine Zeile beim Start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Freischaltung(u8);

impl Default for Freischaltung {
    fn default() -> Freischaltung {
        Freischaltung::ALLES
    }
}

/// Ausbaustufen als benannte Bündel (Plan §4.2). Gelände kommt als Zusatz
/// zu jeder Stufe (`+gelaende`).
const STUFEN: [(&str, &[Modul]); 3] = [
    ("basis", &[]),
    ("pro", &[Modul::Mengen]),
    ("ultimate", &[Modul::Mengen, Modul::Kosten, Modul::Ava]),
];

impl Freischaltung {
    pub const ALLES: Freischaltung = Freischaltung(0b1111);

    pub fn mit(module: &[Modul]) -> Freischaltung {
        Freischaltung(module.iter().fold(0, |b, m| b | m.bit()))
    }

    /// Frei, wenn gesetzt und alle Voraussetzungen frei sind.
    pub fn frei(self, m: Modul) -> bool {
        self.0 & m.bit() != 0 && m.braucht().is_none_or(|v| self.frei(v))
    }

    /// Der Eingang gehört zu einem Modul, das nicht frei ist.
    pub fn gesperrt(self, e: impl Eingang) -> bool {
        e.modul().is_some_and(|m| !self.frei(m))
    }

    /// Stufe als Wort: `basis`, `pro` oder `ultimate`, je auch mit
    /// `+gelaende`.
    pub fn stufe(s: &str) -> Result<Freischaltung, String> {
        let s = s.trim().to_lowercase();
        let (name, gelaende) = match s.strip_suffix("+gelaende") {
            Some(n) => (n, true),
            None => (s.as_str(), false),
        };
        let (_, module) = STUFEN.iter().find(|(n, _)| *n == name).ok_or_else(|| {
            format!("--stufe: unbekannte Stufe „{s}“ (basis, pro, ultimate, je mit +gelaende)")
        })?;
        let f = Freischaltung::mit(module);
        Ok(if gelaende {
            Freischaltung(f.0 | Modul::Gelaende.bit())
        } else {
            f
        })
    }

    /// Stufe von der Befehlszeile (`--stufe pro+gelaende`) für Tests und
    /// Ist-Bilder; ohne Angabe ist alles frei. Ein unbekanntes Wort meldet
    /// sich auf der Konsole, dann bleibt alles frei.
    pub fn aus_args(args: impl Iterator<Item = String>) -> Freischaltung {
        let Some(w) = args.skip_while(|a| a != "--stufe").nth(1) else {
            return Freischaltung::ALLES;
        };
        Freischaltung::stufe(&w).unwrap_or_else(|e| {
            eprintln!("{e}");
            Freischaltung::ALLES
        })
    }

    /// Die Zeilen ohne gesperrte Knöpfe (Freischaltung); vom Knopfpaar bleibt
    /// der freie. Eine Trennlinie, die danach am Ende oder doppelt steht, fällt
    /// mit weg.
    pub(crate) fn zeilen(self, rows: Vec<Row>) -> Vec<Row> {
        let mut out: Vec<Row> = Vec::with_capacity(rows.len());
        let mut weg = false;
        for r in rows {
            let r = match r {
                Row::Button(id, _) | Row::More(id) | Row::TypeChip(id) | Row::Link(id, _)
                    if self.gesperrt(id) =>
                {
                    None
                }
                Row::Pair([a, b]) => match (self.gesperrt(a.0), self.gesperrt(b.0)) {
                    (false, false) => Some(Row::Pair([a, b])),
                    (true, false) => Some(Row::Button(b.0, b.1)),
                    (false, true) => Some(Row::Button(a.0, a.1)),
                    (true, true) => None,
                },
                Row::Segments(items) if items.iter().any(|i| self.gesperrt(i.0)) => None,
                r => Some(r),
            };
            match r {
                None => weg = true,
                Some(Row::Separator) if weg && matches!(out.last(), Some(Row::Separator)) => {}
                Some(r) => out.push(r),
            }
        }
        if weg && matches!(out.last(), Some(Row::Separator)) {
            out.pop();
        }
        out
    }

    /// Freie Blätter des Mengenfensters, von links gezählt: Weil Kosten
    /// Mengen und AVA Kosten braucht, sind es immer die ersten.
    pub fn blaetter(self) -> usize {
        Blatt::ALLE
            .iter()
            .take_while(|b| !self.gesperrt(**b))
            .count()
    }

    /// Name des Knopfs zum Mengenfenster und seines Tooltips: die freien
    /// Blätter.
    pub fn knopf(self) -> &'static str {
        match self.blaetter() {
            3 => crate::cards::KNOPF,
            2 => "Mengen · Kosten",
            _ => "Mengen",
        }
    }

    /// Beschriftung des Knopfs in zwei Zeilen (ab KA-4); mit nur einem Blatt
    /// bleibt die zweite leer.
    pub fn knopf_zeilen(self) -> (&'static str, &'static str) {
        match self.blaetter() {
            3 => crate::cards::KNOPF_ZEILEN,
            2 => ("Mengen", "Kosten"),
            _ => ("Mengen", ""),
        }
    }
}

/// Eine Aufzählung von Eingängen der Oberfläche: jeder Eingang gehört zu
/// einem Modul oder zu keinem.
pub trait Eingang: Copy {
    fn modul(self) -> Option<Modul>;
}

impl Eingang for Command {
    fn modul(self) -> Option<Modul> {
        match self {
            Command::Verwaltung => Some(Modul::Kosten),
            Command::New
            | Command::Open
            | Command::OpenRecent(_)
            | Command::Save
            | Command::SaveAs
            | Command::Close
            | Command::Quit
            | Command::Undo
            | Command::Redo
            | Command::OpenMenu
            | Command::ClearRecent
            | Command::Settings
            | Command::Catalog
            | Command::Materials
            | Command::Erweiterungen
            | Command::BauteilEinlesen
            | Command::Delete
            | Command::Backups
            | Command::OpenBackup(_)
            | Command::Help
            | Command::MeasureFrameTime
            | Command::ExportIfc
            | Command::IfcVorschau => None,
        }
    }
}

impl Eingang for Id {
    fn modul(self) -> Option<Modul> {
        match self {
            Id::Quantity => Some(Modul::Mengen),
            // OK Gelände im Geschossmanager gehört zur Geländeplatte, die es
            // ohne Modul gibt
            Id::Building
            | Id::Interior
            | Id::Ref(_)
            | Id::Sprung(_)
            | Id::View(_)
            | Id::Projektdaten
            | Id::Nord
            | Id::Field(_)
            | Id::Grip(_)
            | Id::Storey(_)
            | Id::FlatRoof
            | Id::LineVis(_)
            | Id::DialogClose
            | Id::DialogMinus
            | Id::DialogPlus
            | Id::DialogCancel
            | Id::DialogStart
            | Id::ToolType
            | Id::PropsType
            | Id::PropsLink
            | Id::PropsFlush
            | Id::PropsSplit
            | Id::PropsMerge
            | Id::PropsEcke
            | Id::PropsVerlaengern
            | Id::PropsDrains
            | Id::PropsDrainAdd
            | Id::PropsDrainDelete
            | Id::PropsAction(_)
            | Id::PropsMore
            | Id::PropsMaterial(_)
            | Id::Ext
            | Id::ExtTyp
            | Id::ExtWahl(..)
            | Id::ExtJa(_)
            | Id::ExtListe(_)
            | Id::LevelsOpen
            | Id::Opening(_) => None,
        }
    }
}

impl Eingang for Blatt {
    fn modul(self) -> Option<Modul> {
        Some(match self {
            Blatt::Mengen => Modul::Mengen,
            Blatt::Kosten => Modul::Kosten,
            Blatt::Ava => Modul::Ava,
        })
    }
}

impl Eingang for Topic {
    fn modul(self) -> Option<Modul> {
        match self {
            Topic::Quantities => Some(Modul::Mengen),
            Topic::Costs | Topic::Verwaltung => Some(Modul::Kosten),
            Topic::Ava => Some(Modul::Ava),
            Topic::Start
            | Topic::Navigation
            | Topic::Building
            | Topic::Draw
            | Topic::InnerWall
            | Topic::Ext
            | Topic::Openings
            | Topic::Rooms
            | Topic::Einlesen
            | Topic::Drag
            | Topic::Split
            | Topic::Upper
            | Topic::Flush
            | Topic::Wall
            | Topic::Foundation
            | Topic::Floor
            | Topic::Terrace
            | Topic::FlatRoof
            | Topic::Levels
            | Topic::Section
            | Topic::North
            | Topic::Tree
            | Topic::File
            | Topic::Settings
            | Topic::SettingsPens
            | Topic::SettingsLineTypes
            | Topic::SettingsFills
            | Topic::SettingsSurfaces
            | Topic::SettingsUi
            | Topic::SettingsVorgaben
            | Topic::Bemassung
            | Topic::Catalog
            | Topic::Materials
            | Topic::Patterns
            | Topic::Delete
            | Topic::Backups => None,
        }
    }
}

/// Zeilen der Hilfe, die von einem Modul sprechen (Review #120 H5): Sie
/// fehlen, solange es gesperrt ist. Thema und Beschriftung wie in
/// `hilfe.txt`; den Knopf zum Mengenfenster nennt die Hilfe mit den freien
/// Blättern ([`Freischaltung::knopf`]).
pub(crate) fn hilfe_zeile(t: Topic, beschriftung: &str) -> Option<Modul> {
    match (t, beschriftung) {
        (Topic::Start, "Mengen") => Some(Modul::Mengen),
        (Topic::Materials, "Preise") => Some(Modul::Kosten),
        (Topic::Materials, "Bestellung") => Some(Modul::Mengen),
        _ => None,
    }
}

impl Eingang for crate::delete::Action {
    fn modul(self) -> Option<Modul> {
        use crate::delete::Action;
        match self {
            Action::Quantity => Some(Modul::Mengen),
            Action::ShowInModel
            | Action::ChangeType
            | Action::Properties
            | Action::Delete
            | Action::DeleteBuilding
            | Action::AddDrain
            | Action::DeleteDrain => None,
        }
    }
}

impl crate::App {
    /// Freischaltung setzen: Paneele, Dateimenü, Kartenleiste und Hilfe
    /// zeigen danach nur noch die freien Eingänge.
    pub(crate) fn freischalten(&mut self, f: Freischaltung) {
        self.ui.frei = f;
        self.menu.frei = f;
        self.help.frei = f;
        self.quantity.set_frei(f);
        if let Some(m) = self.materials.as_mut() {
            m.set_frei(f);
        }
        self.redraw = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{FileMenu, Recent};
    use crate::ui::{Panel, Props, Ui};
    use sk_platform::{Event, Modifiers};
    use sk_ui::theme::Theme;

    fn stufe(s: &str) -> Freischaltung {
        Freischaltung::stufe(s).unwrap()
    }

    #[test]
    fn stufen_und_abhaengigkeiten() {
        let b = stufe("basis");
        let alle = [Modul::Mengen, Modul::Kosten, Modul::Ava, Modul::Gelaende];
        assert!(alle.iter().all(|m| !b.frei(*m)));
        let p = stufe("pro");
        assert!(p.frei(Modul::Mengen) && !p.frei(Modul::Kosten) && !p.frei(Modul::Ava));
        let u = stufe("Ultimate");
        assert!(u.frei(Modul::Ava) && !u.frei(Modul::Gelaende));
        assert!(stufe("basis+gelaende").frei(Modul::Gelaende));
        assert!(!stufe("basis+gelaende").frei(Modul::Mengen));
        assert_eq!(stufe("ultimate+gelaende"), Freischaltung::ALLES);
        assert!(Freischaltung::stufe("gold").is_err());
        // AVA ohne Kosten, Kosten ohne Mengen sind nicht frei
        assert!(!Freischaltung::mit(&[Modul::Ava, Modul::Mengen]).frei(Modul::Ava));
        assert!(!Freischaltung::mit(&[Modul::Kosten]).frei(Modul::Kosten));
        assert!(Freischaltung::mit(&[Modul::Gelaende]).frei(Modul::Gelaende));
        assert_eq!(Freischaltung::default(), Freischaltung::ALLES);
    }

    #[test]
    fn stufe_von_der_befehlszeile() {
        let a = |v: &[&str]| Freischaltung::aus_args(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&["skizzeo"]), Freischaltung::ALLES);
        assert_eq!(a(&["skizzeo", "--stufe", "pro"]), stufe("pro"));
        assert_eq!(
            a(&["skizzeo", "--stufe", "basis+gelaende", "x.szo"]),
            stufe("basis+gelaende")
        );
        assert_eq!(a(&["skizzeo", "--stufe", "nix"]), Freischaltung::ALLES);
        assert_eq!(a(&["skizzeo", "--stufe"]), Freischaltung::ALLES);
    }

    #[test]
    fn eingaenge_der_module() {
        let b = stufe("basis");
        let p = stufe("pro");
        assert!(b.gesperrt(Id::Quantity) && !p.gesperrt(Id::Quantity));
        assert!(!b.gesperrt(Id::Building) && !b.gesperrt(Id::Projektdaten));
        assert!(
            p.gesperrt(Command::Verwaltung) && !stufe("ultimate").gesperrt(Command::Verwaltung)
        );
        assert!(!b.gesperrt(Command::Save) && !b.gesperrt(Command::ExportIfc));
        assert!(p.gesperrt(Topic::Costs) && !p.gesperrt(Topic::Quantities));
        assert!(b.gesperrt(Topic::Quantities) && !b.gesperrt(Topic::Start));
        assert_eq!(
            [
                b,
                p,
                stufe("ultimate"),
                Freischaltung::mit(&[Modul::Mengen, Modul::Kosten])
            ]
            .map(Freischaltung::blaetter),
            [0, 1, 3, 2]
        );
        assert_eq!(p.knopf(), "Mengen");
        assert_eq!(p.knopf_zeilen(), ("Mengen", ""));
        assert_eq!(Freischaltung::ALLES.knopf(), crate::cards::KNOPF);
        // Jedes Hilfethema der freien Blätter ist frei, jedes gesperrte
        // Blatt nimmt sein Thema mit
        for f in [b, p, stufe("ultimate")] {
            for (blatt, thema) in [
                (Blatt::Mengen, Topic::Quantities),
                (Blatt::Kosten, Topic::Costs),
                (Blatt::Ava, Topic::Ava),
            ] {
                assert_eq!(f.gesperrt(blatt), f.gesperrt(thema), "{f:?} {blatt:?}");
            }
        }
    }

    /// Knöpfe eines Paneels, wie man mit der Maus darüber fährt.
    fn knoepfe(ui: &mut Ui, p: Panel) -> Vec<Id> {
        let (w, top) = (1440, 32);
        let r = ui.rect(p, w, top);
        let mut ids = Vec::new();
        let mods = Modifiers::default();
        for y in (r.y as i32..(r.y + r.h) as i32).step_by(2) {
            for x in (r.x as i32 + 2..(r.x + r.w) as i32).step_by(8) {
                let (x, y) = (x as f64, y as f64);
                ui.handle(&Event::MouseMove { x, y, mods }, w, top);
                if let Some(id) = ui.hover.filter(|id| !ids.contains(id)) {
                    ids.push(id);
                }
            }
        }
        ids
    }

    /// Abnahme je Stufe (Plan §4.7): sichtbar und bedienbar sind genau die
    /// Eingänge der Stufe, alle übrigen bleiben wie mit allem frei.
    #[test]
    fn abnahme_je_stufe() {
        let th = Theme::dark();
        let mut voll = Ui::new(1.0, &th);
        voll.fit(1.0, 1440, 900);
        let alle_ansichten = knoepfe(&mut voll, Panel::Views);
        let alle_werkzeuge = knoepfe(&mut voll, Panel::Tools);
        let alle_befehle: Vec<Command> = FileMenu::default()
            .items(true, &Recent::default())
            .iter()
            .filter_map(|i| i.command)
            .collect();
        let mut m = sk_model::Model::with_seed(12);
        let haus = m.add_building(1);
        let ecken = [(0.0, 0.0), (0.0, 8000.0), (10000.0, 8000.0), (10000.0, 0.0)];
        let ecken = ecken.map(|(x, y)| sk_math::vec3(x, y, 0.0));
        m.build_from_polygon(haus, &ecken).unwrap();
        let wand = m
            .elements()
            .iter()
            .find(|(_, e)| e.category == sk_model::Category::ExteriorWall)
            .map(|(id, _)| id)
            .unwrap();
        let alle_zeilen: Vec<String> = crate::delete::ContextMenu::new(
            &m,
            wand,
            &[wand],
            300.0,
            300.0,
            (1440, 900, 32),
            &th,
            1.0,
        )
        .actions()
        .into_iter()
        .map(|z| z.0)
        .collect();
        assert!(alle_zeilen.iter().any(|z| z == crate::cards::KNOPF));
        assert!(alle_ansichten.contains(&Id::Quantity));
        assert!(alle_befehle.contains(&Command::Verwaltung));
        for (w, mengen, kosten, karten) in [
            ("basis", false, false, 1),
            ("pro", true, false, 1),
            ("ultimate", true, true, 3),
            ("basis+gelaende", false, false, 1),
            ("ultimate+gelaende", true, true, 3),
        ] {
            let f = stufe(w);
            // Paneele: „Mengen“ nur mit Mengen, sonst dieselben Knöpfe
            let mut ui = Ui::new(1.0, &th);
            ui.fit(1.0, 1440, 900);
            ui.frei = f;
            let ansichten = knoepfe(&mut ui, Panel::Views);
            let erwartet: Vec<Id> = alle_ansichten
                .iter()
                .copied()
                .filter(|id| mengen || *id != Id::Quantity)
                .collect();
            assert_eq!(ansichten, erwartet, "{w}");
            assert_eq!(knoepfe(&mut ui, Panel::Tools), alle_werkzeuge, "{w}");
            // Menge je Schicht im Paneel „Eigenschaften“ nur mit Mengen
            let layer = (
                sk_paint::Rgba::from_rgb8([1, 2, 3]),
                "24 cm KS".into(),
                "3,796 m³".into(),
                None,
            );
            ui.set_props(Some(Props {
                layers: vec![layer],
                ..Props::default()
            }));
            assert_eq!(ui.props().unwrap().layers[0].2.is_empty(), !mengen, "{w}");
            // Raumbuchmengen ebenso (Review #120 B2): die Zeilen in `mengen`
            let werte = |p: Option<&Props>| -> Vec<&str> {
                p.unwrap().values.iter().map(|v| v.0).collect()
            };
            ui.set_props(Some(Props {
                values: vec![
                    ("Nutzung", "Wohnen".into()),
                    ("Bodenfläche", "20,00 m²".into()),
                    ("", "Estrich".into()),
                    ("Lichte Höhe", "2,50 m".into()),
                ],
                mengen: 1..3,
                ..Props::default()
            }));
            let soll = if mengen {
                vec!["Nutzung", "Bodenfläche", "", "Lichte Höhe"]
            } else {
                vec!["Nutzung", "Lichte Höhe"]
            };
            assert_eq!(werte(ui.props()), soll, "{w}");
            // Dateimenü: „Verwaltung …“ nur mit Kosten; Öffnen und Schließen
            // behalten die Freischaltung
            let mut menu = FileMenu::default();
            menu.frei = f;
            menu.open();
            menu.close();
            let befehle: Vec<Command> = menu
                .items(true, &Recent::default())
                .iter()
                .filter_map(|i| i.command)
                .collect();
            let erwartet: Vec<Command> = alle_befehle
                .iter()
                .copied()
                .filter(|c| kosten || *c != Command::Verwaltung)
                .collect();
            assert_eq!(befehle, erwartet, "{w}");
            // Kontextmenü am Bauteil: „Mengen …“ nur mit Mengen, mit dem
            // Namen der freien Blätter
            let zeilen: Vec<String> = crate::delete::ContextMenu::new(
                &m,
                wand,
                &[wand],
                300.0,
                300.0,
                (1440, 900, 32),
                &th,
                &ui,
            )
            .actions()
            .into_iter()
            .map(|z| z.0)
            .collect();
            let erwartet: Vec<String> = alle_zeilen
                .iter()
                .filter(|z| mengen || z.as_str() != crate::cards::KNOPF)
                .map(|z| z.replace(crate::cards::KNOPF, f.knopf()))
                .collect();
            assert_eq!(zeilen, erwartet, "{w}");
            // Kartenleiste: ein gemerktes gesperrtes Blatt fällt auf Mengen
            let mut q = crate::quantity::QuantityWindow::new();
            assert!(q.waehlen(Blatt::Ava, &th));
            q.set_frei(f);
            assert_eq!(q.blatt() == Blatt::Ava, kosten, "{w}");
            assert_eq!(q.waehlen(Blatt::Kosten, &th), kosten, "{w}");
            assert_eq!(f.blaetter().max(1), karten, "{w}");
        }
    }
}

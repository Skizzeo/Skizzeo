//! Ausgaben aus dem Dateimenü und den Blättern in fremde Formate: Tabelle
//! (.csv), PDF des LV und IFC (ifc/plan.md §5, I2).

use crate::{cards, document, meldung, schedule_view, umfang_view, App, NOTICE_TIME};
use sk_platform::Surface;

impl App {
    /// Dateiname des Hauses ohne Endung, ohne Datei „Unbenannt“.
    pub(crate) fn datei_stamm(&self) -> String {
        self.doc
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or("Unbenannt".to_string(), |s| {
                s.to_string_lossy().into_owned()
            })
    }

    /// „IFC exportieren …“: das Haus als IFC4-Datei (Reference View) für
    /// Statik, Energieberatung, Haustechnik und AVA; danach in der
    /// Statuszeile, was in der Datei steht.
    pub(crate) fn export_ifc(&mut self, surface: &Surface) {
        let name = format!("{}.ifc", self.datei_stamm());
        let filters = [
            ("IFC-Datei (*.ifc)", "*.ifc"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("IFC exportieren", &filters, "ifc", &name) else {
            return;
        };
        let zeit = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let opt = sk_ifc::Optionen {
            zeit,
            datei: path
                .file_name()
                .map_or(name, |n| n.to_string_lossy().into_owned()),
            version: env!("CARGO_PKG_VERSION").into(),
        };
        let (text, bericht) = sk_ifc::export(self.scene.model(), &opt);
        match document::tabelle_schreiben(&path, text.as_bytes()) {
            Ok(()) => self.status(
                meldung::Meldung::mit("{}", &[&ifc_meldung(&bericht)]),
                NOTICE_TIME,
            ),
            Err(e) => {
                let m =
                    meldung::Meldung::aus_io("IFC nicht gespeichert", "IFC speichern", &path, &e);
                surface.message(&m, true);
            }
        }
    }

    /// Tabelle des Mengenblatts im Umfang, in Gliederung und Sicht des
    /// Blatts, die Kopfzeile zuerst (KA-1).
    pub(crate) fn quantity_csv(&mut self) -> Vec<u8> {
        let u = self
            .quantity
            .list
            .as_ref()
            .map_or_else(sk_model::qto::Umfang::projekt, |l| l.leiste.umfang.clone());
        let sched = self.scene.schedule_in(&u);
        let kopf = umfang_view::umfang_text(self.scene.model(), &u, sk_platform::local_date_time());
        let sicht = (self.quantity.grouping, self.quantity.modus);
        schedule_view::csv_mit_kopf(self.scene.model(), &sched, sicht, &kopf)
    }

    /// „Als Tabelle speichern“: Windows-Dialog, .csv für Excel.
    pub(crate) fn save_csv(&mut self, surface: &Surface) {
        let stem = self.datei_stamm();
        let blatt = self.quantity.blatt();
        let kosten = blatt == cards::Blatt::Kosten;
        let ava = self
            .quantity
            .ava
            .as_ref()
            .filter(|_| blatt == cards::Blatt::Ava);
        let suggested = match ava.and_then(|a| a.los_name()) {
            // „haus LV Rohbau.csv“ (paket-ka4 §4)
            Some(los) => format!("{stem} LV {los}.csv"),
            None if kosten => format!("{stem} Kosten.csv"),
            // Bestellliste schon am Namen erkennbar (Bedienbarkeit 38.3)
            None if self.quantity.modus == sk_model::rechenweg::Modus::Bestellung => {
                format!("{stem} Mengenermittlung Bestellung.csv")
            }
            None => format!("{stem} Mengenermittlung.csv"),
        };
        let filters = [
            ("Tabelle für Excel (*.csv)", "*.csv"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("Als Tabelle speichern", &filters, "csv", &suggested)
        else {
            return;
        };
        let ava = self
            .quantity
            .ava
            .as_ref()
            .filter(|_| blatt == cards::Blatt::Ava);
        let bytes = match self.quantity.kosten.as_ref().filter(|_| kosten) {
            // Blatt AVA: das LV des gewählten Loses (ka-4-fach §3.6)
            _ if ava.is_some() => ava.map(|a| a.csv()).unwrap_or_default(),
            // Reiter Kosten: Kosten-CSV wie die Anzeige (ka-2-fach §2.5)
            Some(k) => {
                let (y, mo, d, h, mi) = sk_platform::local_date_time();
                k.csv(&stem, &format!("{d:02}.{mo:02}.{y}, {h:02}:{mi:02}"))
            }
            None => self.quantity_csv(),
        };
        if let Err(e) = document::tabelle_schreiben(&path, &bytes) {
            let m = meldung::Meldung::aus_io(
                "Tabelle nicht gespeichert",
                "Tabelle speichern",
                &path,
                &e,
            );
            surface.message(&m, true);
        }
    }

    /// „LV Rohbau als PDF speichern“ aus der AVA-Druckvorschau: genau die
    /// gezeigten Seiten (kosten/lv-blatt-a4.md §9).
    pub(crate) fn save_pdf(&mut self, surface: &Surface) {
        let Some((name, bytes)) = self
            .quantity
            .ava
            .as_ref()
            .and_then(|a| a.pdf(&self.ui.fonts))
        else {
            return;
        };
        let filters = [
            ("PDF-Dokument (*.pdf)", "*.pdf"),
            ("Alle Dateien (*.*)", "*.*"),
        ];
        let Some(path) = surface.save_dialog("Als PDF speichern", &filters, "pdf", &name) else {
            return;
        };
        if let Err(e) = document::tabelle_schreiben(&path, &bytes) {
            let m = meldung::Meldung::aus_io("PDF nicht gespeichert", "PDF speichern", &path, &e);
            surface.message(&m, true);
        }
    }
}

fn zahl(n: usize, eins: &str, mehr: &str) -> String {
    if n == 1 {
        format!("1 {eins}")
    } else {
        format!("{n} {mehr}")
    }
}

/// Satz der Statuszeile nach dem Export: „IFC gespeichert: 2 Geschosse,
/// 34 Bauteile“, bei mehreren Gebäuden mit deren Zahl vorn, mit den
/// Öffnungen und Türen, wenn es welche gibt, und was nicht in die Datei
/// kam.
pub(crate) fn ifc_meldung(b: &sk_ifc::Bericht) -> String {
    let mut teile = Vec::new();
    if b.gebaeude > 1 {
        teile.push(zahl(b.gebaeude, "Gebäude", "Gebäude"));
    }
    teile.push(zahl(b.geschosse, "Geschoss", "Geschosse"));
    teile.push(zahl(b.bauteile, "Bauteil", "Bauteile"));
    if b.oeffnungen > 0 {
        teile.push(zahl(b.oeffnungen, "Öffnung", "Öffnungen"));
    }
    if b.tueren > 0 {
        teile.push(zahl(b.tueren, "Tür", "Türen"));
    }
    let mut s = format!("IFC gespeichert: {}", teile.join(", "));
    if !b.ausgelassen.is_empty() {
        let nr: Vec<&str> = b.ausgelassen.iter().map(|(n, _)| n.as_str()).collect();
        s += &format!(" · nicht in der Datei: {}", nr.join(", "));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meldung_nach_dem_export() {
        let mut b = sk_ifc::Bericht {
            gebaeude: 1,
            geschosse: 2,
            bauteile: 34,
            baugruppen: 0,
            oeffnungen: 0,
            tueren: 0,
            ausgelassen: vec![],
        };
        assert_eq!(ifc_meldung(&b), "IFC gespeichert: 2 Geschosse, 34 Bauteile");
        (b.oeffnungen, b.tueren) = (3, 1);
        assert_eq!(
            ifc_meldung(&b),
            "IFC gespeichert: 2 Geschosse, 34 Bauteile, 3 Öffnungen, 1 Tür"
        );
        (b.oeffnungen, b.tueren) = (0, 0);
        b.gebaeude = 2;
        b.geschosse = 1;
        b.bauteile = 1;
        assert_eq!(
            ifc_meldung(&b),
            "IFC gespeichert: 2 Gebäude, 1 Geschoss, 1 Bauteil"
        );
        b.ausgelassen = vec![("AW-009".into(), sk_ifc::Grund::OhneForm)];
        assert!(ifc_meldung(&b).ends_with(" · nicht in der Datei: AW-009"));
    }
}

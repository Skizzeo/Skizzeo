//! Sätze für Menschen in der App (Bausteingrenze §5): Was in der
//! Statuszeile oder an einem Feld steht, ist eine [`Meldung`]. Sie entsteht
//! nur aus einem festen Satz, einem festen Satz mit eingesetzten Werten,
//! einem Befund oder einem Dateifehler; aus einem beliebigen `String` gibt
//! es keinen Weg. So erreicht kein `format!("… {e}")` mit Pfad oder
//! Systemtext den Bildschirm. Was eine Fehlersuche braucht (Pfad,
//! Systemtext, Regel und Ort eines Befunds), steht im Fehlerprotokoll
//! `%APPDATA%\Skizzeo\fehlerprotokoll.txt`.

use std::io;
use std::path::Path;

/// Ein Satz für die Statuszeile oder ein Feld.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Meldung(String);

impl Meldung {
    /// Fester Satz.
    pub fn satz(s: &'static str) -> Self {
        Meldung(s.to_string())
    }

    /// Fester Satz mit Platzhaltern `{}`, der Reihe nach durch `werte`
    /// ersetzt (Namen, Zahlen, Uhrzeiten).
    pub fn mit(vorlage: &'static str, werte: &[&str]) -> Self {
        let mut out = String::with_capacity(vorlage.len() + 16);
        let mut rest = vorlage;
        let mut w = werte.iter();
        while let Some(i) = rest.find("{}") {
            out.push_str(&rest[..i]);
            out.push_str(w.next().copied().unwrap_or(""));
            rest = &rest[i + 2..];
        }
        out.push_str(rest);
        debug_assert!(w.next().is_none(), "mehr Werte als Platzhalter: {vorlage}");
        Meldung(out)
    }

    /// Satz eines Befunds; Regel und Ort gehen ins Fehlerprotokoll.
    pub fn aus_befund(b: &sk_cost::Befund) -> Self {
        protokoll(&b.protokoll());
        Meldung(b.satz.clone())
    }

    /// Satz des ersten Befunds, sonst `sonst`, ohne Fehlerprotokoll: Beim
    /// Tippen ist ein abgelehnter Wert kein Fehler, und das Protokoll
    /// bekäme sonst bei jeder Taste eine Zeile (und einen Dateizugriff).
    pub fn vorschau(b: &[sk_cost::Befund], sonst: &'static str) -> Self {
        b.first()
            .map_or_else(|| Meldung::satz(sonst), |x| Meldung(x.satz.clone()))
    }

    /// Der erste Befund, sonst `sonst`; alle gehen ins Fehlerprotokoll.
    pub fn aus_befunden(b: &[sk_cost::Befund], sonst: &'static str) -> Self {
        for x in b.iter().skip(1) {
            protokoll(&x.protokoll());
        }
        b.first()
            .map_or_else(|| Meldung::satz(sonst), Meldung::aus_befund)
    }

    /// Dateifehler als ganzer Satz (Bausteingrenze §5, Bedienbarkeit
    /// Durchgang 8, Nachtrag): „{ergebnis}: {Grund}“ nach der Art des
    /// Fehlers, sonst nur „{was} hat nicht geklappt.“. `datei` erscheint nur
    /// mit Namen, Pfad und Systemtext gehen ins Fehlerprotokoll.
    pub fn aus_io(ergebnis: &'static str, was: &'static str, datei: &Path, e: &io::Error) -> Self {
        protokoll(&format!(
            "{was}: {} – {e} ({:?})",
            datei.display(),
            e.kind()
        ));
        let name = datei
            .file_name()
            .map_or_else(|| "Die Datei".into(), |n| n.to_string_lossy().into_owned());
        let grund = match io_art(e) {
            IoArt::Belegt => Meldung::mit(
                "{} ist gerade in einem anderen Programm geöffnet. Dort schließen, dann nochmal versuchen.",
                &[&name],
            ),
            IoArt::Rechte => Meldung::mit("Keine Schreibrechte für {}.", &[&name]),
            IoArt::Fehlt => Meldung::mit("{} wurde nicht gefunden.", &[&name]),
            IoArt::Voll => Meldung::satz("Auf dem Laufwerk ist kein Platz mehr."),
            IoArt::Netz => {
                Meldung::satz("Das Netzlaufwerk ist gerade nicht erreichbar. Nochmal versuchen.")
            }
            IoArt::Sonst => return Meldung::mit("{} hat nicht geklappt.", &[was]),
        };
        Meldung(format!("{ergebnis}: {}", grund.0))
    }

    /// Ein fester Satz dahinter.
    pub fn dazu(self, satz: &'static str) -> Self {
        Meldung(format!("{} {satz}", self.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for Meldung {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Meldung {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl PartialEq<str> for Meldung {
    fn eq(&self, o: &str) -> bool {
        self.0 == o
    }
}

impl PartialEq<&str> for Meldung {
    fn eq(&self, o: &&str) -> bool {
        self.0 == *o
    }
}

/// Art eines Dateifehlers für den Satz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IoArt {
    Belegt,
    Rechte,
    Fehlt,
    Voll,
    Netz,
    Sonst,
}

fn io_art(e: &io::Error) -> IoArt {
    use io::ErrorKind as K;
    // Windows-Codes: 32/33 Freigabe- bzw. Sperrverletzung, 53/64/67
    // Netzpfad weg. Im Test gelten die Codes auch unter Linux, damit sich
    // die Fälle nachstellen lassen.
    if cfg!(any(windows, test)) {
        match e.raw_os_error() {
            Some(32 | 33) => return IoArt::Belegt,
            Some(53 | 64 | 67) => return IoArt::Netz,
            _ => {}
        }
    }
    match e.kind() {
        K::PermissionDenied | K::ReadOnlyFilesystem => IoArt::Rechte,
        K::NotFound => IoArt::Fehlt,
        K::StorageFull => IoArt::Voll,
        K::TimedOut
        | K::NotConnected
        | K::NetworkUnreachable
        | K::HostUnreachable
        | K::NetworkDown
        | K::ConnectionReset
        | K::ConnectionAborted => IoArt::Netz,
        _ => IoArt::Sonst,
    }
}

/// Alle Arten von Dateifehlern, je ein Beispiel (für `nutzersaetze_sauber`).
#[cfg(test)]
pub fn io_beispiele() -> Vec<io::Error> {
    use io::ErrorKind as K;
    let mut v: Vec<io::Error> = [32, 33, 53, 64, 67, 5, 2]
        .into_iter()
        .map(io::Error::from_raw_os_error)
        .collect();
    v.extend(
        [
            K::NotFound,
            K::PermissionDenied,
            K::ConnectionRefused,
            K::ConnectionReset,
            K::HostUnreachable,
            K::NetworkUnreachable,
            K::ConnectionAborted,
            K::NotConnected,
            K::AddrInUse,
            K::AddrNotAvailable,
            K::NetworkDown,
            K::BrokenPipe,
            K::AlreadyExists,
            K::WouldBlock,
            K::NotADirectory,
            K::IsADirectory,
            K::DirectoryNotEmpty,
            K::ReadOnlyFilesystem,
            K::StaleNetworkFileHandle,
            K::InvalidInput,
            K::InvalidData,
            K::TimedOut,
            K::WriteZero,
            K::StorageFull,
            K::NotSeekable,
            K::QuotaExceeded,
            K::FileTooLarge,
            K::ResourceBusy,
            K::ExecutableFileBusy,
            K::Deadlock,
            K::CrossesDevices,
            K::TooManyLinks,
            K::InvalidFilename,
            K::ArgumentListTooLong,
            K::Interrupted,
            K::Unsupported,
            K::UnexpectedEof,
            K::OutOfMemory,
            K::Other,
        ]
        .into_iter()
        .map(|k| io::Error::new(k, "Systemtext (os error 5)")),
    );
    v
}

/// Eine Zeile ins Fehlerprotokoll: Uhrzeit und Text. Im Test sammeln sich
/// die Zeilen im Speicher ([`protokoll_im_test`]).
pub fn protokoll(zeile: &str) {
    #[cfg(test)]
    PROTOKOLL.with(|p| p.borrow_mut().push(zeile.to_string()));
    #[cfg(not(test))]
    if let Some(a) = std::env::var_os("APPDATA") {
        use std::io::Write;
        let dir = std::path::PathBuf::from(a).join("Skizzeo");
        let _ = std::fs::create_dir_all(&dir);
        let (j, mo, t, h, mi) = sk_platform::local_date_time();
        let datei = dir.join("fehlerprotokoll.txt");
        // Begrenzt: Ab 1 MB wird die Datei zu „fehlerprotokoll-alt.txt“
        // (eine ältere fällt weg), zusammen also höchstens etwa 2 MB
        if std::fs::metadata(&datei).is_ok_and(|m| m.len() > PROTOKOLL_GRENZE) {
            let _ = std::fs::rename(&datei, dir.join("fehlerprotokoll-alt.txt"));
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&datei)
        {
            let _ = writeln!(f, "{t:02}.{mo:02}.{j} {h:02}:{mi:02} {zeile}");
        }
    }
}

/// Größe, ab der das Fehlerprotokoll neu beginnt.
#[cfg(not(test))]
const PROTOKOLL_GRENZE: u64 = 1 << 20;

#[cfg(test)]
thread_local! {
    static PROTOKOLL: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Im Test: die Zeilen des Fehlerprotokolls seit dem letzten Aufruf.
#[cfg(test)]
pub fn protokoll_im_test() -> Vec<String> {
    PROTOKOLL.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

#[cfg(test)]
mod pruefung;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_saetze() {
        let p = Path::new("/irgendwo/tief/firmenkatalog.szk");
        let m = |e: io::Error| {
            Meldung::aus_io(
                "Firmenkatalog nicht gespeichert",
                "Firmenkatalog speichern",
                p,
                &e,
            )
        };
        assert_eq!(
            m(io::Error::other("kaputt")),
            "Firmenkatalog speichern hat nicht geklappt."
        );
        assert_eq!(
            m(io::Error::from(io::ErrorKind::PermissionDenied)),
            "Firmenkatalog nicht gespeichert: Keine Schreibrechte für firmenkatalog.szk."
        );
        assert_eq!(
            m(io::Error::from_raw_os_error(32)),
            "Firmenkatalog nicht gespeichert: firmenkatalog.szk ist gerade in einem anderen Programm geöffnet. Dort schließen, dann nochmal versuchen."
        );
        assert_eq!(
            m(io::Error::from_raw_os_error(53)),
            "Firmenkatalog nicht gespeichert: Das Netzlaufwerk ist gerade nicht erreichbar. Nochmal versuchen."
        );
        let log = protokoll_im_test();
        assert_eq!(log.len(), 4);
        assert!(
            log[0].contains("/irgendwo/tief") && log[0].contains("kaputt"),
            "{log:?}"
        );
    }

    /// Beim Tippen schreibt ein abgelehnter Wert nichts ins Protokoll;
    /// Projekt speichern nennt die Datei nur mit Namen (Review 3al).
    #[test]
    fn vorschau_still_und_projekt_ohne_pfad() {
        protokoll_im_test();
        let b = [sk_cost::Befund::fehler(
            76,
            "Der Preis ist zu hoch.",
            sk_cost::befund::Ort::Datei,
        )];
        assert_eq!(Meldung::vorschau(&b, "Nichts."), "Der Preis ist zu hoch.");
        assert_eq!(Meldung::vorschau(&[], "Nichts."), "Nichts.");
        assert!(protokoll_im_test().is_empty());

        let d = std::env::temp_dir().join("skizzeo-meldung-ohne-pfad");
        let m = sk_model::Model::new();
        let e = crate::document::save(&m, &d.join("fehlt").join("Haus.szo")).unwrap_err();
        assert!(!e.contains("skizzeo-meldung-ohne-pfad"), "{e}");
        assert!(!e.contains("os error"), "{e}");
        assert!(e.starts_with("Projekt nicht gespeichert"), "{e}");
        let e = crate::document::load(&d.join("fehlt.szo")).err().unwrap();
        assert!(!e.contains("skizzeo-meldung-ohne-pfad"), "{e}");
        assert_eq!(protokoll_im_test().len(), 2);
    }

    #[test]
    fn mit_setzt_der_reihe_nach_ein() {
        assert_eq!(
            Meldung::mit("Sicherung von {} wiederhergestellt.", &["09:05"]),
            "Sicherung von 09:05 wiederhergestellt."
        );
        assert_eq!(Meldung::satz("Gut.").dazu("Weiter."), "Gut. Weiter.");
    }
}

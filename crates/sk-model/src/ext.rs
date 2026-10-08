//! Erweiterungsspeicher (KA-0b, architektur/bausteingrenze-sk-cost.md §4.1).
//!
//! Hält die Dateizeilen von Erweiterungsabschnitten, die ein anderer
//! Baustein deutet (Kosten, AVA und Stammdaten in `sk-cost`), roh und mit
//! ihrer Kennung. `sk-model` kennt ihre Bedeutung nicht: Es liest, schreibt
//! und nimmt zurück, Zeile für Zeile bytegleich. Welche Abschnitte dazu
//! gehören und in welcher Reihenfolge sie stehen, sagt der Aufrufer.

/// Eine Zeile eines Erweiterungsabschnitts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtRec {
    pub section: String,
    /// Wert von `guid=` oder `key=`, je nachdem, was zuerst in der Zeile
    /// steht; `None`: Zeile ohne Kennung, bleibt roh (Regel 72).
    pub id: Option<String>,
    /// Die Zeile, wie sie in der Datei steht bzw. geschrieben wird.
    pub line: String,
}

/// Zeilen aller Erweiterungsabschnitte in Dateireihenfolge. Ein eigener
/// Speicher ist frei änderbar (Arbeitskopie in `sk-cost`); der des Modells
/// ändert sich nur über [`crate::Model::ext_put`] im Schritt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtStore {
    recs: Vec<ExtRec>,
    /// Reihenfolge der Abschnitte beim Schreiben.
    sections: Vec<String>,
}

/// Kennung einer Zeile: der erste der Schlüssel `guid=` und `key=`.
pub fn rec_id(line: &str) -> Option<String> {
    let r = crate::szo::Record::parse(0, line).ok().flatten()?;
    let at = |k: &str| {
        r.opt(k)?;
        line.find(&format!(" {k}="))
    };
    let k = match (at("guid"), at("key")) {
        (Some(g), Some(k)) if k < g => "key",
        (Some(_), _) => "guid",
        (None, Some(_)) => "key",
        (None, None) => return None,
    };
    r.opt(k).map(str::to_string)
}

impl ExtStore {
    /// Nimmt die Abschnitte `sections` in dieser Reihenfolge auf, soweit sie
    /// noch fehlen; vorhandene behalten ihren Platz. Ein fehlender kommt
    /// hinter seinen Vorgänger in der Liste, sonst vor seinen ersten
    /// vorhandenen Nachfolger, sonst ans Ende.
    pub fn declare(&mut self, sections: &[&str]) {
        let mut at: Option<usize> = None;
        for (k, s) in sections.iter().enumerate() {
            match self.sections.iter().position(|x| x == s) {
                Some(i) => at = Some(i + 1),
                None => {
                    let i = at.unwrap_or_else(|| {
                        sections[k + 1..]
                            .iter()
                            .find_map(|n| self.sections.iter().position(|x| x == n))
                            .unwrap_or(self.sections.len())
                    });
                    self.sections.insert(i, s.to_string());
                    at = Some(i + 1);
                }
            }
        }
    }

    /// Eine gelesene Zeile anhängen.
    pub fn push_read(&mut self, section: &str, line: &str) {
        self.declare(&[section]);
        self.recs.push(ExtRec {
            section: section.to_string(),
            id: rec_id(line),
            line: line.to_string(),
        });
    }

    pub fn is_empty(&self) -> bool {
        self.recs.is_empty()
    }

    /// Alle Zeilen in Dateireihenfolge.
    pub fn recs(&self) -> &[ExtRec] {
        &self.recs
    }

    /// Zeilen eines Abschnitts in Dateireihenfolge.
    pub fn section<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a ExtRec> + 'a {
        self.recs.iter().filter(move |r| r.section == section)
    }

    /// Platz der `at`-ten Zeile des Abschnitts in `recs`; `at` gleich der
    /// Zahl der Zeilen: hinter die letzte (ohne Zeilen: ans Ende).
    fn slot(&self, section: &str, at: usize) -> usize {
        let mut last = None;
        let mut n = 0;
        for (i, r) in self.recs.iter().enumerate() {
            if r.section == section {
                if n == at {
                    return i;
                }
                n += 1;
                last = Some(i);
            }
        }
        last.map_or(self.recs.len(), |i| i + 1)
    }

    /// Stelle der ersten Zeile mit Kennung `id` im Abschnitt.
    fn find(&self, section: &str, id: &str) -> Option<usize> {
        self.section(section)
            .position(|r| r.id.as_deref() == Some(id))
    }

    /// Ersetzt die erste Zeile mit Kennung `id` oder fügt sie ein: vor die
    /// erste Zeile mit Kennung `before`, sonst ans Ende des Abschnitts.
    /// Gibt die Stelle im Abschnitt und die alte Zeile zurück.
    pub fn put(
        &mut self,
        section: &str,
        id: &str,
        line: String,
        before: Option<&str>,
    ) -> (usize, Option<String>) {
        self.declare(&[section]);
        if let Some(at) = self.find(section, id) {
            let i = self.slot(section, at);
            let old = std::mem::replace(&mut self.recs[i].line, line);
            self.recs[i].id = Some(id.to_string());
            return (at, Some(old));
        }
        let n = self.section(section).count();
        let at = before.and_then(|b| self.find(section, b)).unwrap_or(n);
        self.set(section, at, Some(line), None);
        (at, None)
    }

    /// Entfernt die erste Zeile mit Kennung `id`; Stelle und alte Zeile.
    pub fn remove(&mut self, section: &str, id: &str) -> Option<(usize, String)> {
        let at = self.find(section, id)?;
        let i = self.slot(section, at);
        Some((at, self.recs.remove(i).line))
    }

    /// Stellt die Zeile an Stelle `at` des Abschnitts auf `to`; `from` ist
    /// der Stand davor (Rückgängig und Wiederholen von `Change::Ext`).
    pub(crate) fn set(
        &mut self,
        section: &str,
        at: usize,
        to: Option<String>,
        from: Option<String>,
    ) {
        let i = self.slot(section, at);
        match (to, from) {
            (Some(line), None) => {
                self.declare(&[section]);
                self.recs.insert(
                    i,
                    ExtRec {
                        section: section.to_string(),
                        id: rec_id(&line),
                        line,
                    },
                );
            }
            (None, Some(_)) => {
                if i < self.recs.len() && self.recs[i].section == section {
                    self.recs.remove(i);
                }
            }
            (Some(line), Some(_)) => {
                if let Some(r) = self.recs.get_mut(i).filter(|r| r.section == section) {
                    r.id = rec_id(&line);
                    r.line = line;
                }
            }
            (None, None) => {}
        }
    }

    /// Schreibt alle Zeilen, Abschnitt für Abschnitt in der vereinbarten
    /// Reihenfolge, innerhalb eines Abschnitts in Dateireihenfolge.
    pub fn write(&self, out: &mut String) {
        for s in &self.sections {
            for r in self.section(s) {
                out.push_str(&r.line);
                out.push('\n');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kennung_guid_oder_key() {
        assert_eq!(
            rec_id("[article] guid=0abc name=\"x\" key=k").as_deref(),
            Some("0abc")
        );
        assert_eq!(
            rec_id("[origin] key=0abc rec=article guid=zz").as_deref(),
            Some("0abc")
        );
        assert_eq!(rec_id("[rate] key=wage num=60").as_deref(), Some("wage"));
        assert_eq!(rec_id("[rate] num=60"), None);
        assert_eq!(rec_id("[rate] name=\"guid=1\" num=60"), None);
    }

    #[test]
    fn einfuegen_ersetzen_entfernen_und_zurueck() {
        let mut s = ExtStore::default();
        s.declare(&["a", "b"]);
        s.push_read("b", "[b] key=1");
        s.push_read("a", "[a] guid=x n=1");
        s.push_read("a", "[a] n=0");
        s.push_read("a", "[a] guid=z n=1");
        let mut out = String::new();
        s.write(&mut out);
        assert_eq!(out, "[a] guid=x n=1\n[a] n=0\n[a] guid=z n=1\n[b] key=1\n");
        let before = s.clone();
        // neu vor z
        let (at, old) = s.put("a", "y", "[a] guid=y".into(), Some("z"));
        assert_eq!((at, old), (2, None));
        // ersetzen
        let (at2, old2) = s.put("a", "x", "[a] guid=x n=2".into(), None);
        assert_eq!((at2, old2.as_deref()), (0, Some("[a] guid=x n=1")));
        // entfernen
        let (at3, old3) = s.remove("a", "z").unwrap();
        assert_eq!(at3, 3);
        let mut out = String::new();
        s.write(&mut out);
        assert_eq!(out, "[a] guid=x n=2\n[a] n=0\n[a] guid=y\n[b] key=1\n");
        // rückwärts zurück
        s.set("a", at3, Some(old3), None);
        s.set("a", at2, old2, Some("[a] guid=x n=2".into()));
        s.set("a", at, None, Some("[a] guid=y".into()));
        assert_eq!(s, before);
        // neuer Abschnitt ans Ende der Reihenfolge
        s.put("c", "q", "[c] key=q".into(), None);
        let mut out = String::new();
        s.write(&mut out);
        assert!(out.ends_with("[b] key=1\n[c] key=q\n"), "{out}");
        // Lücke in der Liste: vor den Nachfolger
        s.declare(&["z", "b"]);
        s.declare(&["a", "y"]);
        assert_eq!(s.sections, ["a", "y", "z", "b", "c"]);
    }
}

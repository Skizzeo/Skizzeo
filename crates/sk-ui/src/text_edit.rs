//! Bearbeiten einer Textzeile: Schreibmarke, Markierung, Ausschneiden,
//! Einfügen und Rückgängig im Feld. Ohne Zeichnen und ohne Tasten; die App
//! ordnet die Tasten zu.

/// Text mit Schreibmarke und Markierung (Byte-Stellen an Zeichengrenzen).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    pub text: String,
    /// Schreibmarke.
    pub caret: usize,
    /// Anderes Ende der Markierung (gleich `caret`: nichts markiert).
    pub anchor: usize,
    /// Frühere Stände für Strg+Z im Feld.
    undo: Vec<(String, usize)>,
}

impl TextEdit {
    /// Neuer Text, ganz markiert (wie beim Hineinklicken mit der Tastatur).
    pub fn new(text: &str) -> TextEdit {
        TextEdit {
            text: text.into(),
            caret: text.len(),
            anchor: 0,
            undo: Vec::new(),
        }
    }

    /// Markierter Bereich (von < bis).
    pub fn selection(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn selected(&self) -> &str {
        let (a, z) = self.selection();
        &self.text[a..z]
    }

    fn remember(&mut self) {
        if self.undo.last().is_none_or(|u| u.0 != self.text) {
            self.undo.push((self.text.clone(), self.caret));
            if self.undo.len() > 50 {
                self.undo.remove(0);
            }
        }
    }

    /// Ersetzt die Markierung durch `s` (Tippen und Einfügen).
    pub fn insert(&mut self, s: &str) {
        self.remember();
        let (a, z) = self.selection();
        self.text.replace_range(a..z, s);
        self.caret = a + s.len();
        self.anchor = self.caret;
    }

    /// Rücktaste: Markierung oder das Zeichen vor der Marke löschen.
    pub fn backspace(&mut self) {
        if self.caret == self.anchor {
            self.anchor = self.prev(self.caret);
        }
        self.insert("");
    }

    /// Entf: Markierung oder das Zeichen nach der Marke löschen.
    pub fn delete(&mut self) {
        if self.caret == self.anchor {
            self.anchor = self.next(self.caret);
        }
        self.insert("");
    }

    /// Ausschneiden: liefert den markierten Text und entfernt ihn.
    pub fn cut(&mut self) -> String {
        let s = self.selected().to_string();
        if !s.is_empty() {
            self.insert("");
        }
        s
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    /// Marke nach links bzw. rechts; mit `select` wächst die Markierung.
    pub fn left(&mut self, select: bool) {
        let (a, _) = self.selection();
        self.caret = if !select && self.caret != self.anchor {
            a
        } else {
            self.prev(self.caret)
        };
        if !select {
            self.anchor = self.caret;
        }
    }

    pub fn right(&mut self, select: bool) {
        let (_, z) = self.selection();
        self.caret = if !select && self.caret != self.anchor {
            z
        } else {
            self.next(self.caret)
        };
        if !select {
            self.anchor = self.caret;
        }
    }

    pub fn home(&mut self, select: bool) {
        self.caret = 0;
        if !select {
            self.anchor = 0;
        }
    }

    pub fn end(&mut self, select: bool) {
        self.caret = self.text.len();
        if !select {
            self.anchor = self.caret;
        }
    }

    /// Marke an die Stelle `i` (Byte, auf die nächste Zeichengrenze gerundet).
    pub fn place(&mut self, i: usize, select: bool) {
        let mut i = i.min(self.text.len());
        while !self.text.is_char_boundary(i) {
            i -= 1;
        }
        self.caret = i;
        if !select {
            self.anchor = i;
        }
    }

    /// Strg+Z im Feld. `false`, wenn es nichts zurückzunehmen gibt.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some((t, c)) => {
                self.text = t;
                self.caret = c.min(self.text.len());
                self.anchor = self.caret;
                true
            }
            None => false,
        }
    }

    fn prev(&self, i: usize) -> usize {
        self.text[..i].char_indices().last().map_or(0, |(j, _)| j)
    }

    fn next(&self, i: usize) -> usize {
        self.text[i..]
            .chars()
            .next()
            .map_or(i, |c| i + c.len_utf8())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tippen_loeschen_markieren() {
        let mut e = TextEdit::new("Kräftig");
        e.insert("Fein");
        assert_eq!(e.text, "Fein");
        e.left(false);
        e.left(true);
        assert_eq!(e.selected(), "i");
        e.backspace();
        assert_eq!(e.text, "Fen");
        e.end(false);
        e.insert("ä");
        e.left(false);
        e.delete();
        assert_eq!(e.text, "Fen");
        e.select_all();
        assert_eq!(e.cut(), "Fen");
        assert_eq!(e.text, "");
        assert!(e.undo());
        assert_eq!(e.text, "Fen");
        let mut e = TextEdit::new("Größe");
        e.place(3, false);
        assert_eq!(e.caret, 2, "auf Zeichengrenze");
        e.right(true);
        assert_eq!(e.selected(), "ö");
    }
}

//! Paneele, Teil Dialog „Gebäude erstellen“ (aus ui.rs, Paket T3).

use super::*;

/// Dialog „Gebäude erstellen“ (E16): „Geschosse: 2 (EG + OG)“, derzeit
/// fest, darunter die Vorgaben von oben nach unten (Jörn 10:13); jede
/// gültige Eingabe zeigt das Paneel „Geschosse“ sofort.
impl Ui {
    /// Obere Kante der Zeile `i` (0 = Geschosse, dann die Felder), dip.
    fn dialog_row_y(&self, i: usize) -> f32 {
        DIALOG_ROW_Y + i as f32 * self.size.dialog_row
    }

    /// Höhe des Dialogs (dip): mindestens die eingestellte, sonst so hoch,
    /// dass unter den Feldern der Hinweis und die Knöpfe Platz haben.
    pub(super) fn dialog_h(&self) -> f32 {
        let n = self.dialog_fields.len();
        let need = self.dialog_row_y(n + 2) + 12.0 + 30.0 + 12.0;
        self.size.dialog_h.max(need)
    }

    /// Knöpfe des Dialogs in Paneelkoordinaten.
    pub(super) fn dialog_buttons(&self) -> Vec<(Id, Rect, &'static str)> {
        let (s, z) = (self.scale, &self.size);
        let (w, h, pad) = (z.dialog_w, self.dialog_h(), z.panel_pad);
        let r = |x: f32, y: f32, bw: f32, bh: f32| Rect::new(x * s, y * s, bw * s, bh * s);
        let (start_w, cancel_w, bh) = (150.0, 96.0, 30.0);
        let by = h - 12.0 - bh;
        let fields = self.dialog_fields.iter().enumerate().map(|(i, f)| {
            let y = self.dialog_row_y(i + 1);
            (
                Id::Field(f.field),
                r(w - pad - DIALOG_FIELD_W, y, DIALOG_FIELD_W, 26.0),
                "",
            )
        });
        let mut v = vec![
            (Id::DialogClose, r(w - 10.0 - 24.0, 10.0, 24.0, 24.0), ""),
            (
                Id::DialogMinus,
                r(DIALOG_COUNTER_X, DIALOG_ROW_Y, 26.0, 26.0),
                "−",
            ),
            (
                Id::DialogPlus,
                r(w - pad - 26.0, DIALOG_ROW_Y, 26.0, 26.0),
                "+",
            ),
            (
                Id::DialogCancel,
                r(w - pad - start_w - 8.0 - cancel_w, by, cancel_w, bh),
                "Abbrechen",
            ),
            (
                Id::DialogStart,
                r(w - pad - start_w, by, start_w, bh),
                "Zeichnen beginnen",
            ),
        ];
        v.extend(fields);
        v
    }

    /// Grundbild des Dialogs: Kopfzeile, Zähler und (beim Darüberfahren über
    /// − oder +) der Hinweis, warum er gesperrt ist.
    pub(super) fn paint_dialog(&self, t: &Theme, c: &mut Canvas) {
        let (col, z, s) = (&t.ui, &t.size, self.scale);
        let m = (z.panel_shadow * s).round();
        let (regular, bold) = (self.fonts.regular.as_ref(), self.fonts.bold.as_ref());
        let x = m + z.panel_pad * s;
        let title = "Gebäude erstellen";
        let base = m + (z.panel_pad + 16.0) * s;
        widgets::text(
            c,
            bold.or(regular),
            title,
            z.font_title * s,
            x,
            base,
            col.text,
        );
        let row = m + DIALOG_ROW_Y * s;
        let mid = row + 13.0 * s;
        let cap = |px: f32| regular.map_or(px * 0.7, |f| f.cap_height(px));
        let px = z.font * s;
        let base = (mid + cap(px) * 0.5).round();
        widgets::text(c, regular, "Geschosse", px, x, base, col.text);
        // Zahl mittig zwischen − und +
        let (a, b) = (
            m + (DIALOG_COUNTER_X + 26.0) * s,
            m + (z.dialog_w - z.panel_pad - 26.0) * s,
        );
        let n = "2 (EG + OG)";
        let nw = regular.map_or(0.0, |f| f.width(n, px));
        widgets::text(c, regular, n, px, (a + b - nw) * 0.5, base, col.text);
        // Bezeichnungen der Felder
        for (i, f) in self.dialog_fields.iter().enumerate() {
            let mid = m + (self.dialog_row_y(i + 1) + 13.0) * s;
            let base = (mid + cap(px) * 0.5).round();
            widgets::text(c, regular, f.label, px, x, base, col.text);
        }
        // Eine Zeile unter den Feldern: Grund einer ungültigen Eingabe, sonst
        // beim Darüberfahren über − oder +, warum der Zähler fest ist
        let n = self.dialog_fields.len();
        let hb = m + (self.dialog_row_y(n + 1) + 13.0) * s;
        let error = self
            .edit
            .as_ref()
            .filter(|e| matches!(e.field, Field::Draft(_)))
            .and_then(|e| e.error.as_ref());
        if let Some(why) = error {
            let px = z.font_small * s;
            widgets::text(c, regular, why, px, x, hb, col.field_invalid);
        } else if matches!(self.hover, Some(Id::DialogMinus | Id::DialogPlus)) {
            let px = z.font_small * s;
            let hint = "Derzeit Erdgeschoss und Obergeschoss";
            widgets::text(c, regular, hint, px, x, hb, col.text_dim);
        }
    }

    /// Schließkreuz: zwei Striche, unter der Maus auf hellerem Grund.
    pub(super) fn paint_close(&self, t: &Theme, c: &mut Canvas, b: Rect, hover: bool) {
        let s = self.scale;
        if hover {
            let mut p = Path::new();
            p.rounded_rect(b.x, b.y, b.w, b.h, 6.0 * s);
            c.fill(&p, t.ui.hover);
        }
        let (cx, cy, d) = (b.x + b.w * 0.5, b.y + b.h * 0.5, 4.5 * s);
        let w = (1.5 * s).max(1.0);
        for (dx, dy) in [(d, d), (d, -d)] {
            let mut p = Path::new();
            p.segment((cx - dx, cy - dy), (cx + dx, cy + dy), w);
            c.fill(&p, t.ui.text);
        }
    }
}

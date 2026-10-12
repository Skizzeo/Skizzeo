//! Zeilen des Paneels „Werkzeuge“: Projektdaten, Wand- und Öffnungsknöpfe,
//! „Erweiterungen“ und „Gelände“, darunter die Vorgaben des laufenden
//! Werkzeugs oder die Leiste „Gelände“ (aus `ui.rs` ausgelagert).

use super::*;

fn opening_label(k: OpeningKind) -> &'static str {
    match k {
        OpeningKind::Window => "Fenster",
        OpeningKind::Door => "Tür",
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tool_rows(
    projekt: &[String],
    nord: bool,
    interior: bool,
    chip: bool,
    layers: &[(Rgba, String)],
    upper_active: bool,
    foundation_active: bool,
    ext: Option<(&ExtPanel, Option<&Edit>)>,
    ext_knopf: bool,
    opening: Option<(OpeningKind, bool)>,
    gelaende: Option<Vec<Row>>,
) -> Vec<Row> {
    // Projektdaten als eigener Abschnitt über den Werkzeugen (Paket PD-2)
    let mut rows = vec![Row::Button(Id::Projektdaten, "Projektdaten")];
    let n = projekt.len();
    rows.extend(
        projekt
            .iter()
            .enumerate()
            .map(|(i, z)| Row::Caption(z.clone(), i + 1 == n)),
    );
    rows.extend([
        Row::Separator,
        Row::Title("Werkzeuge"),
        // Wände und Öffnungen je ein Knopfpaar (Paket O3), damit das
        // Diagramm der Geschosse darunter nicht schrumpft
        Row::Pair([(Id::Building, "Gebäude"), (Id::Interior, "Innenwand")]),
        Row::Pair([
            (
                Id::Opening(OpeningKind::Window),
                opening_label(OpeningKind::Window),
            ),
            (
                Id::Opening(OpeningKind::Door),
                opening_label(OpeningKind::Door),
            ),
        ]),
    ]);
    // Leiste „Gelände“ (plan-gelaendemodul.md §4.7) statt der Wandvorgaben
    if let Some(g) = gelaende {
        rows.extend(g);
        return rows;
    }
    // Werkzeug „Erweiterungen“ (E6, Vertrag §14): Knopf des Bauteils, Typ,
    // Felder, woher die Maße kommen, drei feste Hinweise
    if let Some((x, edit)) = ext {
        rows.push(Row::Button(Id::Ext, x.name));
        if x.chip.is_some() {
            rows.push(Row::TypeChip(Id::ExtTyp));
        }
        for f in &x.felder {
            rows.push(Row::Field(f.field, f.label));
            match edit.filter(|e| e.field == f.field) {
                Some(e) => rows.extend(e.error.clone().map(Row::Error)),
                None => rows.extend(f.ausserhalb().map(Row::Error)),
            }
        }
        if let Some((last, first)) = x.aus_eingabe.split_last() {
            rows.extend(first.iter().map(|t| Row::Hint(t)));
            rows.push(Row::Text(last.to_string()));
        }
        rows.push(Row::Separator);
        rows.extend(x.hinweise.map(Row::Hint));
        return rows;
    }
    if ext_knopf {
        rows.push(Row::Button(Id::Ext, "Erweiterungen"));
    }
    // Immer da, auch ohne Freischaltung (§4.7)
    rows.push(Row::Button(Id::Gelaende(GelaendeKnopf::Leiste), "Gelände"));
    // Öffnungen (Paket O3): statt der Wandvorgaben drei kurze Zeilen, im
    // Grundriss Außenwand mit zwei Klicks und die Innentür (Jörn 09:26)
    if let Some((kind, plan)) = opening {
        rows.push(Row::Separator);
        rows.extend(if plan && kind == OpeningKind::Door {
            [
                Row::Hint("Innenwand: Klick setzt,"),
                Row::Hint("Tab: Bezug, Rad: Breite."),
                Row::Hint("Außenwand: Klick, Klick"),
            ]
        } else if plan {
            [
                Row::Hint("Klick auf die Außenwand,"),
                Row::Hint("Breite ziehen, Klick."),
                Row::Hint("Esc: zurück"),
            ]
        } else {
            [
                Row::Hint("Ecke auf der Außenwand,"),
                Row::Hint("dann die Gegenecke."),
                Row::Hint("Linien fangen, Esc: zurück"),
            ]
        });
        return rows;
    }
    // Zweizeilig ohne Abstand dazwischen, damit nichts am Rand abbricht
    if foundation_active {
        rows.push(Row::Hint("Im Fundament gibt es noch"));
        rows.push(Row::Text("nichts zu zeichnen".into()));
    } else if upper_active {
        rows.push(Row::Hint("Außenwände entstehen"));
        rows.push(Row::Text("aus dem EG".into()));
    }
    rows.extend([Row::Label(if interior {
        "Innenwand"
    } else {
        "Außenwand"
    })]);
    if chip {
        rows.push(Row::TypeChip(Id::ToolType));
    } else {
        rows.extend(layers.iter().map(|(c, t)| Row::Layer(*c, t.clone(), None)));
    }
    rows.extend([
        Row::Label("Bezugsseite"),
        Row::Segments([
            (Id::Ref(RefSide::Left), "Außen"),
            (Id::Ref(RefSide::Center), "Achse"),
            (Id::Ref(RefSide::Right), "Innen"),
        ]),
        // Winkelsprung (G2) ohne eigene Überschrift: gleiche Höhe wie der
        // frühere Knopf „90°-Sprung“, der Tooltip nennt ihn
        Row::Segments([
            (Id::Sprung(Sprung::R90), "90°"),
            (Id::Sprung(Sprung::R45), "45°"),
            (Id::Sprung(Sprung::Frei), "frei"),
        ]),
        Row::Separator,
    ]);
    // Drei kurze Zeilen (Paket 8, Darstellung §2.7); mehr in der Hilfe (F1).
    // Beim Aufziehen des Nordpfeils gilt Tab nicht (Hinweis W, S2).
    rows.extend(if nord {
        [
            Row::Hint("Fußpunkt, dann Richtung."),
            Row::Hint("Zahl + Enter setzt genau."),
            Row::Hint("Umschalt: 15°, Esc: zu"),
        ]
    } else {
        [
            Row::Hint("Klick setzt Punkte."),
            Row::Hint("Zahl + Enter setzt genau."),
            Row::Hint("Tab: Winkel, Esc: zu"),
        ]
    });
    rows
}

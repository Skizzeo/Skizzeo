# Skizzeo

3D-Gebäudemodellierer als native Rust-App. Keine externen Crates: Fenster,
Eingabe, OpenGL-Anbindung, 2D-Grafik, Logo und Oberfläche sind selbst geschrieben.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `sk-math` | Vektoren, Matrizen, Strahltests (f64, Millimeter) |
| `sk-paint` | Eigene 2D-Vektorgrafik mit Kantenglättung, eigener TrueType-Leser, PNG- und SVG-Ausgabe |
| `sk-ui` | SK-Logo als Vektor, Farbwerte, eigene Titelleiste, Paneele und Knöpfe |
| `sk-model` | Gebäudemodell als Datenbank: Bauteile mit Guid (IFC-Kurzform) und Nummer (AW-001 …), Arena mit Generationszähler, Baustoff- und Aufbau-Bibliothek, Geschoss; Wandzug mit Schichten, Bezugsseite, Gehrungen, Grundriss- und Senkrechtschnitt |
| `sk-platform` | Windows-Fenster ohne System-Titelleiste, Eingabe, OpenGL-Kontext (eigene Win32-FFI) |
| `sk-render` | OpenGL-3.3-Darstellung: Himmel, Boden, Flächen, Schraffuren, Kanten, Oberfläche |
| `app` | Programm `skizzeo`: Kamera, Navigation, Paneele, Gebäude-Eingabe, Gummiband, Schnittlinie |

## Bedienung

- Drehen: mittlere Maustaste ziehen (um den Punkt unter dem Mauszeiger)
- Verschieben: Umschalt + mittlere Maustaste ziehen
- Zoomen: Mausrad, zum Mauszeiger hin
- In Grundriss, Schnitt und Ansichten verschiebt die mittlere Maustaste

Titelleiste und Paneele sind dunkel. In kleineren Fenstern schrumpfen Paneele
und Knöpfe mit (voll ab 1440 × 810 dip, höchstens auf 60 %).

Paneel „Ansichten“ (rechts): 3D, Grundriss (geschnitten in 1,00 m Höhe),
Schnitt A–A, Vorne, Hinten, Links, Rechts. Alle außer 3D sind
Parallelprojektionen im Bauzeichnungs-Look: altweißes Papier, schwarze
Linien. Im Schnitt ist der tragende Kern (Gasbeton) breit umrandet, die
Dämmung mitteldick; Ansichtskanten mittel, Schichtfugen in Ansichten fein.
Geschnittenes Mauerwerk (Gasbeton) ist auf weißer Fläche schräg schraffiert,
harte Dämmung mit Zickzack.

Schnittlinie A–A im Grundriss (nach DIN 1356: Strichpunktlinie, kräftige
Enden, Pfeile in Blickrichtung, Kennbuchstabe): liegt zuerst in der
Modellmitte, lässt sich mit der linken Maustaste greifen und quer verschieben
(10-mm-Raster). Die Ansicht „Schnitt“ zeigt den Schnitt an dieser Stelle.

Knopf „Gebäude“ (Paneel „Werkzeuge“, links) startet die Außenwand-Eingabe.
Außenwand zweischalig, 31,5 cm: 14 cm Dämmung (WDVS) außen, 17,5 cm Gasbeton
innen, Höhe 3,50 m. Eingabe in 3D und im Grundriss:

- Linksklick setzt Punkte, die Wand wächst live am Cursor mit
- Klick auf den grünen Startpunkt schließt den Zug
- Doppelklick auf den letzten Punkt oder Enter beendet einen offenen Zug
- Tab oder Paneel: Bezugsseite außen (Standard, im Uhrzeigersinn links), innen, Achse
- R: 90°-Sprung ein/aus (standardmäßig an), Umschalt halten kehrt ihn kurz um
- Spurlinien durch den Startpunkt fangen den letzten Punkt rechtwinklig
- Rücktaste: letzten Punkt zurücknehmen, Esc: Zug abbrechen, nochmals Esc: Eingabe beenden
- Strg+Z / Strg+Y: rückgängig / wiederholen

Gummiband (violett am äußeren Wandfuß, in 3D und im Grundriss; erscheint,
sobald der Mauszeiger über einem Wandsegment steht):

- Segment mit der linken Maustaste greifen und quer ziehen (10-mm-Raster)
- Die Wand geht live mit, die Nachbarwände behalten ihre Richtung
- Esc während des Ziehens: abbrechen
- Im Schnitt und in den Ansichten lassen sich die Wände ziehen, die vom
  Betrachter weg laufen: Ihr Fuß erscheint beim Darüberfahren als violette
  Kugel. Verdeckte Wände hinter der Fassade sind nicht greifbar.

Auswahl und Eigenschaften (ohne aktive Gebäude-Eingabe):

- Klick auf eine Wand wählt sie, in 3D, Grundriss, Schnitt und Ansichten;
  ihr Umriss erscheint in Akzentfarbe
- Rechts unter „Ansichten“ zeigt das Paneel „Eigenschaften“ Nummer, Kategorie,
  Geschoss, Länge, Dicke, Höhe, Flächen außen und innen, Volumen und je Schicht
  Volumen und Masse (brutto, aus der Parametrik in `sk-model::qto`)
- Klick ins Leere oder Esc hebt die Auswahl auf; nach Rückgängig verschwindet
  sie, wenn es die Wand nicht mehr gibt

## Bauen

    cargo build --release

Das Programm liegt danach unter `target/release/skizzeo.exe`.
`skizzeo.exe --screenshot bild.png` speichert das erste Bild und beendet sich.
`skizzeo.exe --zeiten zeiten.csv` schreibt für jedes Bild die Dauer in
Millisekunden mit (Ereignisse, Netz, Zeichnen, Tauschen, gesamt). Ziel bei
Echtzeit-Interaktionen wie Griff-Ziehen: 5–15 ms je Bild.

Leistungsmessung ohne Fenster (Ziehen, Netze, Greifen bei 1 bis 1000 Häusern):

    cargo test --release -p skizzeo perf -- --ignored --nocapture

Logos neu erzeugen: `cargo run -p sk-ui --example logos -- logos`

Schriften kommen aus dem Windows-Schriftenordner (Segoe UI, sonst Arial oder
Tahoma) und werden mit dem eigenen TrueType-Leser gezeichnet.

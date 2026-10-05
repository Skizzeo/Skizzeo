# Skizzeo

3D-Gebäudemodellierer als native Rust-App. Keine externen Crates: Fenster,
Eingabe, OpenGL-Anbindung, 2D-Grafik, Logo und Oberfläche sind selbst geschrieben.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `sk-math` | Vektoren, Matrizen, Strahltests (f64, Millimeter) |
| `sk-paint` | Eigene 2D-Vektorgrafik mit Kantenglättung, PNG- und SVG-Ausgabe |
| `sk-ui` | SK-Logo als Vektor, Farbwerte, eigene Titelleiste mit Fensterknöpfen |
| `sk-model` | Gebäudemodell: Wandzug mit Bezugsseite, Gehrungen, Dicke und Höhe |
| `sk-platform` | Windows-Fenster ohne System-Titelleiste, Eingabe, OpenGL-Kontext (eigene Win32-FFI) |
| `sk-render` | OpenGL-3.3-Darstellung: Himmel, Boden, Flächen, Kanten, Oberfläche |
| `app` | Programm `skizzeo`: Kamera, Navigation, Wandzug, Gummiband |

## Bedienung

- Drehen: mittlere Maustaste ziehen (um den Punkt unter dem Mauszeiger)
- Verschieben: Umschalt + mittlere Maustaste ziehen
- Zoomen: Mausrad, zum Mauszeiger hin

Wandzug (Wanddicke 40 cm, Höhe 3,50 m):

- Linksklick setzt Punkte, die Wand wächst live am Cursor mit
- Klick auf den grünen Startpunkt schließt den Zug
- Doppelklick auf den letzten Punkt oder Enter beendet einen offenen Zug
- Tab: Bezugsseite links (Standard, im Uhrzeigersinn außen), rechts, Mitte
- R: 90°-Sprung ein/aus, Umschalt halten kehrt ihn kurz um
- Spurlinien durch den Startpunkt fangen den letzten Punkt rechtwinklig
- Rücktaste: letzten Punkt zurücknehmen, Esc: abbrechen
- Strg+Z / Strg+Y: rückgängig / wiederholen

Gummiband (violett am äußeren Wandfuß):

- Segment mit der linken Maustaste greifen und quer ziehen (10-mm-Raster)
- Die Wand geht live mit, die Nachbarwände behalten ihre Richtung
- Esc während des Ziehens: abbrechen

## Bauen

    cargo build --release

Das Programm liegt danach unter `target/release/skizzeo.exe`.
`skizzeo.exe --screenshot bild.png` speichert das erste Bild und beendet sich.

Logos neu erzeugen: `cargo run -p sk-ui --example logos -- logos`

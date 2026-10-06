# Skizzeo

3D-Gebäudemodellierer als native Rust-App. Keine externen Crates: Fenster,
Eingabe, OpenGL-Anbindung, 2D-Grafik, Logo und Oberfläche sind selbst geschrieben.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `sk-math` | Vektoren, Matrizen, Strahltests (f64, Millimeter) |
| `sk-paint` | Eigene 2D-Vektorgrafik mit Kantenglättung, eigener TrueType-Leser, PNG- und SVG-Ausgabe |
| `sk-ui` | SK-Logo als Vektor, Farbwerte, eigene Titelleiste, Paneele und Knöpfe |
| `sk-model` | Gebäudemodell: Wandzug mit Schichten, Bezugsseite, Gehrungen, Grundriss- und Senkrechtschnitt |
| `sk-platform` | Windows-Fenster ohne System-Titelleiste, Eingabe, OpenGL-Kontext (eigene Win32-FFI) |
| `sk-render` | OpenGL-3.3-Darstellung: Himmel, Boden, Flächen, Kanten, Oberfläche |
| `app` | Programm `skizzeo`: Kamera, Navigation, Paneele, Gebäude-Eingabe, Gummiband |

## Bedienung

- Drehen: mittlere Maustaste ziehen (um den Punkt unter dem Mauszeiger)
- Verschieben: Umschalt + mittlere Maustaste ziehen
- Zoomen: Mausrad, zum Mauszeiger hin
- In Grundriss, Schnitt und Ansichten verschiebt die mittlere Maustaste

Paneel „Ansichten“ (rechts): 3D, Grundriss (geschnitten in 1,00 m Höhe),
Schnitt (senkrecht durch die Modellmitte, Blick nach Norden), Vorne, Hinten,
Links, Rechts. Alle außer 3D sind Parallelprojektionen.

Knopf „Gebäude“ (Paneel „Werkzeuge“, links) startet die Außenwand-Eingabe.
Außenwand zweischalig, 31,5 cm: 14 cm Dämmung (WDVS) außen, 17,5 cm Gasbeton
innen, Höhe 3,50 m. Eingabe in 3D und im Grundriss:

- Linksklick setzt Punkte, die Wand wächst live am Cursor mit
- Klick auf den grünen Startpunkt schließt den Zug
- Doppelklick auf den letzten Punkt oder Enter beendet einen offenen Zug
- Tab oder Paneel: Bezugsseite außen (Standard, im Uhrzeigersinn links), innen, Achse
- R: 90°-Sprung ein/aus, Umschalt halten kehrt ihn kurz um
- Spurlinien durch den Startpunkt fangen den letzten Punkt rechtwinklig
- Rücktaste: letzten Punkt zurücknehmen, Esc: Zug abbrechen, nochmals Esc: Eingabe beenden
- Strg+Z / Strg+Y: rückgängig / wiederholen

Gummiband (violett am äußeren Wandfuß, in 3D und im Grundriss):

- Segment mit der linken Maustaste greifen und quer ziehen (10-mm-Raster)
- Die Wand geht live mit, die Nachbarwände behalten ihre Richtung
- Esc während des Ziehens: abbrechen

## Bauen

    cargo build --release

Das Programm liegt danach unter `target/release/skizzeo.exe`.
`skizzeo.exe --screenshot bild.png` speichert das erste Bild und beendet sich.

Logos neu erzeugen: `cargo run -p sk-ui --example logos -- logos`

Schriften kommen aus dem Windows-Schriftenordner (Segoe UI, sonst Arial oder
Tahoma) und werden mit dem eigenen TrueType-Leser gezeichnet.

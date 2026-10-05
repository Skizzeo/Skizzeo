# Skizzeo

3D-Gebäudemodellierer als native Rust-App. Keine externen Crates: Fenster,
Eingabe, OpenGL-Anbindung, 2D-Grafik, Logo und Oberfläche sind selbst geschrieben.

## Aufbau

| Crate | Aufgabe |
|---|---|
| `sk-math` | Vektoren, Matrizen, Strahltests (f64, Millimeter) |
| `sk-paint` | Eigene 2D-Vektorgrafik mit Kantenglättung, PNG- und SVG-Ausgabe |
| `sk-ui` | SK-Logo als Vektor, Farbwerte, eigene Titelleiste mit Fensterknöpfen |
| `sk-platform` | Windows-Fenster ohne System-Titelleiste, Eingabe, OpenGL-Kontext (eigene Win32-FFI) |
| `sk-render` | OpenGL-3.3-Darstellung: Himmel, Boden, Flächen, Kanten, Oberfläche |
| `app` | Programm `skizzeo`: Kamera, Navigation, Testkörper |

## Bedienung

- Drehen: mittlere Maustaste ziehen (um den Punkt unter dem Mauszeiger)
- Verschieben: Umschalt + mittlere Maustaste ziehen
- Zoomen: Mausrad, zum Mauszeiger hin

## Bauen

    cargo build --release

Das Programm liegt danach unter `target/release/skizzeo.exe`.
`skizzeo.exe --screenshot bild.png` speichert das erste Bild und beendet sich.

Logos neu erzeugen: `cargo run -p sk-ui --example logos -- logos`

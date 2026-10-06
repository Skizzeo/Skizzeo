//! Schreibt die Logos als SVG (Vektor) und als hochaufgelöste PNG-Vorschau.
//! Aufruf: cargo run -p sk-ui --example logos -- <Zielordner>

use sk_ui::{logo, titlebar::TitleBar};
use std::{fs, path::PathBuf};

fn main() -> std::io::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "logos".into()));
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("skizzeo-titelleiste.svg"), logo::svg_titlebar())?;
    fs::write(dir.join("skizzeo-programm.svg"), logo::svg_app())?;
    fs::write(
        dir.join("skizzeo-titelleiste-1024.png"),
        logo::titlebar_logo(1024).to_png(),
    )?;
    fs::write(
        dir.join("skizzeo-programm-1024.png"),
        logo::app_icon(1024).to_png(),
    )?;
    for s in [16, 32, 48, 256] {
        fs::write(
            dir.join(format!("skizzeo-programm-{s}.png")),
            logo::app_icon(s).to_png(),
        )?;
    }
    let mut t = TitleBar::new(1.5);
    fs::write(
        dir.join("titelleiste-vorschau.png"),
        t.paint(&sk_ui::theme::Theme::dark(), 900).to_png(),
    )?;
    t.maximized = true;
    t.hover = Some(sk_ui::titlebar::Button::Close);
    fs::write(
        dir.join("titelleiste-vorschau-max-hover.png"),
        t.paint(&sk_ui::theme::Theme::dark(), 900).to_png(),
    )?;
    Ok(())
}

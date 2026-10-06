//! Programmeinstellungen des Nutzers: `%APPDATA%\Skizzeo\einstellungen.txt`.
//!
//! Gleiche Zeilensyntax wie `.szo` ([`sk_model::szo`]), eigener Kopf.
//! Gespeichert werden nur Abweichungen vom Grundschema, damit Verbesserungen
//! am Standardschema auch bei bestehenden Nutzern ankommen. Lesen bricht nie
//! ab: Unbekanntes und Kaputtes wird übersprungen und als Hinweis gesammelt.

use sk_model::szo::{check_header, hex, parse_hex, Line, Record};
use sk_paint::Rgba;
use sk_ui::theme::Theme;
use std::path::PathBuf;

const HEAD: &str = "SKIZZEO-EINSTELLUNGEN";
const VERSION: u32 = 1;

type RgbaRole = (&'static str, fn(&mut Theme) -> &mut Rgba);
type F4Role = (&'static str, fn(&mut Theme) -> &mut [f32; 4]);
type SizeRole = (&'static str, fn(&mut Theme) -> &mut f32);

const RGBA_ROLES: [RgbaRole; 25] = [
    ("ui.bg", |t| &mut t.ui.bg),
    ("ui.border", |t| &mut t.ui.border),
    ("ui.field", |t| &mut t.ui.field),
    ("ui.text", |t| &mut t.ui.text),
    ("ui.text_dim", |t| &mut t.ui.text_dim),
    ("ui.on_accent", |t| &mut t.ui.on_accent),
    ("ui.accent", |t| &mut t.ui.accent),
    ("ui.accent_hover", |t| &mut t.ui.accent_hover),
    ("ui.hover", |t| &mut t.ui.hover),
    ("ui.pressed", |t| &mut t.ui.pressed),
    ("ui.shadow", |t| &mut t.ui.shadow),
    ("title.bg", |t| &mut t.title.bg),
    ("title.glyph", |t| &mut t.title.glyph),
    ("title.glyph_inactive", |t| &mut t.title.glyph_inactive),
    ("title.hover", |t| &mut t.title.hover),
    ("title.pressed", |t| &mut t.title.pressed),
    ("title.close_hover", |t| &mut t.title.close_hover),
    ("title.close_pressed", |t| &mut t.title.close_pressed),
    ("title.close_glyph_hover", |t| {
        &mut t.title.close_glyph_hover
    }),
    ("title.logo", |t| &mut t.title.logo),
    ("env.ground", |t| &mut t.env.ground),
    ("env.face", |t| &mut t.env.face),
    ("env.edge", |t| &mut t.env.edge),
    ("env.paper_fallback", |t| &mut t.env.paper_fallback),
    ("env.fill_fallback", |t| &mut t.env.fill_fallback),
];

const F4_ROLES: [F4Role; 10] = [
    ("interact.select", |t| &mut t.interact.select),
    ("interact.draw", |t| &mut t.interact.draw),
    ("interact.track", |t| &mut t.interact.track),
    ("interact.guide", |t| &mut t.interact.guide),
    ("interact.start", |t| &mut t.interact.start),
    ("interact.drag", |t| &mut t.interact.drag),
    ("interact.drag_hot", |t| &mut t.interact.drag_hot),
    ("interact.drag_ghost", |t| &mut t.interact.drag_ghost),
    ("interact.shadow_tool", |t| &mut t.interact.shadow_tool),
    ("interact.shadow_band", |t| &mut t.interact.shadow_band),
];

const SIZE_ROLES: [SizeRole; 11] = [
    ("corner_radius", |t| &mut t.size.corner_radius),
    ("font", |t| &mut t.size.font),
    ("font_small", |t| &mut t.size.font_small),
    ("font_detail", |t| &mut t.size.font_detail),
    ("font_title", |t| &mut t.size.font_title),
    ("font_mark", |t| &mut t.size.font_mark),
    ("outline", |t| &mut t.size.outline),
    ("panel_margin", |t| &mut t.size.panel_margin),
    ("panel_pad", |t| &mut t.size.panel_pad),
    ("panel_width", |t| &mut t.size.panel_width),
    ("panel_shadow", |t| &mut t.size.panel_shadow),
];

/// Grundschema zu einem Namen.
fn base(name: &str) -> Option<Theme> {
    let t = Theme::dark();
    (t.name == name).then_some(t)
}

fn rgba_hex(c: Rgba) -> String {
    let s = hex([c.0, c.1, c.2]);
    if c.3 == 255 {
        s
    } else {
        format!("{s}{:02x}", c.3)
    }
}

fn parse_rgba(s: &str) -> Option<Rgba> {
    match s.len() {
        6 => parse_hex(s).map(Rgba::from_rgb8),
        8 => {
            let [r, g, b] = parse_hex(&s[..6])?;
            let a = u8::from_str_radix(s.get(6..)?, 16).ok()?;
            Some(Rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_f4(s: &str) -> Option<[f32; 4]> {
    let v: Vec<f32> = s
        .split(',')
        .map(|x| x.parse().ok().filter(|v: &f32| v.is_finite()))
        .collect::<Option<_>>()?;
    v.try_into().ok()
}

/// Das Schema als Einstellungsdatei: nur die Abweichungen von seinem Grundschema.
pub fn write(theme: &Theme) -> String {
    let mut out = format!("{HEAD} {VERSION}\n");
    let mut b = base(&theme.name).unwrap_or_else(Theme::dark);
    let mut t = theme.clone();
    Line::new("theme").text("base", &b.name).finish(&mut out);
    for (name, f) in RGBA_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            Line::new("color")
                .word("role", name)
                .word("value", &rgba_hex(v))
                .finish(&mut out);
        }
    }
    for (name, f) in F4_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            let s = v.map(|x| x.to_string()).join(",");
            Line::new("color")
                .word("role", name)
                .word("value", &s)
                .finish(&mut out);
        }
    }
    for (name, f) in SIZE_ROLES {
        let v = *f(&mut t);
        if v != *f(&mut b) {
            Line::new("size")
                .word("key", name)
                .num("value", v)
                .finish(&mut out);
        }
    }
    if t.px_per_mm != b.px_per_mm {
        Line::new("screen")
            .num("px_per_mm", t.px_per_mm)
            .finish(&mut out);
    }
    if t.env.horizon_softness != b.env.horizon_softness {
        Line::new("env")
            .num("horizon_softness", t.env.horizon_softness)
            .finish(&mut out);
    }
    if t.env.sky != b.env.sky {
        for (k, c) in &t.env.sky {
            Line::new("sky")
                .num("t", k)
                .word("value", &rgba_hex(*c))
                .finish(&mut out);
        }
    }
    out
}

/// Liest die Einstellungen. Nie ein Abbruch: was nicht passt, wird übersprungen
/// und als Hinweis gemeldet.
pub fn read(text: &str) -> (Theme, Vec<String>) {
    let mut hints = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if let Err(e) = check_header(text.lines().next(), HEAD, VERSION) {
        hints.push(format!("Einstellungen nicht gelesen: {e}"));
        return (Theme::dark(), hints);
    }
    let mut t = Theme::dark();
    let mut sky = Vec::new();
    let mut changed = false;
    for (i, l) in text.lines().enumerate().skip(1) {
        let r = match Record::parse(i + 1, l) {
            Ok(Some(r)) => r,
            Ok(None) => continue,
            Err(e) => {
                hints.push(e.to_string());
                continue;
            }
        };
        let skip = |hints: &mut Vec<String>, what: &str| {
            hints.push(format!("Zeile {}: {what} übersprungen", r.line));
        };
        match r.section.as_str() {
            "theme" => match r.opt("base").map(|n| (n, base(n))) {
                Some((_, Some(b))) => t = b,
                Some((n, None)) => skip(&mut hints, &format!("unbekanntes Grundschema „{n}“")),
                None => skip(&mut hints, "[theme] ohne base"),
            },
            "color" => {
                let (role, value) = (r.opt("role").unwrap_or(""), r.opt("value").unwrap_or(""));
                if let Some((_, f)) = RGBA_ROLES.iter().find(|x| x.0 == role) {
                    match parse_rgba(value) {
                        Some(c) => *f(&mut t) = c,
                        None => skip(&mut hints, &format!("Farbe „{value}“ für {role}")),
                    }
                } else if let Some((_, f)) = F4_ROLES.iter().find(|x| x.0 == role) {
                    match parse_f4(value) {
                        Some(c) => *f(&mut t) = c,
                        None => skip(&mut hints, &format!("Farbe „{value}“ für {role}")),
                    }
                } else {
                    skip(&mut hints, &format!("unbekannte Rolle „{role}“"));
                }
            }
            "size" => {
                let key = r.opt("key").unwrap_or("");
                match (SIZE_ROLES.iter().find(|x| x.0 == key), r.f32("value")) {
                    (Some((_, f)), Ok(v)) => *f(&mut t) = v,
                    (None, _) => skip(&mut hints, &format!("unbekannte Größe „{key}“")),
                    (_, Err(_)) => skip(&mut hints, &format!("Wert für Größe „{key}“")),
                }
            }
            "screen" => match r.f32("px_per_mm") {
                Ok(v) if v > 0.0 => t.px_per_mm = v,
                _ => skip(&mut hints, "px_per_mm"),
            },
            "env" => match r.f32("horizon_softness") {
                Ok(v) => t.env.horizon_softness = v,
                Err(_) => skip(&mut hints, "horizon_softness"),
            },
            "sky" => match (r.f32("t"), r.opt("value").and_then(parse_rgba)) {
                (Ok(k), Some(c)) => sky.push((k, c)),
                _ => skip(&mut hints, "Stützstelle des Himmels"),
            },
            s => {
                skip(&mut hints, &format!("unbekannter Abschnitt [{s}]"));
                continue;
            }
        }
        changed = true;
        r.unused(&mut hints);
    }
    if !sky.is_empty() {
        sky.sort_by(|a, b| a.0.total_cmp(&b.0));
        t.env.sky = sky;
    }
    // Ein vom Standard abweichendes Schema bekommt einen eigenen Stand
    if changed && t != Theme::dark() {
        t.rev = 1;
    }
    (t, hints)
}

/// Ort der Einstellungsdatei und Stand beim Laden.
pub struct Settings {
    /// `None` mit `--ohne-einstellungen` oder ohne `APPDATA`.
    pub path: Option<PathBuf>,
    loaded_rev: u64,
    /// Übersprungenes beim Lesen (später im Einstellungsfenster).
    pub hints: Vec<String>,
}

impl Settings {
    pub fn new(args: impl Iterator<Item = String>, appdata: Option<PathBuf>) -> Settings {
        let off = args.skip(1).any(|a| a == "--ohne-einstellungen");
        Settings {
            path: appdata
                .filter(|_| !off)
                .map(|d| d.join("Skizzeo").join("einstellungen.txt")),
            loaded_rev: 0,
            hints: Vec::new(),
        }
    }

    /// Liest das Schema; fehlt die Datei, gilt das dunkle Standardschema.
    pub fn load(&mut self) -> Theme {
        let text = self
            .path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let theme = match text {
            Some(text) => {
                let (t, hints) = read(&text);
                self.hints = hints;
                t
            }
            None => Theme::dark(),
        };
        self.loaded_rev = theme.rev;
        theme
    }

    /// Schreibt atomar, wenn sich das Schema seit dem Laden geändert hat.
    pub fn save_if_changed(&mut self, theme: &Theme) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if theme.rev == self.loaded_rev {
            return Ok(());
        }
        let tmp = path.with_extension("txt.tmp");
        let res = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&tmp, write(theme)))
            .and_then(|_| std::fs::rename(&tmp, path));
        match res {
            Ok(()) => {
                self.loaded_rev = theme.rev;
                Ok(())
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(format!("Einstellungen nicht gespeichert: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("skizzeo-einst-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn args(v: &[&str]) -> impl Iterator<Item = String> {
        v.iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn standard_ohne_datei() {
        let d = dir("ohne");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        assert_eq!(s.load(), Theme::dark());
        assert!(s.hints.is_empty());
        // Unverändert: nichts geschrieben
        s.save_if_changed(&Theme::dark()).unwrap();
        assert!(!d.exists());
        assert_eq!(
            write(&Theme::dark()),
            "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n"
        );
    }

    #[test]
    fn nur_die_abweichung_wird_gespeichert() {
        let d = dir("abweichung");
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let mut t = s.load();
        t.ui.border = Rgba::rgb(40, 120, 220);
        t.rev += 1;
        s.save_if_changed(&t).unwrap();
        let path = d.join("Skizzeo").join("einstellungen.txt");
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "SKIZZEO-EINSTELLUNGEN 1\n[theme] base=\"Dunkel\"\n[color] role=ui.border value=2878dc\n"
        );
        assert!(!path.with_extension("txt.tmp").exists());
        let mut s2 = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        let back = s2.load();
        assert!(s2.hints.is_empty(), "{:?}", s2.hints);
        assert_eq!(Theme { rev: t.rev, ..back }, t);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn alle_arten_im_rundlauf() {
        let mut t = Theme::dark();
        t.set_accent(Rgba::rgb(40, 120, 220));
        t.ui.shadow = Rgba(0, 0, 0, 30);
        t.interact.track = [0.1, 0.2, 0.3, 0.75];
        t.size.font = 15.5;
        t.px_per_mm = 6.0;
        t.env.horizon_softness = 2.0;
        t.env.sky = vec![(0.0, Rgba::rgb(1, 2, 3)), (1.0, Rgba(4, 5, 6, 7))];
        let text = write(&t);
        assert!(
            text.contains("[color] role=ui.shadow value=0000001e\n"),
            "{text}"
        );
        assert!(
            text.contains("[color] role=interact.track value=0.1,0.2,0.3,0.75\n"),
            "{text}"
        );
        let (back, hints) = read(&text);
        assert!(hints.is_empty(), "{hints:?}");
        assert_eq!(write(&back), text);
        assert_eq!(Theme { rev: t.rev, ..back }, t);
    }

    #[test]
    fn kaputtes_wird_uebersprungen() {
        let text = "SKIZZEO-EINSTELLUNGEN 1\n\
            [theme] base=\"Dunkel\"\n\
            [color] role=ui.gibtsnicht value=ffffff\n\
            [color] role=ui.accent value=zz00zz\n\
            [color] role=ui.border value=2878dc\n";
        let (t, hints) = read(text);
        assert_eq!(hints.len(), 2, "{hints:?}");
        assert!(hints[0].contains("Zeile 3"), "{hints:?}");
        assert!(hints[1].contains("Zeile 4"), "{hints:?}");
        assert_eq!(t.ui.border, Rgba::rgb(40, 120, 220));
        assert_eq!(t.ui.accent, Theme::dark().ui.accent);
        // Fremde Datei oder neuere Version: Standardschema und ein Hinweis
        let (t, hints) = read("SKIZZEO-EINSTELLUNGEN 9\n[color] role=ui.border value=000000\n");
        assert_eq!(t, Theme::dark());
        assert_eq!(hints.len(), 1);
        let (t, hints) = read("");
        assert_eq!((t, hints.len()), (Theme::dark(), 1));
    }

    #[test]
    fn ohne_einstellungen_wird_nichts_gelesen_oder_geschrieben() {
        let d = dir("aus");
        let path = d.join("Skizzeo").join("einstellungen.txt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "SKIZZEO-EINSTELLUNGEN 1\n[size] key=font value=20\n").unwrap();
        let mut s = Settings::new(
            args(&["skizzeo.exe", "--ohne-einstellungen"]),
            Some(d.clone()),
        );
        assert!(s.path.is_none());
        let mut t = s.load();
        assert_eq!(t, Theme::dark());
        t.set_accent(Rgba::rgb(1, 2, 3));
        s.save_if_changed(&t).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "SKIZZEO-EINSTELLUNGEN 1\n[size] key=font value=20\n");
        // Ohne den Schalter wird die Datei gelesen
        let mut s = Settings::new(args(&["skizzeo.exe"]), Some(d.clone()));
        assert_eq!(s.load().size.font, 20.0);
        let _ = std::fs::remove_dir_all(&d);
    }
}

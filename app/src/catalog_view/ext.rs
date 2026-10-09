//! Reiter „Erweiterungen“ im Bauteilkatalog (E6, Vertrag §14): die
//! eingelesenen Bauteile nach Gruppe › Name, rechts ihre Angaben und
//! „Einfügen“, das den Katalog schließt und das Werkzeug startet.

use super::{label, Btn, Catalog, Item, ListLayout, Out, Target, CONTENT_X, PAD};
use crate::prefs::Win;
use sk_model::erweiterung::{anzeige, ANZEIGE_MAX};
use sk_model::ExtDef;
use sk_paint::Canvas;
use sk_ui::theme::Theme;
use sk_ui::widgets::{self, Fonts, Rect};

/// Ein Bauteil der Liste: Definition und ob sie im Ordner „Erweiterungen“
/// liegt (sonst nur im Projekt, etwa aus einer fremden Datei).
#[derive(Clone, Debug)]
pub(super) struct Eintrag {
    pub def: ExtDef,
    pub im_ordner: bool,
}

/// Wie das Bauteil gesetzt wird, in Worten.
fn art(d: &ExtDef) -> String {
    let a = match d.einfuegen() {
        "linie" => "Linie (Anfang und Ende)",
        "rechteck" => "Rechteck (zwei Ecken)",
        _ => "Punkt",
    };
    if d.drehen() {
        format!("{a}, drehbar")
    } else {
        a.to_string()
    }
}

impl Catalog {
    /// Bauteile aus dem Ordner „Erweiterungen“; dazu die, die nur das
    /// Projekt kennt. Reihenfolge wie im Werkzeug: Gruppe, dann Name.
    pub fn set_ext(&mut self, ordner: &[ExtDef]) {
        let mut v: Vec<Eintrag> = ordner
            .iter()
            .map(|d| Eintrag {
                def: d.clone(),
                im_ordner: true,
            })
            .collect();
        for d in self.work.ext_defs() {
            if !ordner.iter().any(|o| o.key == d.key) {
                v.push(Eintrag {
                    def: d.clone(),
                    im_ordner: false,
                });
            }
        }
        v.sort_by(|a, b| {
            (a.def.gruppe_rang(), a.def.name()).cmp(&(b.def.gruppe_rang(), b.def.name()))
        });
        self.ext_sel = (!v.is_empty()).then_some(0);
        self.ext = v;
    }

    /// Liste links: je Gruppe ein Kopf, darunter die Bauteile.
    pub(super) fn ext_layout(&self, t: &Theme, w: &Win) -> ListLayout {
        let s = w.scale;
        let body = self.list_body(t, w);
        let tile_h = t.size.catalog_tile_h * s;
        let mut y = body.y - self.scroll;
        let (mut tiles, mut heads) = (Vec::new(), Vec::new());
        let mut gruppe = None;
        for (i, e) in self.ext.iter().enumerate() {
            let g = e.def.gruppe();
            if gruppe != Some(g) {
                if gruppe.is_some() {
                    y += 10.0 * s;
                }
                gruppe = Some(g);
                heads.push((g.to_string(), y));
                y += 24.0 * s;
            }
            let r = Rect::new(
                body.x + 10.0 * s,
                y.round(),
                (t.size.catalog_list_w - 8.0) * s,
                tile_h,
            );
            tiles.push((Item::Ext(i), r));
            y += tile_h + 4.0 * s;
        }
        let content = y + self.scroll - body.y;
        (tiles, heads, content)
    }

    /// Knopf „Einfügen“ rechts unter den Angaben.
    pub(super) fn ext_button(&self, t: &Theme, w: &Win) -> Option<Rect> {
        self.ext_sel?;
        Some(self.r(t, w, CONTENT_X, 404.0, 140.0, 32.0))
    }

    /// „Einfügen“: Katalog zu, Werkzeug mit dem Bauteil. Ungespeicherte
    /// Änderungen an Typen gingen dabei verloren, also erst OK oder
    /// Abbrechen.
    pub(super) fn ext_einfuegen(&mut self, out: &mut Out) {
        let Some(e) = self.ext_sel.and_then(|i| self.ext.get(i)) else {
            return;
        };
        if self.pending() {
            self.message = Some("Erst die Änderungen mit OK übernehmen oder abbrechen".into());
            self.blink = Some(self.clock());
            return;
        }
        out.ext_einfuegen = Some(e.def.key.clone());
        out.closed = true;
    }

    /// Kachel: Name, darunter Version und Herkunft, rechts die Zahl im
    /// Projekt.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_ext_tile(
        &self,
        c: &mut Canvas,
        i: usize,
        r: Rect,
        t: &Theme,
        fonts: &Fonts,
        w: &Win,
        sel: bool,
        hover: bool,
    ) {
        let Some(e) = self.ext.get(i) else {
            return;
        };
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        if sel {
            super::rounded(c, r, 6.0 * s, u.pressed);
            c.fill_rect(
                r.x,
                r.y + 4.0 * s,
                (3.0 * s).round(),
                r.h - 8.0 * s,
                u.accent,
            );
        } else if hover {
            super::rounded(c, r, 6.0 * s, u.hover);
        }
        let tx = r.x + 16.0 * s;
        let right = r.x + r.w - 10.0 * s;
        let uses = self.work.ext_uses(&e.def.key).len();
        let px = t.size.font * s;
        let font = if sel { bold } else { regular };
        let max = right - tx - if uses > 0 { 30.0 * s } else { 0.0 };
        let name = widgets::ellipsize(font, &anzeige(e.def.name(), 80), px, max);
        label(c, font, &name, px, tx, r.y + 20.0 * s, u.text);
        let pd = t.size.font_detail * s;
        let detail = format!(
            "Version {} · {}",
            e.def.version,
            if e.im_ordner {
                "eingelesen"
            } else {
                "nur im Projekt"
            }
        );
        let detail = widgets::ellipsize(regular, &detail, pd, right - tx);
        label(c, regular, &detail, pd, tx, r.y + 38.0 * s, u.text_dim);
        if uses > 0 {
            let text = format!("{uses}×");
            let uw = regular.map_or(0.0, |ft| ft.width(&text, pd));
            label(
                c,
                regular,
                &text,
                pd,
                right - uw,
                r.y + 18.0 * s,
                u.text_dim,
            );
        }
    }

    /// Rechte Seite: Angaben des gewählten Bauteils und „Einfügen“.
    pub(super) fn paint_ext(&self, c: &mut Canvas, t: &Theme, fonts: &Fonts, w: &Win) {
        let s = w.scale;
        let u = &t.ui;
        let (regular, bold) = (
            fonts.regular.as_ref(),
            fonts.bold.as_ref().or(fonts.regular.as_ref()),
        );
        let small = t.size.font_small * s;
        let px = t.size.font * s;
        let x0 = self.r(t, w, CONTENT_X, 0.0, 0.0, 0.0).x;
        let y = self.r(t, w, 0.0, 100.0, 0.0, 0.0).y;
        let f = self.frame(t, w);
        let max = f.x + f.w - PAD * s - x0;
        let Some(e) = self.ext_sel.and_then(|i| self.ext.get(i)) else {
            label(
                c,
                bold,
                "Keine Erweiterungen",
                t.size.font_title * s,
                x0,
                y,
                u.text,
            );
            let text =
                "Bauteile aus der Werkbank (.szb) erscheinen hier, sobald sie eingelesen sind.";
            for (k, z) in widgets::wrap(regular, text, small, max).iter().enumerate() {
                let y = y + (26.0 + 18.0 * k as f32) * s;
                label(c, regular, z, small, x0, y, u.text_dim);
            }
            return;
        };
        let d = &e.def;
        let name = anzeige(d.name(), 80);
        let titel = widgets::ellipsize(bold, &name, t.size.font_title * s, max);
        label(c, bold, &titel, t.size.font_title * s, x0, y, u.text);
        let pfad = format!("Bauteilkatalog › Erweiterungen › {} › {}", d.gruppe(), name);
        let pfad = widgets::ellipsize(regular, &pfad, small, max);
        label(c, regular, &pfad, small, x0, y + 24.0 * s, u.text_dim);
        let mut yy = y + 64.0 * s;
        let beschreibung = anzeige(
            d.def.bauteil_feld("beschreibung").unwrap_or(""),
            ANZEIGE_MAX,
        );
        for z in widgets::wrap(regular, &beschreibung, px, max)
            .iter()
            .take(3)
        {
            label(c, regular, z, px, x0, yy, u.text);
            yy += 20.0 * s;
        }
        yy += 12.0 * s;
        let typen: Vec<String> = d
            .def
            .typ
            .iter()
            .map(|t| anzeige(t.get("name").unwrap_or(t.key()), 80))
            .collect();
        let autor = anzeige(d.def.bauteil_feld("autor").unwrap_or(""), 80);
        let n = self.work.ext_uses(&d.key).len();
        let zeilen = [
            ("Setzen", art(d)),
            (
                "Typen",
                if typen.is_empty() {
                    "keine".to_string()
                } else {
                    typen.join(", ")
                },
            ),
            (
                "Version",
                if autor.is_empty() {
                    d.version.to_string()
                } else {
                    format!("{} · {autor}", d.version)
                },
            ),
            (
                "Herkunft",
                if e.im_ordner {
                    "Ordner „Erweiterungen“".to_string()
                } else {
                    "nur im Projekt (aus der Datei)".to_string()
                },
            ),
            (
                "Im Projekt",
                if n == 0 {
                    "noch nicht gesetzt".to_string()
                } else {
                    format!("{n}×")
                },
            ),
            ("Schlüssel", d.key.clone()),
        ];
        let wert_x = x0 + 110.0 * s;
        for (k, v) in zeilen {
            label(c, regular, k, small, x0, yy, u.text_dim);
            let v = widgets::ellipsize(regular, &v, small, max - 110.0 * s);
            label(c, regular, &v, small, wert_x, yy, u.text);
            yy += 22.0 * s;
        }
        if let Some(r) = self.ext_button(t, w) {
            let st = self.btn_state(Btn::Einfuegen, true, false);
            widgets::button(c, fonts, r, "Einfügen", st, s, t);
            let hint = "Schließt den Katalog; Esc beendet das Setzen.";
            let hx = r.x + r.w + 14.0 * s;
            label(c, regular, hint, small, hx, r.y + 21.0 * s, u.text_dim);
        }
    }

    /// Ziel rechts im Reiter „Erweiterungen“.
    pub(super) fn ext_hit(&self, t: &Theme, w: &Win, x: f64, y: f64) -> Option<Target> {
        self.ext_button(t, w)
            .filter(|r| r.contains(x, y))
            .map(|_| Target::Btn(Btn::Einfuegen))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Tab};
    use super::*;
    use crate::scene::Scene;
    use sk_model::Model;
    use sk_platform::{Event, Modifiers, MouseButton};

    const WIN: Win = Win {
        w: 1280,
        h: 800,
        top: 32,
        scale: 1.0,
    };

    fn def(text: &str) -> ExtDef {
        ExtDef::einlesen(text).unwrap()
    }

    fn klick(c: &mut Catalog, s: &mut Scene, r: Rect) -> Out {
        let (theme, f) = (Theme::dark(), Fonts::system());
        let mut cx = Ctx {
            scene: s,
            theme: &theme,
            fonts: &f,
            win: WIN,
            company: None,
            company_standard: false,
        };
        let (x, y) = ((r.x + r.w * 0.5) as f64, (r.y + r.h * 0.5) as f64);
        let mods = Modifiers::default();
        let button = MouseButton::Left;
        c.handle(&Event::MouseDown { button, x, y, mods }, &mut cx);
        c.handle(&Event::MouseUp { button, x, y, mods }, &mut cx)
    }

    /// Reiter „Erweiterungen“: Gruppe › Name, Kachel wählt, „Einfügen“
    /// schließt mit dem Schlüssel; ein Bauteil nur aus dem Projekt steht
    /// mit in der Liste.
    #[test]
    fn einfuegen_aus_dem_katalog() {
        let (t, f) = (Theme::dark(), Fonts::system());
        let stuetze = def(include_str!(
            "../../../crates/sk-szb/beispiele/werk.stuetze.szb"
        ));
        let treppe = def(include_str!(
            "../../../crates/sk-szb/beispiele/werk.treppe.szb"
        ));
        let platte = def(include_str!(
            "../../../crates/sk-szb/beispiele/werk.bodenplatte.szb"
        ));
        let mut m = Model::with_seed(7);
        m.put_ext_def(platte).unwrap();
        let mut s = Scene::with_model(m);
        let mut c = Catalog::open(&s, None);
        c.set_ext(&[treppe, stuetze]);
        let namen: Vec<(&str, bool)> = c.ext.iter().map(|e| (e.def.name(), e.im_ordner)).collect();
        assert_eq!(
            namen,
            [
                ("Bodenplatte", false),
                ("Stahlbetonstütze", true),
                ("Gerade Treppe", true)
            ]
        );
        // Zum Reiter
        let tab = c.tab_rect(&t, &WIN, Tab::Ext);
        klick(&mut c, &mut s, tab);
        assert_eq!(c.tab, Tab::Ext);
        let (tiles, heads, _) = c.list_layout(&t, &WIN);
        let heads: Vec<&str> = heads.iter().map(|h| h.0.as_str()).collect();
        assert_eq!(heads, ["Tragwerk", "Treppen und Geländer"]);
        assert_eq!(tiles.len(), 3);
        klick(&mut c, &mut s, tiles[2].1);
        assert_eq!(c.ext_sel, Some(2));
        assert!(c.is_selected(Item::Ext(2)));
        let (img, ..) = c.paint(&t, &f, &WIN);
        assert!(img.width > 0);
        let b = c.ext_button(&t, &WIN).unwrap();
        let out = klick(&mut c, &mut s, b);
        assert!(out.closed);
        assert_eq!(out.ext_einfuegen.as_deref(), Some("werk.treppe"));
        // Ungespeicherte Typänderung: erst OK oder Abbrechen
        let mut c = Catalog::open(&s, None);
        c.set_ext(&[]);
        assert_eq!(c.ext.len(), 1);
        let id = c.work.duplicate_type(c.sel.unwrap()).unwrap();
        c.show(id);
        c.tab = Tab::Ext;
        let b = c.ext_button(&t, &WIN).unwrap();
        let out = klick(&mut c, &mut s, b);
        assert!(!out.closed && out.ext_einfuegen.is_none());
        assert!(c.message.is_some());
    }
}

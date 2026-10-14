//! „Als Tabelle speichern“ (.csv) des Mengenblatts, nach Geschoss und
//! nach Gewerk. Ausgelagert, damit `schedule_view.rs` unter der Grenze
//! bleibt (R4).

use super::*;

/// Zahl für die .csv: Dezimalkomma, 4 Nachkommastellen kaufmännisch,
/// ASCII-Minus.
pub(super) fn csv_num(v: f64) -> String {
    let s = format!("{:.4}", kaufmaennisch(v, 4));
    let s = if s == "-0.0000" { "0.0000".into() } else { s };
    s.replace('.', ",")
}

pub(super) fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// „Als Tabelle speichern“: UTF-8 mit BOM, Semikolon, Dezimalkomma, alle
/// Gruppen aufgeklappt, 4 Nachkommastellen (m, m², m³).
#[cfg(test)]
pub fn csv(m: &Model, sched: &Schedule) -> Vec<u8> {
    csv_grouped(m, sched, Grouping::Storey)
}

/// Wie [`csv_grouped`] in der Sicht der Mengen (VOB oder Bestellung, wie
/// der Bildschirm), mit der Kopfzeile des Umfangs als erster Zeile
/// (paket-ka1.md §3), nach dem BOM. Vorn steht die Sicht, damit eine
/// Bestellliste nicht als Aufmaß durchgeht (Bedienbarkeit 38.3).
pub fn csv_mit_kopf(
    m: &Model,
    sched: &Schedule,
    (by, modus): (Grouping, Modus),
    kopf: &str,
) -> Vec<u8> {
    let mut b = csv_sicht(m, sched, by, modus);
    debug_assert!(b.starts_with(&[0xEF, 0xBB, 0xBF]), "CSV mit BOM");
    let zeile = format!(
        "{}\r\n",
        csv_field(&format!("{} · {kopf}", sicht_name(modus)))
    );
    b.splice(3..3, zeile.into_bytes());
    b
}

/// Name der Sicht in der Kopfzeile der Tabelle.
pub fn sicht_name(modus: Modus) -> &'static str {
    match modus {
        Modus::Vob => "VOB-Mengen",
        Modus::Bestellung => "Bestellmengen mit Verschnitt",
    }
}

/// Wie [`csv`] in der Gliederung des Mengenfensters (§12 Prüfpunkt 5).
#[cfg(test)]
pub fn csv_grouped(m: &Model, sched: &Schedule, by: Grouping) -> Vec<u8> {
    csv_sicht(m, sched, by, Modus::Vob)
}

/// In Gliederung `by` und Sicht `modus`: Die Bestellung zeigt die Mengen
/// mit Zuschlag wie der Bildschirm.
pub fn csv_sicht(m: &Model, sched: &Schedule, by: Grouping, modus: Modus) -> Vec<u8> {
    match by {
        Grouping::Storey => csv_storeys(m, sched, modus),
        Grouping::Trade => csv_trades(m, sched, modus),
    }
}

/// Fläche der Summe eines Gewerks wie die Zeilen (Abrechnungsfläche in der
/// Sicht): die in m² abgerechneten Zeilen, Bauteile mit `once` je einmal.
fn summe_flaeche(rows: &[LayerRow], zeig: &dyn Fn(&LayerRow, Bezug) -> f64) -> f64 {
    let mut seen = Vec::new();
    let mut a = 0.0;
    for r in rows.iter().filter(|r| r.bill_area > 0.0) {
        if r.once {
            if seen.contains(&r.element) {
                continue;
            }
            seen.push(r.element);
        }
        a += zeig(r, Bezug::Flaeche);
    }
    a
}

/// Nach Gewerk (paket-1a §7): je Gewerk die Schichtgruppen mit
/// Zwischensumme, die Bauteile mit Geschoss und der Kostengruppe der
/// Schicht, die Summe des Gewerks; danach die Summen nach Baustoff.
fn csv_trades(m: &Model, sched: &Schedule, modus: Modus) -> Vec<u8> {
    let mut out = String::new();
    let mut row = |cols: [&str; 10]| {
        let v: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&v.join(";"));
        out.push_str("\r\n");
    };
    row([
        "Gebäude",
        "Gewerk",
        "Kostengruppe",
        "Bauteil",
        "Nr.",
        "Geschoss",
        "Länge (m)",
        "Fläche (m²)",
        "Volumen (m³)",
        "Hinweis",
    ]);
    let len = |mm: f64| csv_num(mm / 1e3);
    let area = |mm2: f64| csv_num(mm2 / 1e6);
    let vol = |mm3: f64| csv_num(mm3 / 1e9);
    let opt = |v: f64, f: &dyn Fn(f64) -> String| if v > 0.0 { f(v) } else { String::new() };
    // Mengen wie auf dem Bildschirm: Abrechnungsfläche, in der Bestellung
    // mit Zuschlag; Länge mit Zuschlag nur, wo nach Länge bestellt wird
    let zeig = |r: &LayerRow, b: Bezug| rechenweg::anzeige(m, r, b, modus);
    let flaeche = |r: &LayerRow| opt(zeig(r, Bezug::Flaeche), &area);
    let laenge = |r: &LayerRow| {
        opt(
            if haupt_bezug(m, r) == Some(Bezug::Laenge) {
                zeig(r, Bezug::Laenge)
            } else {
                r.length
            },
            &len,
        )
    };
    // Summe des Gewerks in der Sicht: Verhältnis Sicht ÷ VOB über die Zeilen
    let faktor = |rows: &[LayerRow], b: Bezug, f: &dyn Fn(&LayerRow) -> bool| {
        let (a, z) = rows.iter().filter(|r| f(r)).fold((0.0, 0.0), |(a, z), r| {
            (a + rechenweg::wert(r, b), z + zeig(r, b))
        });
        if a > 0.0 {
            z / a
        } else {
            1.0
        }
    };
    for b in &sched.buildings {
        let gb = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        // Gewerke der Schichten und der Erweiterungen (E8a) nach Bauablauf
        let ext_t = ext::gewerke(m, b);
        let mut ids: Vec<sk_model::trade::TradeId> = b.by_trade.iter().map(|t| t.trade).collect();
        for t in ext_t.iter().flatten() {
            if !ids.contains(t) {
                ids.push(*t);
            }
        }
        ids.sort_by_key(|t| m.trade(*t).map_or(u16::MAX, |x| x.order));
        for id in ids {
            let Some(tr) = m.trade(id) else { continue };
            let trade = format!("{} {}", tr.code, tr.name);
            let Some(t) = b.by_trade.iter().find(|t| t.trade == id) else {
                ext::csv_gewerk(m, b, &gb, &trade, Some(id), &mut row);
                continue;
            };
            let namen = if t.rows.iter().any(|r| r.category == Category::Space) {
                raum::namen(b)
            } else {
                raum::Namen::new()
            };
            let mut i = 0;
            while i < t.rows.len() {
                let r = &t.rows[i];
                let kg = |x: &LayerRow| x.kg.map_or(String::new(), |k| k.to_string());
                if !layer_grouped(r.category) {
                    row([
                        &gb,
                        &trade,
                        &kg(r),
                        &layer_row_label(m, r),
                        &r.number,
                        &storey_name(m, r.storey).0,
                        &laenge(r),
                        &flaeche(r),
                        &if r.category == Category::Coping {
                            String::new()
                        } else {
                            vol(zeig(r, Bezug::Volumen))
                        },
                        &weg_text(m, r, modus),
                    ]);
                    i += 1;
                    continue;
                }
                let n = t.rows[i..]
                    .iter()
                    .take_while(|x| {
                        x.category == r.category
                            && x.material == r.material
                            && (x.thickness - r.thickness).abs() < 1e-6
                    })
                    .count();
                let rows = per_element(&t.rows[i..i + n], &namen);
                i += n;
                let title = layer_group_title(m, r, &rows);
                let a: f64 = rows.iter().map(|x| zeig(x, Bezug::Flaeche)).sum();
                let range = match rows.as_slice() {
                    [] => String::new(),
                    [one] => one.number.clone(),
                    [first, .., last] => {
                        let tail = last.number.rsplit('-').next().unwrap_or(&last.number);
                        format!("{} … {tail}", first.number)
                    }
                };
                // Räume: Kostengruppe nur, wenn alle Zeilen sie teilen
                let kg_g = if rows.iter().all(|x| x.kg == r.kg) {
                    kg(r)
                } else {
                    String::new()
                };
                row([
                    &gb,
                    &trade,
                    &kg_g,
                    &format!("{title} (Summe, {} Stück)", rows.len()),
                    &range,
                    "",
                    &opt(rows.iter().map(|x| x.length).sum(), &len),
                    &opt(a, &area),
                    &if ohne_volumen(r) {
                        String::new()
                    } else {
                        vol(rows.iter().map(|x| zeig(x, Bezug::Volumen)).sum())
                    },
                    "",
                ]);
                // Je Bauteil seine erste Schicht der Gruppe und ihre Zahl,
                // einmal je Gruppe (Review #170 B1)
                let mut je: HashMap<ElementId, (usize, usize)> = HashMap::new();
                for (k, y) in t.rows[i - n..i].iter().enumerate() {
                    je.entry(y.element).or_insert((i - n + k, 0)).1 += 1;
                }
                for x in &rows {
                    // Rechenweg, wenn das Bauteil hier nur eine Schicht hat;
                    // beim Raum die Flächen zusammen, je mit Namen
                    let weg = match je.get(&x.element) {
                        _ if x.category == Category::Space => weg_text(m, x, modus),
                        Some(&(k, 1)) => weg_text(m, &t.rows[k], modus),
                        _ => String::new(),
                    };
                    let bauteil = if x.category == Category::Space {
                        format!("{title} · {}", raum::flaechen(x))
                    } else {
                        title.clone()
                    };
                    row([
                        &gb,
                        &trade,
                        &kg(x),
                        &bauteil,
                        &x.number,
                        &storey_name(m, x.storey).0,
                        &opt(x.length, &len),
                        &flaeche(x),
                        &if ohne_volumen(x) {
                            String::new()
                        } else {
                            vol(zeig(x, Bezug::Volumen))
                        },
                        &weg,
                    ]);
                }
            }
            row([
                &gb,
                &trade,
                "",
                &format!("Summe {}", tr.name),
                "",
                "",
                &t.length.map_or(String::new(), |l| {
                    len(l * faktor(&t.rows, Bezug::Laenge, &|r| r.bill_length > 0.0))
                }),
                &t.area
                    .map_or(String::new(), |_| area(summe_flaeche(&t.rows, &zeig))),
                &if t.rows.iter().all(ohne_volumen) {
                    String::new()
                } else {
                    vol(t.volume
                        * faktor(&t.rows, Bezug::Volumen, &|r| {
                            !(r.once && r.bill_length > 0.0)
                        }))
                },
                // Die Fläche zählt nur die nach m² abgerechneten Schichten,
                // darüber stehen auch Mauerwerk und Putz (Review #77, B)
                if t.area.is_some() {
                    "Fläche: davon nach m² abgerechnet"
                } else {
                    ""
                },
            ]);
            ext::csv_gewerk(m, b, &gb, &trade, Some(id), &mut row);
        }
        if ext_t.contains(&None) {
            ext::csv_gewerk(m, b, &gb, "ohne Gewerk", None, &mut row);
        }
        for x in &b.by_material {
            let name = m
                .material(x.material)
                .map_or(String::new(), |y| y.name.clone());
            // in der Bestellsicht mit dem Zuschlag des Baustoffs, wie nach
            // Geschoss (Review #77, Nachtrag A)
            let zu = zu_faktor(m, x.material, modus);
            let (v, a) = x.in_sicht(modus);
            row([
                &gb,
                "Summe nach Baustoff",
                "",
                &name,
                "",
                "",
                &x.length.map_or(String::new(), |l| len(l * zu)),
                &a.map_or(String::new(), |a| area(a * zu)),
                &if x.length.is_some() {
                    String::new()
                } else {
                    vol(v * zu)
                },
                "",
            ]);
        }
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

/// Faktor der Menge eines Baustoffs in der Sicht `modus`: 1 in VOB, in
/// der Bestellung 1 + Verschnittzuschlag.
fn zu_faktor(m: &Model, mat: sk_model::MaterialId, modus: Modus) -> f64 {
    match modus {
        Modus::Vob => 1.0,
        Modus::Bestellung => 1.0 + m.material(mat).map_or(0.0, sk_model::matprop::waste) / 100.0,
    }
}

/// Nach Geschoss (B7).
fn csv_storeys(m: &Model, sched: &Schedule, modus: Modus) -> Vec<u8> {
    let mut out = String::new();
    let mut row = |cols: [&str; 10]| {
        let v: Vec<String> = cols.iter().map(|c| csv_field(c)).collect();
        out.push_str(&v.join(";"));
        out.push_str("\r\n");
    };
    row([
        "Gebäude",
        "Geschoss",
        "Kostengruppe",
        "Bauteil",
        "Nr.",
        "Länge (m)",
        "Stück",
        "Fläche (m²)",
        "Volumen (m³)",
        "Hinweis",
    ]);
    let len = |mm: f64| csv_num(mm / 1e3);
    let area = |mm2: f64| csv_num(mm2 / 1e6);
    let vol = |mm3: f64| csv_num(mm3 / 1e9);
    // Bestellung wie auf dem Bildschirm: je Bauteil das Verhältnis zur
    // VOB-Menge, je Baustoff der Zuschlag
    let faktor = bestell_faktoren(m, sched, modus);
    let fv = |e: ElementId| faktor.get(&e).map_or(1.0, |f| f.0);
    let fa = |e: ElementId| faktor.get(&e).map_or(1.0, |f| f.1);
    let zu = |mat| zu_faktor(m, mat, modus);
    let mut storeys: Vec<(String, &StoreyQto)> = Vec::new();
    for b in &sched.buildings {
        let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        for st in &b.storeys {
            storeys.push((nr.clone(), st));
        }
    }
    for st in &sched.loose {
        storeys.push((String::new(), st));
    }
    for (gb, st) in storeys {
        let (sname, short) = storey_name(m, st.id);
        for g in &st.groups {
            if g.ext.is_some() {
                ext::csv_geschoss(m, &gb, &sname, g, &mut row);
                continue;
            }
            if g.category == Category::Space {
                raum::csv_geschoss(m, (&gb, &sname), st, g, modus, &mut row);
                continue;
            }
            let kg = g.category.din276().map_or(String::new(), |k| k.to_string());
            let title = if is_wall(g.category) {
                group_title(m, g, false)
            } else {
                g.category.name().to_string()
            };
            // Zwischensumme der Wandgruppe (die übrigen sind einzelne Bauteile)
            let t = g.total;
            if is_wall(g.category) {
                row([
                    &gb,
                    &sname,
                    &kg,
                    &format!("{title} (Summe)"),
                    &number_range(m, g),
                    &if t.length > 0.0 {
                        len(t.length)
                    } else {
                        String::new()
                    },
                    &t.count.to_string(),
                    &if t.area > 0.0 {
                        area(t.area)
                    } else {
                        String::new()
                    },
                    // wie die Zeilen nach VOB/C (Bedienbarkeit 38.2)
                    &vol(g
                        .rows
                        .iter()
                        .filter_map(|r| Some(wand_vol(&faktor, r.element, r.q.as_ref()?.volume())))
                        .sum()),
                    "",
                ]);
            }
            for r in &g.rows {
                let vol = |x: f64| vol(x * fv(r.element));
                let area = |x: f64| area(x * fa(r.element));
                let (bauteil, l, a, v, note) = match &r.q {
                    Some(ElementQto::Wall(w)) => (
                        format!("{} {}, Höhe {} m", title, r.number, de(w.height / 1e3, 3)),
                        len(w.list_length),
                        String::new(),
                        // mit den übermessenen Öffnungen (38.2)
                        vol(w.volume + faktor.get(&r.element).map_or(0.0, |f| f.2)),
                        offset_note(m, r.element).unwrap_or_default(),
                    ),
                    Some(ElementQto::Footing(f)) => (
                        "Frostschürze".into(),
                        len(f.length),
                        String::new(),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Slab(p)) => (
                        format!("Sohlplatte {}", cm(p.thickness)),
                        String::new(),
                        area(p.area),
                        vol(p.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Floor(f)) => (
                        format!("Decke über {short} {}", cm(f.thickness)),
                        String::new(),
                        area(f.area),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Strip(f)) => (
                        format!("Randdämmstreifen {} × {}", cm(f.width), cm(f.height)),
                        len(f.length),
                        String::new(),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Soffit(f)) => (
                        format!("Untersichtdämmung {}", cm(f.thickness)),
                        String::new(),
                        area(f.area),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Perimeter(f)) => (
                        format!("Perimeterdämmung {}", cm(f.thickness)),
                        String::new(),
                        area(f.area),
                        vol(f.volume),
                        String::new(),
                    ),
                    Some(ElementQto::Terrace(t)) => (
                        "Dachterrasse".into(),
                        String::new(),
                        area(t.area),
                        vol(t.volume),
                        format!(
                            "Dämmung {} m³, Belag {} m³",
                            de(t.insulation_volume / 1e9, 3),
                            de(t.finish_volume / 1e9, 3)
                        ),
                    ),
                    Some(ElementQto::Coping(c)) => (
                        "Attikablech".into(),
                        len(c.length),
                        String::new(),
                        String::new(),
                        format!("Abwicklung {} mm", c.girth.round()),
                    ),
                    // Erweiterungen und Räume stehen in ihrer Gruppe (oben)
                    Some(ElementQto::Ext(_)) | Some(ElementQto::Room(_)) | None => (
                        g.category.name().to_string(),
                        String::new(),
                        String::new(),
                        "–".into(),
                        r.note.clone().unwrap_or_default(),
                    ),
                };
                row([
                    &gb, &sname, &kg, &bauteil, &r.number, &l, "1", &a, &v, &note,
                ]);
                if let Some(ElementQto::Wall(w)) = &r.q {
                    if w.facing_support > 0.0 {
                        row([
                            &gb,
                            &sname,
                            &kg,
                            "Abfangung Verblender",
                            &r.number,
                            &len(w.facing_support),
                            "",
                            "",
                            "",
                            "",
                        ]);
                    }
                    if let Some(f) = &w.facing_foot {
                        row([
                            &gb,
                            &sname,
                            &kg,
                            "Fußpunkt Schaumglas",
                            &r.number,
                            &len(f.length),
                            "",
                            "",
                            &vol(f.volume),
                            &format!("{} Lagen, Höhe {} mm", f.courses, f.height.round()),
                        ]);
                    }
                }
                if let Some(ElementQto::Soffit(f)) = &r.q {
                    for (name, l, a, v) in soffit_lines(f) {
                        let pos = |x: f64, s: &dyn Fn(f64) -> String| {
                            if x > 0.0 {
                                s(x)
                            } else {
                                String::new()
                            }
                        };
                        row([
                            &gb,
                            &sname,
                            &kg,
                            &name,
                            &r.number,
                            &pos(l, &len),
                            "",
                            &pos(a, &area),
                            &pos(v, &vol),
                            "",
                        ]);
                    }
                }
                if let Some(ElementQto::Floor(f)) = &r.q {
                    if f.bearing > 0.0 {
                        row([
                            &gb,
                            &sname,
                            &kg,
                            "davon Auflager in den Außenwänden",
                            &r.number,
                            "",
                            "",
                            "",
                            &vol(f.bearing),
                            "in der Decke enthalten",
                        ]);
                    }
                }
            }
            if is_wall(g.category) {
                // Schichten je Baustoff
                let mut mats: Vec<(sk_model::MaterialId, f64, f64)> = Vec::new();
                for r in &g.rows {
                    if let Some(ElementQto::Wall(w)) = &r.q {
                        for l in &w.layers {
                            match mats.iter_mut().find(|x| x.0 == l.material) {
                                Some(x) => {
                                    x.1 += l.volume;
                                    x.2 += l.side_area;
                                }
                                None => mats.push((l.material, l.volume, l.side_area)),
                            }
                        }
                    }
                }
                for (mat, v, a) in mats {
                    let Some(x) = m.material(mat) else { continue };
                    if x.category == sk_model::MatCategory::Air {
                        continue;
                    }
                    let ins = x.category == sk_model::MatCategory::Insulation;
                    row([
                        &gb,
                        &sname,
                        &kg,
                        &format!("{title}: {}", x.name),
                        "",
                        "",
                        "",
                        &if ins {
                            area(a * zu(mat))
                        } else {
                            String::new()
                        },
                        &vol(v * zu(mat)),
                        "",
                    ]);
                }
                if t.pocket > 0.0 {
                    let what = match g.category {
                        Category::ExteriorWall => "Abzug Deckenauflager",
                        _ => "Abzug Deckenstreifen",
                    };
                    row([
                        &gb,
                        &sname,
                        &kg,
                        &format!("{title}: {what}"),
                        "",
                        "",
                        "",
                        "",
                        &vol(-t.pocket),
                        "in der Decke enthalten",
                    ]);
                }
            }
        }
    }
    for b in &sched.buildings {
        let nr = m.building(b.id).map_or(String::new(), |x| x.number.clone());
        for x in &b.by_material {
            let name = m
                .material(x.material)
                .map_or(String::new(), |y| y.name.clone());
            let (v, a) = x.in_sicht(modus);
            row([
                &nr,
                "Summe nach Baustoff",
                "",
                &name,
                "",
                &x.length.map_or(String::new(), |l| len(l * zu(x.material))),
                "",
                &a.map_or(String::new(), |a| area(a * zu(x.material))),
                &if x.length.is_some() {
                    String::new()
                } else {
                    vol(v * zu(x.material))
                },
                "",
            ]);
        }
    }
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

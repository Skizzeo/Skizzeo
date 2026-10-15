//! Geländeflächen in der `.szo` (Gelände G3b, plan-gelaendemodul.md §6):
//!
//! ```text
//! [tpatch] pts=0,0;10000,0;10000,8000;0,8000 z=-150
//! [tpatch] pts=… z=-150 fall=2 dir=90 n=1.5,0,1.5,2 paved=1
//! [tpatch] pts=… ecken=0:24.35;2:24.1 href=abs
//! [sitesoil] key=project spread=250 reuse=0.9
//! ```
//!
//! Umriss in mm im Modell, Höhe in mm über ±0,00 oder mit `href=abs` in m
//! ü. NHN, so wie eingegeben. `fall` in Prozent in Richtung `dir` (Grad),
//! `ecken` bis drei Höhen an Ecken (Nummer ab 0), `n` die Böschung 1:n
//! ringsum oder je Kante (0 senkrecht), ohne `n` die Vorgabe.
//! `[sitesoil]` nur mit Werten, die von der Vorgabe abweichen. Ohne Flächen
//! keine Zeile; Speichern, Öffnen, Speichern bleibt bytegleich. Eine Zeile,
//! die nicht gilt, gibt einen Hinweis und bleibt roh stehen (Regel 72,
//! 114); nur ein doppeltes `[sitesoil]` hält das Öffnen auf.

use super::{err, Line, LoadError, Record};
use crate::model::{PatchHeight, Project, SiteSoil, TerrainPatch};
use crate::Model;

/// Abschnitte der Geländeflächen.
pub(super) const SECTIONS: [&str; 2] = ["tpatch", "sitesoil"];

/// Der Abschnitt `s`, wenn er zu den Geländeflächen gehört.
pub(super) fn known(s: &str) -> Option<&'static str> {
    SECTIONS.iter().find(|k| **k == s).copied()
}

/// Zahlen mit `,` und Paare mit `;`.
fn paare(v: &[[f64; 2]]) -> String {
    let t: Vec<String> = v.iter().map(|p| format!("{},{}", p[0], p[1])).collect();
    t.join(";")
}

pub(super) fn write(m: &Model, out: &mut String) {
    let p = m.project();
    let mut roh = p.site_patches_raw.iter().peekable();
    for (i, f) in p.site_patches.iter().enumerate() {
        while let Some((_, r)) = roh.next_if(|(k, _)| *k <= i) {
            out.push_str(r);
            out.push('\n');
        }
        let mut l = Line::new("tpatch").word("pts", &paare(&f.outline));
        match &f.height {
            PatchHeight::Flat(z) => l = l.num("z", z),
            PatchHeight::Fall { z, dir, fall } => {
                l = l.num("z", z).num("fall", fall).num("dir", dir);
            }
            PatchHeight::Corners(e) => {
                let t: Vec<String> = e.iter().map(|(i, z)| format!("{i}:{z}")).collect();
                l = l.word("ecken", &t.join(";"));
            }
        }
        if f.abs {
            l = l.word("href", "abs");
        }
        if !f.slopes.is_empty() {
            let t: Vec<String> = f.slopes.iter().map(|n| n.to_string()).collect();
            l = l.word("n", &t.join(","));
        }
        if f.paved {
            l = l.flag("paved", true);
        }
        l.finish(out);
    }
    for (_, r) in roh {
        out.push_str(r);
        out.push('\n');
    }
    let werte = p.site_soil.werte();
    let vorgabe = SiteSoil::default().werte();
    if werte == vorgabe {
        if let Some(r) = &p.site_soil_raw {
            out.push_str(r);
            out.push('\n');
        }
        return;
    }
    let mut l = Line::new("sitesoil").word("key", "project");
    for ((k, ..), (v, d)) in SiteSoil::FELDER.iter().zip(werte.into_iter().zip(vorgabe)) {
        if v != d {
            l = l.num(k, v);
        }
    }
    l.finish(out);
}

/// Zahl aus Text; endlich.
fn zahl(t: &str) -> Option<f64> {
    t.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// Eine `[tpatch]`-Zeile; `Err` mit dem Grund, warum sie nicht gilt.
fn patch(r: &Record) -> Result<TerrainPatch, String> {
    let roh = |k: &str| r.opt(k);
    let pts = roh("pts").ok_or("„pts“ fehlt")?;
    let outline = pts
        .split(';')
        .map(|p| {
            let (x, y) = p.split_once(',')?;
            Some([zahl(x)?, zahl(y)?])
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| format!("„pts={pts}“ unlesbar"))?;
    let z = roh("z").map(|v| zahl(v).ok_or_else(|| format!("„z={v}“ unlesbar")));
    let fall = roh("fall").map(|v| zahl(v).ok_or_else(|| format!("„fall={v}“ unlesbar")));
    let dir = roh("dir").map(|v| zahl(v).ok_or_else(|| format!("„dir={v}“ unlesbar")));
    let ecken = roh("ecken").map(|v| {
        v.split(';')
            .map(|e| {
                let (i, z) = e.split_once(':')?;
                Some((i.trim().parse::<usize>().ok()?, zahl(z)?))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("„ecken={v}“ unlesbar"))
    });
    let height = match (z, fall, dir, ecken) {
        (Some(z), None, None, None) => PatchHeight::Flat(z?),
        (Some(z), Some(fall), dir, None) => PatchHeight::Fall {
            z: z?,
            fall: fall?,
            dir: dir.transpose()?.unwrap_or(0.0),
        },
        (None, None, None, Some(e)) => PatchHeight::Corners(e?),
        _ => return Err("Höhe nicht eindeutig (z, fall/dir oder ecken)".into()),
    };
    let abs = match roh("href") {
        None | Some("rel") => false,
        Some("abs") => true,
        Some(v) => return Err(format!("„href={v}“ unbekannt")),
    };
    let slopes = match roh("n") {
        None => Vec::new(),
        Some(v) => v
            .split(',')
            .map(zahl)
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("„n={v}“ unlesbar"))?,
    };
    let paved = match roh("paved") {
        None | Some("0") => false,
        Some("1") => true,
        Some(v) => return Err(format!("„paved={v}“ nicht 0 oder 1")),
    };
    let f = TerrainPatch {
        outline,
        height,
        abs,
        slopes,
        paved,
    };
    if !f.valid() {
        return Err(
            "Fläche lässt sich nicht bauen (Umriss, Höhe oder Böschung fehlt oder liegt \
             außerhalb ihres Bereichs)"
                .into(),
        );
    }
    Ok(f)
}

/// Geländeflächen und Werte des Geländebaus ins Projekt.
pub(super) fn read_project<'r>(
    project: &mut Project,
    recs: &dyn Fn(&str) -> &'r [Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<(), LoadError> {
    let roh = |r: &Record| lines.get(r.line - 1).copied().unwrap_or("").to_string();
    for r in recs("tpatch") {
        match patch(r) {
            Ok(f) => project.site_patches.push(f),
            Err(warum) => {
                r.skip();
                hints.push(format!(
                    "Zeile {}: [tpatch] {warum}, die Zeile bleibt unverändert stehen",
                    r.line
                ));
                project
                    .site_patches_raw
                    .push((project.site_patches.len(), roh(r)));
            }
        }
    }
    match recs("sitesoil") {
        [] => {}
        [r] => {
            // Kennung `key=project`; gelesen, damit sie nicht als fremd gilt
            let _ = r.opt("key");
            let mut falsch = Vec::new();
            let mut w = SiteSoil::default().werte();
            for (i, (k, name, min, max)) in SiteSoil::FELDER.into_iter().enumerate() {
                let einheit = if k == "spread" { " mm" } else { "" };
                if let Some(v) =
                    super::gelaende::bereich(r, k, name, (min, max), einheit, &mut falsch)
                {
                    w[i] = v;
                }
            }
            if !falsch.is_empty() {
                r.skip();
                hints.extend(
                    falsch.into_iter().map(|h| {
                        h.replace(", Vorgabe gilt", ", Vorgaben gelten, die Zeile bleibt")
                    }),
                );
                project.site_soil_raw = Some(roh(r));
            } else {
                project.site_soil = SiteSoil::aus_werten(w).unwrap_or_default();
                // nur Vorgaben: ohne Zeile fiele sie beim Speichern weg
                if project.site_soil == SiteSoil::default() {
                    project.site_soil_raw = Some(roh(r));
                }
            }
        }
        [_, r, ..] => return Err(err(r.line, "[sitesoil] doppelt")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{read, write};
    use crate::model::{PatchHeight, SiteSoil, TerrainPatch};
    use crate::{GuidGen, Model};

    fn zeilen(t: &str) -> Vec<&str> {
        t.lines()
            .filter(|l| {
                super::SECTIONS
                    .iter()
                    .any(|k| l.starts_with(&format!("[{k}]")))
            })
            .collect()
    }

    /// Ohne Flächen keine Zeile; mit Flächen kommt alles gleich zurück und
    /// die Datei bleibt bytegleich.
    #[test]
    fn flaechen_in_der_datei() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut m = Model::with_seed(5);
        assert!(zeilen(&write(&m)).is_empty());
        let q = vec![
            [0.0, 0.0],
            [10_000.0, 0.0],
            [10_000.0, 8_000.0],
            [0.0, 8_000.0],
        ];
        m.begin("Gelände");
        m.add_site_patch(TerrainPatch::flat(q.clone(), -150.0));
        m.add_site_patch(TerrainPatch {
            outline: q.clone(),
            height: PatchHeight::Fall {
                z: -150.5,
                dir: 90.0,
                fall: 2.0,
            },
            abs: false,
            slopes: vec![1.5, 0.0, 1.5, 2.0],
            paved: true,
        });
        m.add_site_patch(TerrainPatch {
            outline: q,
            height: PatchHeight::Corners(vec![(0, 24.35), (2, 24.1)]),
            abs: true,
            slopes: vec![2.0],
            paved: false,
        });
        m.set_site_soil(SiteSoil {
            spread: 250.0,
            reuse: 1.0,
        });
        m.commit();
        let text = write(&m);
        assert_eq!(
            zeilen(&text),
            [
                "[tpatch] pts=0,0;10000,0;10000,8000;0,8000 z=-150",
                "[tpatch] pts=0,0;10000,0;10000,8000;0,8000 z=-150.5 fall=2 dir=90 n=1.5,0,1.5,2 paved=1",
                "[tpatch] pts=0,0;10000,0;10000,8000;0,8000 ecken=0:24.35;2:24.1 href=abs n=2",
                "[sitesoil] key=project spread=250",
            ]
        );
        let back = load(&text);
        assert!(back.hints.is_empty(), "{:?}", back.hints);
        assert_eq!(back.model.site_patches(), m.site_patches());
        assert_eq!(back.model.site_soil(), m.site_soil());
        assert_eq!(write(&back.model), text);
    }

    /// Eine Zeile, die nicht gilt, gibt einen Hinweis und bleibt roh; die
    /// anderen gelten.
    #[test]
    fn falsche_zeilen_bleiben() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut leer = write(&Model::with_seed(5));
        leer.push_str(concat!(
            "[tpatch] pts=0,0;1000,0;1000,1000 z=10\n",
            "[tpatch] pts=0,0;1000,0 z=10\n",
            "[tpatch] pts=0,0;1000,0;1000,1000 z=10 ecken=0:1\n",
            "[tpatch] pts=0,0;1000,0;1000,1000 z=x\n",
            "[sitesoil] key=project spread=5000 reuse=0.9\n",
        ));
        let back = load(&leer);
        assert_eq!(back.model.site_patches().len(), 1);
        assert_eq!(back.model.project().site_patches_raw.len(), 3);
        assert_eq!(back.model.site_soil(), SiteSoil::default());
        assert_eq!(back.hints.len(), 4, "{:?}", back.hints);
        assert!(back.hints[0].contains("[tpatch]"), "{:?}", back.hints);
        assert!(
            back.hints[3].contains("Oberboden andecken 5000 mm"),
            "{:?}",
            back.hints
        );
        let text = write(&back.model);
        for l in leer
            .lines()
            .filter(|l| l.starts_with("[tpatch]") || l.starts_with("[sitesoil]"))
        {
            assert!(text.contains(l), "{l}");
        }
        // neue Werte ersetzen die Zeile
        let mut m = back.model;
        m.begin("Boden");
        assert!(m.set_site_soil(SiteSoil {
            spread: 300.0,
            reuse: 1.0
        }));
        m.commit();
        let text = write(&m);
        assert!(text.contains("\n[sitesoil] key=project spread=300\n"));
        assert!(!text.contains("spread=5000"));
    }
}

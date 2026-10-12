//! Höhenaufmaß in der `.szo` (Gelände G1, plan-gelaendemodul.md §6):
//!
//! ```text
//! [terrain] src="aufmass.csv" read=2026-10-12 crs=utm32 hsys=DHHN2016
//! [tpoint] nr=1 e=467108.214 n=5878741.637 h=24.337 code=GR
//! [tpoint] nr=245 e=… n=… h=… code=GP off=1
//! [tline] kind=boundary pts=1,2,3,4
//! [tcode] code=BO kind=break
//! ```
//!
//! Die Werte stehen so, wie sie gelesen wurden (m, Rechtswert ohne
//! Zonenkennzahl); Speichern, Öffnen, Speichern bleibt bytegleich. Ohne
//! Aufmaß keine Zeile. Eine Zeile, die nicht zählt (unlesbare Zahl, keine
//! Nummer), gibt einen Hinweis und bleibt roh stehen (Regel 72, 114); das
//! Öffnen hält sie nie auf.

use super::{err, Line, LoadError, Record};
use crate::model::{Crs, HeightRef, HeightSystem, LocalFrame, Show, Terrain, TerrainPoint, Trim};
use crate::Model;
use sk_gelaende::codes::Bedeutung;

/// Abschnitte des Aufmaßes.
pub(super) const SECTIONS: [&str; 4] = ["terrain", "tpoint", "tline", "tcode"];

/// Wort ohne Leerzeichen und Anführungszeichen, sonst Text in Anführungszeichen.
fn wort(l: Line, k: &str, v: &str) -> Line {
    let schlicht =
        !v.is_empty() && !v.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '\\' | '='));
    if schlicht {
        l.word(k, v)
    } else {
        l.text(k, v)
    }
}

pub(super) fn write(m: &Model, out: &mut String) {
    let Some(t) = m.terrain() else {
        return;
    };
    let mut l = Line::new("terrain").text("src", &t.src);
    if !t.read.is_empty() {
        l = wort(l, "read", &t.read);
    }
    l = l.word("crs", &t.crs.key()).word("hsys", t.hsys.key());
    if t.trim == Trim::Hull {
        l = l.word("trim", "hull");
    }
    if let Some(s) = t.show {
        l = l.word("show", s.key());
    }
    if t.href == HeightRef::Abs {
        l = l.word("href", "abs");
    }
    if let Some(f) = t.local {
        l = l
            .flag("local", true)
            .num("e0", f.e0)
            .num("n0", f.n0)
            .num("rot", f.rot);
    }
    l.finish(out);
    for p in &t.points {
        let mut l = wort(Line::new("tpoint"), "nr", &p.nr)
            .num("e", p.e)
            .num("n", p.n)
            .num("h", p.h);
        if !p.code.is_empty() {
            l = wort(l, "code", &p.code);
        }
        if p.off {
            l = l.flag("off", true);
        }
        l.finish(out);
    }
    for b in &t.boundaries {
        wort(
            Line::new("tline").word("kind", "boundary"),
            "pts",
            &b.join(","),
        )
        .finish(out);
    }
    for (c, b) in &t.codes {
        wort(Line::new("tcode"), "code", c)
            .word("kind", b.schluessel())
            .finish(out);
    }
    for r in &t.raw {
        out.push_str(r);
        out.push('\n');
    }
}

/// Aufmaß aus den Abschnitten; `None` ohne `[terrain]` und ohne Punkte.
pub(super) fn read<'r>(
    recs: &dyn Fn(&str) -> &'r [Record],
    lines: &[&str],
    hints: &mut Vec<String>,
) -> Result<Option<Terrain>, LoadError> {
    let [kopf, punkte, linien, codes] = SECTIONS.map(recs);
    if kopf.is_empty() && punkte.is_empty() && linien.is_empty() && codes.is_empty() {
        return Ok(None);
    }
    let mut roh: Vec<(usize, String)> = Vec::new();
    let mut weg = |r: &Record, warum: &str, hints: &mut Vec<String>| {
        r.skip();
        hints.push(format!(
            "Zeile {}: [{}] {warum}, die Zeile bleibt unverändert stehen",
            r.line, r.section
        ));
        roh.push((
            r.line,
            lines.get(r.line - 1).copied().unwrap_or("").to_string(),
        ));
    };
    let mut t = Terrain::new(Crs::Local);
    match kopf {
        [] => hints.push("Höhenaufmaß ohne [terrain]-Zeile: gelesen als örtliches System".into()),
        [r] => {
            t.src = r.opt("src").unwrap_or("").to_string();
            t.read = r.opt("read").unwrap_or("").to_string();
            let mut falsch = |k: &str, v: &str| {
                hints.push(format!(
                    "Zeile {}: [terrain] „{k}={v}“ unbekannt, die Vorgabe gilt",
                    r.line
                ))
            };
            if let Some(v) = r.opt("crs") {
                match Crs::from_key(v) {
                    Some(c) => t.crs = c,
                    None => falsch("crs", v),
                }
            }
            if let Some(v) = r.opt("hsys") {
                match HeightSystem::ALL.into_iter().find(|h| h.key() == v) {
                    Some(h) => t.hsys = h,
                    None => falsch("hsys", v),
                }
            }
            match r.opt("trim") {
                None | Some("auto") => {}
                Some("hull") => t.trim = Trim::Hull,
                Some(v) => falsch("trim", v),
            }
            if let Some(v) = r.opt("show") {
                match [Show::All, Show::Fade, Show::Parcel]
                    .into_iter()
                    .find(|s| s.key() == v)
                {
                    Some(s) => t.show = Some(s),
                    None => falsch("show", v),
                }
            }
            match r.opt("href") {
                None | Some("high") => {}
                Some("abs") => t.href = HeightRef::Abs,
                Some(v) => falsch("href", v),
            }
            if r.opt("local") == Some("1") {
                let z = |k: &str| {
                    r.opt(k)
                        .and_then(|v| v.parse::<f64>().ok())
                        .filter(|v| v.is_finite())
                };
                match (z("e0"), z("n0"), z("rot")) {
                    (Some(e0), Some(n0), Some(rot)) => t.local = Some(LocalFrame { e0, n0, rot }),
                    _ => hints.push(format!(
                        "Zeile {}: [terrain] Lage des örtlichen Systems unlesbar, das Gelände liegt unter dem Haus",
                        r.line
                    )),
                }
            }
        }
        [_, r, ..] => return Err(err(r.line, "[terrain] doppelt")),
    }
    for r in punkte {
        let z = |k: &str| {
            r.opt(k)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let (Some(nr), Some(e), Some(n), Some(h)) = (r.opt("nr"), z("e"), z("n"), z("h")) else {
            weg(r, "ohne Nummer oder mit unlesbarer Lage oder Höhe", hints);
            continue;
        };
        let off = match r.opt("off") {
            None | Some("0") => false,
            Some("1") => true,
            Some(_) => {
                hints.push(format!(
                    "Zeile {}: [tpoint] „off“ ist weder 0 noch 1, der Punkt zählt",
                    r.line
                ));
                false
            }
        };
        t.points.push(TerrainPoint {
            nr: nr.to_string(),
            e,
            n,
            h,
            code: r.opt("code").unwrap_or("").to_string(),
            off,
        });
    }
    for r in linien {
        match (r.opt("kind"), r.opt("pts")) {
            (Some("boundary"), Some(p)) if !p.is_empty() => {
                t.boundaries
                    .push(p.split(',').map(|s| s.to_string()).collect());
            }
            _ => weg(r, "unbekannte Linie", hints),
        }
    }
    for r in codes {
        match (
            r.opt("code"),
            r.opt("kind").and_then(Bedeutung::aus_schluessel),
        ) {
            (Some(c), Some(b)) => t.codes.push((c.to_string(), b)),
            _ => weg(r, "unbekannter Code", hints),
        }
    }
    roh.sort();
    t.raw = roh.into_iter().map(|r| r.1).collect();
    Ok(Some(t))
}

#[cfg(test)]
mod tests {
    use super::super::tests::house;
    use super::super::{read, write};
    use crate::model::{Crs, HeightRef, Terrain, TerrainPoint};
    use crate::GuidGen;
    use sk_gelaende::codes::Bedeutung;

    fn aufmass() -> Terrain {
        let mut t = Terrain::new(Crs::Utm(32));
        t.src = "höhen aufmaß.csv".into();
        t.read = "2026-10-12".into();
        t.href = HeightRef::Abs;
        for (nr, e, n, h, code) in [
            ("1", 467108.214, 5878741.637, 24.337, "GR"),
            ("2", 467132.035, 5878744.562, 24.624, "GR"),
            ("3", 467128.548, 5878777.885, 25.71, "GR"),
            ("A 7", 467120.0, 5878760.0, 25.0, ""),
        ] {
            t.points.push(TerrainPoint {
                nr: nr.into(),
                e,
                n,
                h,
                code: code.into(),
                off: nr == "3",
            });
        }
        t.boundaries.push(vec!["1".into(), "2".into(), "3".into()]);
        t.codes.push(("XY".into(), Bedeutung::NichtImGelaende));
        t
    }

    /// Ohne Aufmaß keine Zeile; mit Aufmaß kommt alles gleich zurück und
    /// die Datei bleibt bytegleich.
    #[test]
    fn aufmass_in_der_datei() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut m = house();
        assert!(!write(&m).contains("[terrain]"));
        m.begin("Aufmaß");
        assert!(m.set_terrain(Some(aufmass())));
        m.commit();
        let text = write(&m);
        let z: Vec<&str> = text
            .lines()
            .filter(|l| {
                super::SECTIONS
                    .iter()
                    .any(|k| l.starts_with(&format!("[{k}]")))
            })
            .collect();
        assert_eq!(
            z,
            [
                "[terrain] src=\"höhen aufmaß.csv\" read=2026-10-12 crs=utm32 hsys=DHHN2016 href=abs",
                "[tpoint] nr=1 e=467108.214 n=5878741.637 h=24.337 code=GR",
                "[tpoint] nr=2 e=467132.035 n=5878744.562 h=24.624 code=GR",
                "[tpoint] nr=3 e=467128.548 n=5878777.885 h=25.71 code=GR off=1",
                "[tpoint] nr=\"A 7\" e=467120 n=5878760 h=25",
                "[tline] kind=boundary pts=1,2,3",
                "[tcode] code=XY kind=off",
            ]
        );
        let l = load(&text);
        assert!(l.hints.is_empty(), "{:?}", l.hints);
        assert_eq!(l.model.terrain(), m.terrain());
        assert_eq!(write(&l.model), text, "bytegleich");
    }

    /// Unlesbares hält das Öffnen nicht auf: Hinweis, Zeile bleibt roh.
    #[test]
    fn unlesbare_zeilen_bleiben_roh() {
        let load = |t: &str| read(t, GuidGen::with_seed(99)).unwrap();
        let mut m = house();
        m.begin("Aufmaß");
        m.set_terrain(Some(aufmass()));
        m.commit();
        let text = write(&m)
            .replace("h=24.624", "h=abc")
            .replace("[tcode] code=XY kind=off", "[tcode] code=XY kind=quatsch")
            .replace("crs=utm32", "crs=utm99");
        let l = load(&text);
        assert_eq!(l.hints.len(), 3, "{:?}", l.hints);
        assert!(l.hints[0].contains("crs=utm99"));
        assert!(l.hints[1].contains("[tpoint]") && l.hints[1].contains("bleibt unverändert"));
        let t = l.model.terrain().unwrap();
        assert_eq!(t.points.len(), 3);
        assert_eq!(t.crs, Crs::Local);
        assert!(t.codes.is_empty());
        let wieder = write(&l.model);
        assert!(wieder.contains("[tpoint] nr=2 e=467132.035 n=5878744.562 h=abc code=GR"));
        assert!(wieder.contains("[tcode] code=XY kind=quatsch"));
        assert_eq!(write(&load(&wieder).model), wieder);
    }
}

//! Übersetzung des Modells nach IFC4 (ISO 16739-1:2018, ADD2 TC1), Ansicht
//! Reference View 1.2: Projekt › Grundstück › Gebäude › Geschosse, jedes
//! Bauteil mit seiner Kennung, Klasse aus `kinds.rs` bzw. der Erweiterung,
//! Form als Dreiecksnetz je Baustoff mit Farbe, Bauteiltyp mit Schichtaufbau,
//! Baustoffkennwerte, allgemeine Eigenschaften, Mengen aus der Mengenliste
//! ([`crate::mengen`]), Kostengruppe DIN 276 und Leistungsbereich VOB/C.
//!
//! Der Export liest nur. Zwei Exporte desselben Modells mit derselben Zeit
//! sind bytegleich; alle Kennungen, die das Modell nicht trägt (Grundstück,
//! Beziehungen, Eigenschaftssätze), werden aus Kennungen des Modells
//! abgeleitet und bleiben so über Exporte gleich.

use crate::geometrie::{self, Netz};
use crate::klassen::{self, Klasse, Prop, Zuordnung};
use crate::mengen;
use crate::step::{e, lens, r, refs, s, typed, Writer, V};
use sk_math::{vec3, Vec3};
use sk_model::qto::ElementQto;
use sk_model::{
    library, Category, ElementId, ElementKind, Guid, LayerFunction, LayerSetId, LevelKind,
    MatCategory, MaterialId, MaterialLayer, Model, StoreyId,
};
use std::collections::{BTreeMap, HashMap};

/// Angaben zum Export, die nicht im Modell stehen.
#[derive(Clone, Debug)]
pub struct Optionen {
    /// Zeitpunkt in Sekunden seit 1970 (UTC), für Kopf und Änderungsstand.
    pub zeit: i64,
    /// Dateiname für den Kopf, z. B. „Haus Meyer.ifc“.
    pub datei: String,
    /// Programmstand, z. B. „0.2.0“.
    pub version: String,
}

impl Default for Optionen {
    fn default() -> Optionen {
        Optionen {
            zeit: 0,
            datei: "skizzeo.ifc".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// Was geschrieben wurde (Statuszeile, Tests).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bericht {
    pub gebaeude: usize,
    pub geschosse: usize,
    pub bauteile: usize,
    /// Bauteile, die nicht geschrieben wurden, mit Nummer und Grund.
    pub ausgelassen: Vec<(String, Grund)>,
}

/// Warum ein Bauteil nicht in der Datei steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grund {
    /// Sein Geschoss gehört zu keinem Gebäude.
    OhneGeschoss,
    /// Kein Körper oder kein Dreieck.
    OhneForm,
    /// Öffnungen brauchen das Bauteil, das sie lochen (`IfcRelVoidsElement`,
    /// ifc/plan.md §5 I5); bis dahin nicht in der Datei.
    OeffnungBisI5,
}

impl Grund {
    pub fn text(self) -> &'static str {
        match self {
            Grund::OhneGeschoss => "ohne Geschoss",
            Grund::OhneForm => "ohne Form",
            Grund::OeffnungBisI5 => "Öffnung bis I5",
        }
    }
}

/// Kennung, abgeleitet aus einer Kennung des Modells und einem Zweck.
fn abgeleitet(basis: Guid, zweck: &str) -> Guid {
    let h =
        sk_model::sha256::sha256(format!("skizzeo.ifc\n{}\n{zweck}", basis.to_ifc()).as_bytes());
    let mut b = [0u8; 16];
    b.copy_from_slice(&h[..16]);
    Guid(u128::from_be_bytes(b))
}

fn gid(g: Guid) -> V {
    s(&g.to_ifc())
}

/// Text oder `$`, wenn leer.
fn opt(t: &str) -> V {
    if t.trim().is_empty() {
        V::Null
    } else {
        s(t.trim())
    }
}

/// Datum und Uhrzeit (UTC) nach ISO 8601 aus Sekunden seit 1970.
pub fn iso_zeit(t: i64) -> String {
    let (tage, sek) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    // Tage seit 1970 → Kalenderdatum (Howard Hinnant, civil_from_days)
    let z = tage + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        sek / 3600,
        sek % 3600 / 60,
        sek % 60
    )
}

/// Winkel in Grad als `IfcCompoundPlaneAngleMeasure` (Grad, Minuten,
/// Sekunden, Millionstel Sekunden, alle mit demselben Vorzeichen).
fn grad_min_sek(w: f64) -> V {
    let sign = if w < 0.0 { -1 } else { 1 };
    let mut mikro = (w.abs() * 3600.0 * 1e6).round() as i64;
    let g = mikro / 3_600_000_000;
    mikro -= g * 3_600_000_000;
    let m = mikro / 60_000_000;
    mikro -= m * 60_000_000;
    let sec = mikro / 1_000_000;
    mikro -= sec * 1_000_000;
    V::List(
        [g, m, sec, mikro]
            .iter()
            .map(|v| V::Int(sign * v))
            .collect(),
    )
}

/// Farbanteil 0..1 auf vier Stellen.
fn anteil(c: u8) -> V {
    V::Real((c as f64 / 255.0 * 1e4).round() / 1e4)
}

struct Ctx<'a> {
    m: &'a Model,
    w: Writer,
    oh: u32,
    /// Lage im Ursprung ohne Drehung, von allen gleichen Lagen geteilt.
    origin: u32,
    body: u32,
    axis: u32,
    /// Darstellungsschlüssel → IfcSurfaceStyle.
    styles: HashMap<u16, u32>,
    /// Baustoff → IfcMaterial.
    stoffe: HashMap<MaterialId, u32>,
    /// Aufbau (Signatur) → IfcMaterialLayerSet.
    aufbauten: HashMap<String, u32>,
    /// (Bauteiltyp, Typklasse) → Typobjekt.
    typen: BTreeMap<(u32, &'static str), u32>,
    /// Typobjekt → Bauteile.
    typ_nutzer: BTreeMap<u32, Vec<u32>>,
    /// Baustoffangabe → Bauteile, mit Kennung der Beziehung.
    stoff_nutzer: BTreeMap<u32, (Guid, Vec<u32>)>,
    /// Kostengruppe → Bauteile.
    kg_nutzer: BTreeMap<u16, Vec<u32>>,
    /// Leistungsbereich der VOB/C (ATV-Nummer, Name) → Bauteile.
    atv_nutzer: BTreeMap<(String, String), Vec<u32>>,
    /// Geschoss → Bauteile.
    inhalt: BTreeMap<u32, Vec<u32>>,
    /// Räume je Geschoss: hängen über `IfcRelAggregates` am Geschoss, nicht
    /// in `IfcRelContainedInSpatialStructure` (IFC4 WR31).
    raeume: BTreeMap<u32, Vec<u32>>,
    projekt: Guid,
    /// Zeilen der Mengenliste je Bauteil (I4).
    mengen: HashMap<ElementId, ElementQto>,
}

/// Exportiert das Modell als IFC4-Datei (Text) und sagt, was darin steht.
pub fn export(m: &Model, opt: &Optionen) -> (String, Bericht) {
    let p = m.project();
    let mut c = Ctx {
        m,
        w: Writer::new(),
        oh: 0,
        origin: 0,
        body: 0,
        axis: 0,
        styles: HashMap::new(),
        stoffe: HashMap::new(),
        aufbauten: HashMap::new(),
        typen: BTreeMap::new(),
        typ_nutzer: BTreeMap::new(),
        stoff_nutzer: BTreeMap::new(),
        kg_nutzer: BTreeMap::new(),
        atv_nutzer: BTreeMap::new(),
        inhalt: BTreeMap::new(),
        raeume: BTreeMap::new(),
        projekt: p.guid,
        mengen: mengen_je_bauteil(m),
    };
    let mut bericht = Bericht::default();
    c.oh = c.owner_history(opt);
    let units = c.units();
    let (ctx, body, axis) = c.contexts();
    c.body = body;
    c.axis = axis;
    let name = if p.name.trim().is_empty() {
        "Projekt"
    } else {
        p.name.trim()
    };
    let project = c.w.add(
        "IFCPROJECT",
        vec![
            gid(p.guid),
            r(c.oh),
            s(name),
            opt_text(&p.kind),
            V::Null,
            opt_text(&p.number),
            V::Null,
            refs(&[ctx]),
            r(units),
        ],
    );

    // Grundstück, Gebäude, Geschosse
    let origin = c.placement3(vec3(0.0, 0.0, 0.0));
    let site_pl = c.w.add("IFCLOCALPLACEMENT", vec![V::Null, r(origin)]);
    let loc = m.location().normalized();
    let site_guid = abgeleitet(p.guid, "site");
    let site = c.w.add(
        "IFCSITE",
        vec![
            gid(site_guid),
            r(c.oh),
            s(if p.site.trim().is_empty() {
                "Grundstück"
            } else {
                p.site.trim()
            }),
            V::Null,
            V::Null,
            r(site_pl),
            V::Null,
            opt_text(&p.place),
            e("ELEMENT"),
            loc.lat.map_or(V::Null, grad_min_sek),
            loc.lon.map_or(V::Null, grad_min_sek),
            V::Null,
            V::Null,
            V::Null,
        ],
    );
    c.aggregate(abgeleitet(p.guid, "aggregates.site"), project, &[site]);

    let solids: HashMap<ElementId, sk_model::Solid> = m.element_solids().into_iter().collect();
    let mut buildings: Vec<_> = m.buildings().iter().collect();
    buildings.sort_by(|a, b| a.1.number.cmp(&b.1.number));
    let mut b_ids = Vec::new();
    let mut storey_of: HashMap<StoreyId, (u32, u32, f64)> = HashMap::new();
    for (bid, b) in &buildings {
        let pl = c.placement3(vec3(0.0, 0.0, 0.0));
        let b_pl = c.w.add("IFCLOCALPLACEMENT", vec![r(site_pl), r(pl)]);
        let bi = c.w.add(
            "IFCBUILDING",
            vec![
                gid(b.guid),
                r(c.oh),
                s(&b.number),
                V::Null,
                V::Null,
                r(b_pl),
                V::Null,
                s(&b.name),
                e("ELEMENT"),
                V::Null,
                V::Null,
                V::Null,
            ],
        );
        b_ids.push(bi);
        bericht.gebaeude += 1;
        let mut storeys: Vec<_> = m
            .storeys()
            .iter()
            .filter(|(_, st)| st.building == Some(*bid))
            .collect();
        storeys.sort_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation));
        let mut s_ids = Vec::new();
        for (sid, st) in storeys {
            let pl = c.placement3(vec3(0.0, 0.0, st.elevation));
            let s_pl = c.w.add("IFCLOCALPLACEMENT", vec![r(b_pl), r(pl)]);
            let si = c.w.add(
                "IFCBUILDINGSTOREY",
                vec![
                    gid(st.guid),
                    r(c.oh),
                    s(&st.short),
                    opt_text(match st.kind {
                        LevelKind::Foundation => "Gründung",
                        LevelKind::Storey => "",
                        LevelKind::Roof => "Dach",
                    }),
                    V::Null,
                    r(s_pl),
                    V::Null,
                    s(&st.name),
                    e("ELEMENT"),
                    V::Len(st.elevation),
                ],
            );
            storey_of.insert(sid, (si, s_pl, st.elevation));
            s_ids.push(si);
            bericht.geschosse += 1;
        }
        c.aggregate(abgeleitet(b.guid, "aggregates.storeys"), bi, &s_ids);
    }
    c.aggregate(abgeleitet(p.guid, "aggregates.buildings"), site, &b_ids);

    // Bauteile in der Reihenfolge des Modells
    let mut ids: Vec<ElementId> = m.elements().ids().collect();
    ids.sort_by_key(|id| id.index());
    for id in ids {
        let Some(el) = m.element(id) else {
            continue;
        };
        // Öffnungen zuerst: sie haben (seit O2) keinen eigenen Körper und
        // fehlen bis I5 aus diesem Grund, nicht „ohne Form“
        let klasse = c.zuordnung(id).klasse.name;
        if klasse == "IfcOpeningElement" {
            bericht
                .ausgelassen
                .push((el.number.clone(), Grund::OeffnungBisI5));
            continue;
        }
        let Some(&(si, s_pl, elev)) = storey_of.get(&el.storey) else {
            bericht
                .ausgelassen
                .push((el.number.clone(), Grund::OhneGeschoss));
            continue;
        };
        let Some(solid) = solids.get(&id) else {
            bericht
                .ausgelassen
                .push((el.number.clone(), Grund::OhneForm));
            continue;
        };
        let netze = geometrie::netze(solid, vec3(0.0, 0.0, elev));
        if netze.is_empty() {
            bericht
                .ausgelassen
                .push((el.number.clone(), Grund::OhneForm));
            continue;
        }
        let pi = c.element(id, s_pl, elev, &netze);
        if klasse == "IfcSpace" {
            c.raeume.entry(si).or_default().push(pi);
        } else {
            c.inhalt.entry(si).or_default().push(pi);
        }
        bericht.bauteile += 1;
    }

    // Beziehungen gesammelt
    let geschosse: Vec<(StoreyId, (u32, u32, f64))> =
        storey_of.iter().map(|(k, v)| (*k, *v)).collect();
    let mut geschosse = geschosse;
    geschosse.sort_by_key(|(_, v)| v.0);
    for (sid, (si, _, _)) in geschosse {
        let g = m.storey(sid).map_or(p.guid, |s| s.guid);
        if let Some(raeume) = c.raeume.get(&si).cloned() {
            c.aggregate(abgeleitet(g, "aggregates.spaces"), si, &raeume);
        }
        if let Some(els) = c.inhalt.get(&si).cloned() {
            c.w.add(
                "IFCRELCONTAINEDINSPATIALSTRUCTURE",
                vec![
                    gid(abgeleitet(g, "contains")),
                    r(c.oh),
                    V::Null,
                    V::Null,
                    refs(&els),
                    r(si),
                ],
            );
        }
    }
    let typen: Vec<(u32, Vec<u32>)> = std::mem::take(&mut c.typ_nutzer).into_iter().collect();
    for (t, els) in typen {
        let g = abgeleitet(c.projekt, &format!("type.{t}"));
        let g = c.typ_guid(t).map_or(g, |tg| abgeleitet(tg, "defines"));
        c.w.add(
            "IFCRELDEFINESBYTYPE",
            vec![gid(g), r(c.oh), V::Null, V::Null, refs(&els), r(t)],
        );
    }
    let stoffe: Vec<(u32, (Guid, Vec<u32>))> =
        std::mem::take(&mut c.stoff_nutzer).into_iter().collect();
    for (mat, (g, els)) in stoffe {
        c.w.add(
            "IFCRELASSOCIATESMATERIAL",
            vec![gid(g), r(c.oh), V::Null, V::Null, refs(&els), r(mat)],
        );
    }
    let kg: Vec<(u16, Vec<u32>)> = std::mem::take(&mut c.kg_nutzer).into_iter().collect();
    if !kg.is_empty() {
        let cls = c.w.add(
            "IFCCLASSIFICATION",
            vec![
                s("DIN Deutsches Institut für Normung e. V."),
                s("2018-12"),
                V::Null,
                s("DIN 276"),
                s("Kosten im Bauwesen, Kostengruppen"),
                V::Null,
                V::Null,
            ],
        );
        for (k, els) in kg {
            let rf = c.w.add(
                "IFCCLASSIFICATIONREFERENCE",
                vec![
                    V::Null,
                    s(&k.to_string()),
                    sk_model::din276::name(k).map_or(V::Null, s),
                    r(cls),
                    V::Null,
                    V::Null,
                ],
            );
            c.w.add(
                "IFCRELASSOCIATESCLASSIFICATION",
                vec![
                    gid(abgeleitet(c.projekt, &format!("din276.{k}"))),
                    r(c.oh),
                    s(&format!("DIN 276 {k}")),
                    V::Null,
                    refs(&els),
                    r(rf),
                ],
            );
        }
    }

    let atv: Vec<((String, String), Vec<u32>)> =
        std::mem::take(&mut c.atv_nutzer).into_iter().collect();
    if !atv.is_empty() {
        let cls = c.w.add(
            "IFCCLASSIFICATION",
            vec![
                s("DIN Deutsches Institut für Normung e. V."),
                V::Null,
                V::Null,
                s("VOB/C"),
                s("Allgemeine Technische Vertragsbedingungen für Bauleistungen (ATV)"),
                V::Null,
                V::Null,
            ],
        );
        for ((code, name), els) in atv {
            let rf = c.w.add(
                "IFCCLASSIFICATIONREFERENCE",
                vec![
                    V::Null,
                    s(&format!("DIN {code}")),
                    opt_text(&name),
                    r(cls),
                    V::Null,
                    V::Null,
                ],
            );
            c.w.add(
                "IFCRELASSOCIATESCLASSIFICATION",
                vec![
                    gid(abgeleitet(c.projekt, &format!("atv.{code}"))),
                    r(c.oh),
                    s(&format!("VOB/C DIN {code}")),
                    V::Null,
                    refs(&els),
                    r(rf),
                ],
            );
        }
    }

    let header = vec![
        (
            "FILE_DESCRIPTION",
            vec![
                V::List(vec![s("ViewDefinition [ReferenceView_V1.2]")]),
                s("2;1"),
            ],
        ),
        (
            "FILE_NAME",
            vec![
                s(&opt.datei),
                s(&iso_zeit(opt.zeit)),
                V::List(vec![s(p.author.trim())]),
                V::List(vec![s("")]),
                s(&format!("Skizzeo {}", opt.version)),
                s(&format!("Skizzeo {}", opt.version)),
                s(""),
            ],
        ),
        ("FILE_SCHEMA", vec![V::List(vec![s("IFC4")])]),
    ];
    (c.w.finish(&header), bericht)
}

fn opt_text(t: &str) -> V {
    opt(t)
}

impl Ctx<'_> {
    fn owner_history(&mut self, opt: &Optionen) -> u32 {
        let p = self.m.project();
        let author = if p.author.trim().is_empty() {
            "Planverfasser"
        } else {
            p.author.trim()
        };
        let person = self.w.add(
            "IFCPERSON",
            vec![
                V::Null,
                s(author),
                V::Null,
                V::Null,
                V::Null,
                V::Null,
                V::Null,
                V::Null,
            ],
        );
        let buero = self.w.add(
            "IFCORGANIZATION",
            vec![V::Null, s(author), V::Null, V::Null, V::Null],
        );
        let po = self.w.add(
            "IFCPERSONANDORGANIZATION",
            vec![r(person), r(buero), V::Null],
        );
        let hersteller = self.w.add(
            "IFCORGANIZATION",
            vec![V::Null, s("Skizzeo"), V::Null, V::Null, V::Null],
        );
        let app = self.w.add(
            "IFCAPPLICATION",
            vec![r(hersteller), s(&opt.version), s("Skizzeo"), s("Skizzeo")],
        );
        self.w.add(
            "IFCOWNERHISTORY",
            vec![
                r(po),
                r(app),
                V::Null,
                e("NOCHANGE"),
                V::Null,
                V::Null,
                V::Null,
                V::Int(opt.zeit),
            ],
        )
    }

    /// Einheiten: Länge mm, Fläche m², Rauminhalt m³, Winkel rad, Masse kg,
    /// Zeit s, Temperatur K; abgeleitet U-Wert, λ und Rohdichte.
    fn units(&mut self) -> u32 {
        let si = |w: &mut Writer, typ: &str, pre: Option<&str>, name: &str| {
            w.add(
                "IFCSIUNIT",
                vec![V::Derived, e(typ), pre.map_or(V::Null, e), e(name)],
            )
        };
        let w = &mut self.w;
        let mm = si(w, "LENGTHUNIT", Some("MILLI"), "METRE");
        let m2 = si(w, "AREAUNIT", None, "SQUARE_METRE");
        let m3 = si(w, "VOLUMEUNIT", None, "CUBIC_METRE");
        let rad = si(w, "PLANEANGLEUNIT", None, "RADIAN");
        let kg = si(w, "MASSUNIT", Some("KILO"), "GRAM");
        let sec = si(w, "TIMEUNIT", None, "SECOND");
        let k = si(w, "THERMODYNAMICTEMPERATUREUNIT", None, "KELVIN");
        let m = si(w, "LENGTHUNIT", None, "METRE");
        let derived = |w: &mut Writer, typ: &str, el: &[(u32, i64)]| {
            let els: Vec<u32> = el
                .iter()
                .map(|&(u, x)| w.add("IFCDERIVEDUNITELEMENT", vec![r(u), V::Int(x)]))
                .collect();
            w.add("IFCDERIVEDUNIT", vec![refs(&els), e(typ), V::Null])
        };
        // W/(m²K) = kg·s⁻³·K⁻¹, W/(mK) = kg·m·s⁻³·K⁻¹, kg/m³
        let u = derived(
            w,
            "THERMALTRANSMITTANCEUNIT",
            &[(kg, 1), (sec, -3), (k, -1)],
        );
        let lambda = derived(
            w,
            "THERMALCONDUCTANCEUNIT",
            &[(kg, 1), (m, 1), (sec, -3), (k, -1)],
        );
        let rho = derived(w, "MASSDENSITYUNIT", &[(kg, 1), (m, -3)]);
        w.add(
            "IFCUNITASSIGNMENT",
            vec![refs(&[mm, m2, m3, rad, kg, sec, k, u, lambda, rho])],
        )
    }

    /// Darstellungszusammenhang Modell (3D, Nordrichtung) mit den
    /// Unterzusammenhängen Body und Axis.
    fn contexts(&mut self) -> (u32, u32, u32) {
        let origin = self.placement3(vec3(0.0, 0.0, 0.0));
        let n = self.m.location().normalized().north_deg().to_radians();
        let north = self.w.add(
            "IFCDIRECTION",
            vec![V::List(vec![
                V::Real(round9(n.sin())),
                V::Real(round9(n.cos())),
            ])],
        );
        let ctx = self.w.add(
            "IFCGEOMETRICREPRESENTATIONCONTEXT",
            vec![
                V::Null,
                s("Model"),
                V::Int(3),
                V::Real(0.01),
                r(origin),
                r(north),
            ],
        );
        let sub = |w: &mut Writer, id: &str, view: &str| {
            w.add(
                "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
                vec![
                    s(id),
                    s("Model"),
                    V::Derived,
                    V::Derived,
                    V::Derived,
                    V::Derived,
                    r(ctx),
                    V::Null,
                    e(view),
                    V::Null,
                ],
            )
        };
        let body = sub(&mut self.w, "Body", "MODEL_VIEW");
        let axis = sub(&mut self.w, "Axis", "GRAPH_VIEW");
        (ctx, body, axis)
    }

    fn placement3(&mut self, at: Vec3) -> u32 {
        if at == Vec3::default() && self.origin != 0 {
            return self.origin;
        }
        let p = self
            .w
            .add("IFCCARTESIANPOINT", vec![lens(&[at.x, at.y, at.z])]);
        let a = self
            .w
            .add("IFCAXIS2PLACEMENT3D", vec![r(p), V::Null, V::Null]);
        if at == Vec3::default() {
            self.origin = a;
        }
        a
    }

    fn aggregate(&mut self, g: Guid, whole: u32, parts: &[u32]) {
        if parts.is_empty() {
            return;
        }
        self.w.add(
            "IFCRELAGGREGATES",
            vec![gid(g), r(self.oh), V::Null, V::Null, r(whole), refs(parts)],
        );
    }

    /// Kennung des Typobjekts `t` (zurückgelesen aus der Zuordnung).
    fn typ_guid(&self, t: u32) -> Option<Guid> {
        let ((ls, klasse), _) = self.typen.iter().find(|(_, v)| **v == t)?;
        let set = self
            .m
            .layer_sets()
            .iter()
            .find(|(id, _)| id.index() == *ls)?
            .1;
        Some(abgeleitet(set.guid, klasse))
    }

    /// Oberflächenstil eines Baustoffs (Farbe der 3D-Oberfläche; bei einem
    /// Muster die Schnittfarbe, die das Muster im Mittel trägt).
    fn style(&mut self, mat: u16) -> u32 {
        if let Some(&s) = self.styles.get(&mat) {
            return s;
        }
        let m = self.m.material_by_key(mat);
        let sf = m.and_then(|m| self.m.attr().surface(m.surface));
        let rgb = sf.map_or([200, 200, 200], |f| {
            if f.pattern.is_some() {
                f.cut_color
            } else {
                f.color
            }
        });
        let name = m.map_or("Ohne Baustoff", |m| m.name.as_str()).to_string();
        let col = self.w.add(
            "IFCCOLOURRGB",
            vec![V::Null, anteil(rgb[0]), anteil(rgb[1]), anteil(rgb[2])],
        );
        let shading = self
            .w
            .add("IFCSURFACESTYLESHADING", vec![r(col), V::Real(0.0)]);
        let st = self.w.add(
            "IFCSURFACESTYLE",
            vec![s(&name), e("BOTH"), refs(&[shading])],
        );
        self.styles.insert(mat, st);
        st
    }

    /// Baustoff mit Kennwerten (Rohdichte, λ).
    fn stoff(&mut self, id: MaterialId) -> Option<u32> {
        if let Some(&x) = self.stoffe.get(&id) {
            return Some(x);
        }
        let mat = self.m.material(id)?;
        let mi = self.w.add(
            "IFCMATERIAL",
            vec![s(&mat.name), V::Null, s(mat.category.name())],
        );
        let rho = (mat.density > 0.0).then(|| {
            self.w.add(
                "IFCPROPERTYSINGLEVALUE",
                vec![
                    s("MassDensity"),
                    V::Null,
                    typed("IFCMASSDENSITYMEASURE", V::Real(mat.density)),
                    V::Null,
                ],
            )
        });
        if let Some(rho) = rho {
            self.w.add(
                "IFCMATERIALPROPERTIES",
                vec![s("Pset_MaterialCommon"), V::Null, refs(&[rho]), r(mi)],
            );
        }
        if let Some(l) = mat.lambda.filter(|l| *l > 0.0) {
            let p = self.w.add(
                "IFCPROPERTYSINGLEVALUE",
                vec![
                    s("ThermalConductivity"),
                    V::Null,
                    typed("IFCTHERMALCONDUCTIVITYMEASURE", V::Real(l)),
                    V::Null,
                ],
            );
            self.w.add(
                "IFCMATERIALPROPERTIES",
                vec![s("Pset_MaterialThermal"), V::Null, refs(&[p]), r(mi)],
            );
        }
        self.stoffe.insert(id, mi);
        Some(mi)
    }

    /// Schichtaufbau als `IfcMaterialLayerSet`, je gleicher Folge einmal.
    fn aufbau(&mut self, name: &str, code: &str, layers: &[MaterialLayer]) -> u32 {
        let sig = format!(
            "{name}|{code}|{}",
            layers
                .iter()
                .map(|l| format!("{}:{}:{:?}", l.material.index(), l.thickness, l.function))
                .collect::<Vec<_>>()
                .join(";")
        );
        if let Some(&x) = self.aufbauten.get(&sig) {
            return x;
        }
        let mut ls = Vec::new();
        for l in layers {
            let mat = self.m.material(l.material);
            let air = mat.is_some_and(|m| m.category == MatCategory::Air)
                || l.function == LayerFunction::AirGap;
            let mi = self.stoff(l.material);
            let kat = match l.function {
                LayerFunction::Structure if l.core => "LoadBearing",
                LayerFunction::Insulation => "Insulation",
                LayerFunction::Membrane => "Membrane",
                LayerFunction::AirGap => "AirGap",
                _ => "Finish",
            };
            ls.push(self.w.add(
                "IFCMATERIALLAYER",
                vec![
                    mi.map_or(V::Null, r),
                    V::Len(l.thickness),
                    if air { V::Unknown } else { V::Bool(false) },
                    mat.map_or(V::Null, |m| s(&m.name)),
                    V::Null,
                    s(kat),
                    V::Null,
                ],
            ));
        }
        let set = self
            .w
            .add("IFCMATERIALLAYERSET", vec![refs(&ls), opt(name), opt(code)]);
        self.aufbauten.insert(sig, set);
        set
    }

    /// IFC-Klasse und Art eines Bauteils aus der Bauteil-Liste bzw. dem
    /// `.szb` der Erweiterung.
    fn zuordnung(&self, id: ElementId) -> Zuordnung {
        let m = self.m;
        let el = m.element(id).expect("Bauteil aus der Liste des Modells");
        let ext = match &el.kind {
            ElementKind::Ext(_) => m.ext_def_of(id),
            _ => None,
        };
        klassen::zuordnen(ext.map_or(el.category.ifc_class(), |d| d.ifc()))
    }

    fn element(&mut self, id: ElementId, s_pl: u32, elev: f64, netze: &[Netz]) -> u32 {
        let m = self.m;
        let el = m.element(id).expect("Bauteil aus der Liste des Modells");
        let ext = match &el.kind {
            ElementKind::Ext(_) => m.ext_def_of(id),
            _ => None,
        };
        let z: Zuordnung = self.zuordnung(id);
        let k: &Klasse = z.klasse;
        let art = ext.map_or(el.category.name(), |d| d.name()).to_string();
        let layer_set: Option<(LayerSetId, &library::LayerSet)> =
            el.layer_set.and_then(|ls| m.layer_set(ls).map(|s| (ls, s)));
        let layers = m.element_layers(id);

        // Form
        let mut items = Vec::new();
        for n in netze {
            let coords: Vec<V> = n.punkte.iter().map(|p| lens(&[p.x, p.y, p.z])).collect();
            let pl = self.w.add("IFCCARTESIANPOINTLIST3D", vec![V::List(coords)]);
            let idx: Vec<V> = n
                .dreiecke
                .iter()
                .map(|t| V::List(t.iter().map(|&i| V::Int(i as i64 + 1)).collect()))
                .collect();
            let closed = if geometrie::geschlossen(n) {
                V::Bool(true)
            } else {
                V::Null
            };
            let fs = self.w.add(
                "IFCTRIANGULATEDFACESET",
                vec![r(pl), V::Null, closed, V::List(idx), V::Null],
            );
            let st = self.style(n.mat);
            self.w
                .add("IFCSTYLEDITEM", vec![r(fs), refs(&[st]), V::Null]);
            items.push(fs);
        }
        let mut reps = vec![self.w.add(
            "IFCSHAPEREPRESENTATION",
            vec![r(self.body), s("Body"), s("Tessellation"), refs(&items)],
        )];
        let achse = if k.name == "IfcWall" {
            self.wall_axis(id, elev, netze)
        } else {
            None
        };
        if let Some((a, b)) = achse {
            let pa = self.w.add("IFCCARTESIANPOINT", vec![lens(&[a.x, a.y])]);
            let pb = self.w.add("IFCCARTESIANPOINT", vec![lens(&[b.x, b.y])]);
            let poly = self.w.add("IFCPOLYLINE", vec![refs(&[pa, pb])]);
            reps.push(self.w.add(
                "IFCSHAPEREPRESENTATION",
                vec![r(self.axis), s("Axis"), s("Curve2D"), refs(&[poly])],
            ));
        }
        let shape = self.w.add(
            "IFCPRODUCTDEFINITIONSHAPE",
            vec![V::Null, V::Null, refs(&reps)],
        );
        let origin = self.placement3(vec3(0.0, 0.0, 0.0));
        let pl = self.w.add("IFCLOCALPLACEMENT", vec![r(s_pl), r(origin)]);

        let mut args = vec![
            gid(el.guid),
            r(self.oh),
            s(&el.number),
            s(&art),
            z.object_type.as_deref().map_or(V::Null, s),
            r(pl),
            r(shape),
        ];
        match k.attrs {
            // IfcSpace: LongName, CompositionType, PredefinedType,
            // ElevationWithFlooring
            11 => args.extend([s(&art), e("ELEMENT"), e(z.predefined), V::Null]),
            // Tür und Fenster: Tag, Höhe, Breite, Typ, Teilung
            13 => args.extend([
                s(&el.number),
                V::Null,
                V::Null,
                e(z.predefined),
                V::Null,
                V::Null,
            ]),
            _ => args.extend([s(&el.number), e(z.predefined)]),
        }
        let pi = self.w.add(&k.name.to_ascii_uppercase(), args);

        // Typ und Baustoff
        let mut typ_name = String::new();
        let mut code = String::new();
        if let Some((ls_id, ls)) = layer_set {
            typ_name = ls.name.clone();
            code = ls.code.clone();
            let set = self.aufbau(&ls.name, &ls.code, &ls.layers);
            if !k.type_class.is_empty() && k.attrs == 9 {
                let key = (ls_id.index(), k.type_class);
                let t = match self.typen.get(&key) {
                    Some(&t) => t,
                    None => {
                        let tg = abgeleitet(ls.guid, k.type_class);
                        let mut targs = vec![
                            gid(tg),
                            r(self.oh),
                            s(&ls.name),
                            V::Null,
                            V::Null,
                            V::Null,
                            V::Null,
                            opt(&ls.code),
                            opt(&ls.code),
                        ];
                        // IfcFurnitureType hat vor der Art noch AssemblyPlace
                        if k.type_class == "IfcFurnitureType" {
                            targs.push(e("NOTDEFINED"));
                        }
                        targs.push(e("NOTDEFINED"));
                        let t = self.w.add(&k.type_class.to_ascii_uppercase(), targs);
                        let tg_rel = abgeleitet(tg, "material");
                        self.stoff_nutzer
                            .entry(set)
                            .or_insert((tg_rel, Vec::new()))
                            .1
                            .push(t);
                        self.typen.insert((ls_id.index(), k.type_class), t);
                        t
                    }
                };
                self.typ_nutzer.entry(t).or_default().push(pi);
            }
            // Lage der Schichten am Bauteil
            let usage = match k.name {
                "IfcWall" if achse.is_some() => Some((e("AXIS2"), e("POSITIVE"), 0.0)),
                "IfcSlab" | "IfcRoof" | "IfcCovering" => {
                    let top = geometrie::grenzen(netze).map_or(0.0, |b| b.1.z);
                    Some((e("AXIS3"), e("NEGATIVE"), top))
                }
                _ => None,
            };
            let mat = match usage {
                Some((dir, sense, off)) => self.w.add(
                    "IFCMATERIALLAYERSETUSAGE",
                    vec![r(set), dir, sense, V::Len(off), V::Null],
                ),
                None => set,
            };
            let g = abgeleitet(el.guid, "material");
            self.stoff_nutzer
                .entry(mat)
                .or_insert((g, Vec::new()))
                .1
                .push(pi);
        } else if layers.len() > 1 {
            let set = self.aufbau("", "", &layers);
            let g = abgeleitet(el.guid, "material");
            self.stoff_nutzer
                .entry(set)
                .or_insert((g, Vec::new()))
                .1
                .push(pi);
        } else {
            // Ein Baustoff (aus der Schicht oder dem Körper)
            let mut mats: Vec<MaterialId> = layers.iter().map(|l| l.material).collect();
            if mats.is_empty() {
                for n in netze {
                    if let Some(mid) = self.material_of_key(n.mat) {
                        if !mats.contains(&mid) {
                            mats.push(mid);
                        }
                    }
                }
            }
            match mats.as_slice() {
                [] => {}
                [one] => {
                    if let Some(mi) = self.stoff(*one) {
                        let g = m.material(*one).map_or(el.guid, |x| x.guid);
                        let g = abgeleitet(g, "users");
                        self.stoff_nutzer
                            .entry(mi)
                            .or_insert((g, Vec::new()))
                            .1
                            .push(pi);
                    }
                }
                many => {
                    let parts: Vec<u32> = many
                        .iter()
                        .filter_map(|mid| {
                            let mi = self.stoff(*mid)?;
                            Some(self.w.add(
                                "IFCMATERIALCONSTITUENT",
                                vec![V::Null, V::Null, r(mi), V::Null, V::Null],
                            ))
                        })
                        .collect();
                    let set = self.w.add(
                        "IFCMATERIALCONSTITUENTSET",
                        vec![s(&art), V::Null, refs(&parts)],
                    );
                    let g = abgeleitet(el.guid, "material");
                    self.stoff_nutzer
                        .entry(set)
                        .or_insert((g, Vec::new()))
                        .1
                        .push(pi);
                }
            }
        }

        // Eigenschaften
        let interior = el.category == Category::InteriorWall;
        let bearing = match el.category {
            Category::ExteriorWall | Category::InteriorWall | Category::Parapet => {
                Some(library::bears(&layers, interior))
            }
            Category::Floor | Category::GroundSlab | Category::StripFooting => Some(true),
            _ => None,
        };
        let u = layer_set.and_then(|(ls, _)| m.u_value(ls));
        if let Some((pset, props)) = k.pset {
            let mut ps = Vec::new();
            for p in props {
                let v = match p {
                    Prop::Reference if !code.is_empty() => {
                        Some(("Reference", typed("IFCIDENTIFIER", s(&code))))
                    }
                    Prop::IsExternal if ext.is_none() => Some((
                        "IsExternal",
                        typed("IFCBOOLEAN", V::Bool(el.category.is_external())),
                    )),
                    Prop::LoadBearing => {
                        bearing.map(|b| ("LoadBearing", typed("IFCBOOLEAN", V::Bool(b))))
                    }
                    Prop::ThermalTransmittance => u.map(|u| {
                        (
                            "ThermalTransmittance",
                            typed("IFCTHERMALTRANSMITTANCEMEASURE", V::Real(round9(u))),
                        )
                    }),
                    _ => None,
                };
                if let Some((name, v)) = v {
                    ps.push(self.single(name, v));
                }
            }
            if !ps.is_empty() {
                self.pset(abgeleitet(el.guid, pset), pset, &ps, pi);
            }
        }
        let kg = layers
            .iter()
            .position(|l| l.core)
            .and_then(|i| m.layer_kg(id, i))
            .or_else(|| ext.map_or(el.category.din276(), |d| d.kg()));
        let trade = layers
            .iter()
            .position(|l| l.core)
            .or((!layers.is_empty()).then_some(0))
            .and_then(|i| m.layer_trade(id, i))
            // ohne Schichten: das der Erweiterung bzw. der Bauteilart
            .or_else(|| match ext {
                Some(d) => m.trade_by_code(d.def.bauteil_feld("gewerk")?),
                None => m.trade_by_code(sk_model::kinds::spec(el.category).default_trade?),
            })
            .and_then(|t| m.trade(t));
        let gewerk = trade.map(|t| format!("{} {}", t.code, t.name));
        // Als Klassifikation nur Leistungsbereiche der VOB/C (ATV DIN 18299
        // ff.); eigene Nummern der Firma bleiben nur Eigenschaft
        if let Some(t) = trade.filter(|t| atv(&t.code)) {
            self.atv_nutzer
                .entry((t.code.clone(), t.name.trim().to_string()))
                .or_default()
                .push(pi);
        }
        let mut eigen = vec![
            self.single("Nummer", typed("IFCIDENTIFIER", s(&el.number))),
            self.single("Bauteilart", typed("IFCLABEL", s(&art))),
        ];
        if !typ_name.is_empty() {
            eigen.push(self.single("Bauteiltyp", typed("IFCLABEL", s(&typ_name))));
        }
        if let Some(kg) = kg {
            eigen.push(self.single("Kostengruppe", typed("IFCIDENTIFIER", s(&kg.to_string()))));
        }
        if let Some(g) = gewerk {
            eigen.push(self.single("Gewerk", typed("IFCLABEL", s(&g))));
        }
        self.pset(
            abgeleitet(el.guid, "Skizzeo_Bauteil"),
            "Skizzeo_Bauteil",
            &eigen,
            pi,
        );
        if let Some(kg) = kg {
            self.kg_nutzer.entry(kg).or_default().push(pi);
        }
        if let Some(q) = self.mengen.get(&id) {
            for satz in mengen::saetze(q, k.name) {
                self.mengensatz(abgeleitet(el.guid, satz.name), &satz, pi);
            }
        }
        pi
    }

    /// Ein Mengensatz am Bauteil `to`.
    fn mengensatz(&mut self, g: Guid, satz: &mengen::Satz, to: u32) {
        let qs: Vec<u32> = satz
            .mengen
            .iter()
            .map(|q| {
                let v = match q.art {
                    mengen::Art::Anzahl if q.wert.fract() == 0.0 => V::Int(q.wert as i64),
                    _ => V::Real(round9(q.wert)),
                };
                self.w.add(
                    q.art.klasse(),
                    vec![s(&q.name), V::Null, V::Null, v, V::Null],
                )
            })
            .collect();
        let eq = self.w.add(
            "IFCELEMENTQUANTITY",
            vec![
                gid(g),
                r(self.oh),
                s(satz.name),
                V::Null,
                V::Null,
                refs(&qs),
            ],
        );
        self.definiert(g, eq, to);
    }

    fn material_of_key(&self, key: u16) -> Option<MaterialId> {
        let mat = self.m.material_by_key(key)?;
        self.m
            .materials()
            .iter()
            .find(|(_, x)| x.guid == mat.guid)
            .map(|(id, _)| id)
    }

    fn single(&mut self, name: &str, v: V) -> u32 {
        self.w
            .add("IFCPROPERTYSINGLEVALUE", vec![s(name), V::Null, v, V::Null])
    }

    fn pset(&mut self, g: Guid, name: &str, props: &[u32], to: u32) {
        let ps = self.w.add(
            "IFCPROPERTYSET",
            vec![gid(g), r(self.oh), s(name), V::Null, refs(props)],
        );
        self.definiert(g, ps, to);
    }

    /// Eigenschafts- oder Mengensatz `ps` (Kennung `g`) am Bauteil `to`.
    fn definiert(&mut self, g: Guid, ps: u32, to: u32) {
        self.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            vec![
                gid(abgeleitet(g, "rel")),
                r(self.oh),
                V::Null,
                V::Null,
                refs(&[to]),
                r(ps),
            ],
        );
    }

    /// Achse einer Wand: die Außenkante ihres Fußes (Bezugsseite der
    /// Schichten), so gerichtet, dass der Körper links liegt (Schichten
    /// laufen in +y der Achse), im Geschoss.
    fn wall_axis(&self, id: ElementId, _elev: f64, netze: &[Netz]) -> Option<(Vec3, Vec3)> {
        let (run, seg) = self.m.segment_of(id)?;
        let chain = self.m.chain(run)?;
        let (a, b) = *chain.outer_foot().get(seg)?;
        let (lo, hi) = geometrie::grenzen(netze)?;
        let mid = (lo + hi) * 0.5;
        let d = b - a;
        let left = d.x * (mid.y - a.y) - d.y * (mid.x - a.x) > 0.0;
        let (a, b) = if left { (a, b) } else { (b, a) };
        Some((vec3(a.x, a.y, 0.0), vec3(b.x, b.y, 0.0)))
    }
}

/// ATV-Nummer der VOB/C: „18299“ … „18459“ (fünf Ziffern, 18…).
fn atv(code: &str) -> bool {
    code.len() == 5 && code.starts_with("18") && code.bytes().all(|b| b.is_ascii_digit())
}

/// Die Zeile der Mengenliste je Bauteil, das eine Menge hat.
fn mengen_je_bauteil(m: &Model) -> HashMap<ElementId, ElementQto> {
    let liste = sk_model::qto::schedule(m);
    liste
        .buildings
        .into_iter()
        .flat_map(|b| b.storeys)
        .chain(liste.loose)
        .flat_map(|st| st.groups)
        .flat_map(|g| g.rows)
        .filter_map(|z| Some((z.element, z.q?)))
        .collect()
}

fn round9(x: f64) -> f64 {
    let v = (x * 1e9).round() / 1e9;
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zeit() {
        assert_eq!(iso_zeit(0), "1970-01-01T00:00:00");
        assert_eq!(iso_zeit(1_791_819_528), "2026-10-12T15:38:48");
        assert_eq!(iso_zeit(951_782_400), "2000-02-29T00:00:00");
    }

    #[test]
    fn winkel() {
        assert_eq!(
            grad_min_sek(53.0 + 2.0 / 60.0 + 3.5 / 3600.0),
            V::List(vec![V::Int(53), V::Int(2), V::Int(3), V::Int(500_000)])
        );
        assert_eq!(
            grad_min_sek(-8.5),
            V::List(vec![V::Int(-8), V::Int(-30), V::Int(0), V::Int(0)])
        );
    }

    #[test]
    fn kennung_stabil() {
        let g = Guid(42);
        assert_eq!(abgeleitet(g, "site"), abgeleitet(g, "site"));
        assert_ne!(abgeleitet(g, "site"), abgeleitet(g, "contains"));
    }
}

//! Szene, Teil „Kosten und Mengen“ (aus scene.rs, Paket T2).

use super::*;

impl Scene {
    /// Mengenliste (B7): einmal je Modellstand berechnet und gemerkt, nie
    /// während eines offenen Schritts (Ziehen); dann bleibt der alte Stand.
    pub fn schedule(&mut self) -> &Schedule {
        self.netz_fertig();
        let rev = self.model.revision();
        let fresh = matches!(&self.schedule, Some((r, _)) if *r == rev);
        if !fresh && (self.schedule.is_none() || !self.model.in_step()) {
            self.schedule = Some((rev, self.mengen_neu()));
            self.schedule_runs += 1;
        }
        &self.schedule.as_ref().expect("gerade berechnet").1
    }

    /// Mengenliste im Umfang `u` (KA-1): gefiltert aus [`Scene::schedule`]
    /// über `Schedule::restrict`, gemerkt je Berechnung und Umfang. Rechnet
    /// keine Geometrie; ein Chip-Klick rechnet die Liste nicht neu.
    pub fn schedule_in(&mut self, u: &Umfang) -> Rc<Schedule> {
        self.schedule();
        let runs = self.schedule_runs;
        let full = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        if u.alles() {
            return full;
        }
        let s = match self.gemerkte_liste(runs, u) {
            Some(s) => s,
            None => Rc::new(full.restrict(&self.model, u)),
        };
        self.schedule_in = Some((runs, u.clone(), s.clone()));
        s
    }

    /// Eine schon eingeschränkte Liste im Umfang `u` aus der Berechnung
    /// `runs`, aus [`Scene::schedule_in`] oder der Vorschau: jede Liste nur
    /// einmal im Speicher (Review #213 H-213n).
    fn gemerkte_liste(&self, runs: u64, u: &Umfang) -> Option<Rc<Schedule>> {
        self.schedule_in
            .iter()
            .chain(
                self.kostenspeicher_live
                    .iter()
                    .filter_map(|e| e.liste.as_ref()),
            )
            .find(|(r, w, _)| *r == runs && w == u)
            .map(|(_, _, s)| s.clone())
    }

    /// Wirksamer Kostenkatalog (Projekt, Firma oder Werk), gemerkt je
    /// `ext_revision`, Firmenstand und Baustoffen und Gewerken des Modells
    /// (`lesen::umfeld_stempel`). `firma`: Firmenkatalog und sein Stand
    /// ([`crate::catalog::Company::stand`]).
    pub fn katalog(&mut self, firma: Option<(&sk_model::Library, u64)>) -> Rc<sk_cost::Katalog> {
        let key = (
            self.model.ext_revision(),
            firma.map_or(0, |f| f.1),
            sk_cost::lesen::umfeld_stempel(&self.model),
        );
        if let Some((k, kat)) = &self.katalog {
            if *k == key {
                return kat.clone();
            }
        }
        let kat = Rc::new(sk_cost::lesen::katalog(&self.model, firma.map(|f| f.0)));
        self.katalog = Some((key, kat.clone()));
        kat
    }

    /// Kostenblatt im Umfang `u` (KA-2, paket-ka2 §3): `lesen::kosten_mit`
    /// über die Liste aus [`Scene::schedule`] und den Zwischenspeicher,
    /// gemerkt je Berechnung der Liste, Katalogstempel und Umfang. Während
    /// eines offenen Schritts bleibt es wie die Liste beim letzten Stand.
    pub fn kostenblatt(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        u: &Umfang,
    ) -> Rc<sk_cost::Kostenblatt> {
        let kat = self.katalog(firma);
        self.schedule();
        let key = (self.schedule_runs, kat.stempel, u.clone());
        if let Some(i) = self.kostenblatt.iter().position(|(k, _)| *k == key) {
            let e = self.kostenblatt.remove(i);
            let b = e.1.clone();
            self.kostenblatt.insert(0, e);
            return b;
        }
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        let sp = std::mem::take(&mut self.kostenspeicher);
        let (b, sp) = sk_cost::lesen::kosten_mit(sp, &self.model, &sched, &kat, u);
        self.kostenspeicher = sp;
        let b = Rc::new(b);
        self.kostenblatt.insert(0, (key, b.clone()));
        // Umfang der Ansicht, Projekt für das LV und ein weiterer
        self.kostenblatt.truncate(3);
        b
    }

    /// Leistungsverzeichnis des Loses `w.los` im Umfang `u` (KA-4,
    /// paket-ka4 §2): `lv::lv_aus` auf dem Kostenblatt desselben Umfangs,
    /// gemerkt je Kostenblatt, Modellstand und Wahl.
    pub fn lv(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        u: &Umfang,
        w: &sk_cost::lv::LvWahl,
    ) -> Rc<sk_cost::lv::Lv> {
        let kat = self.katalog(firma);
        let blatt = self.kostenblatt(firma, u);
        let von = (self.schedule_runs, kat.stempel, u.clone());
        let key: LvSchluessel = (von, self.model.revision(), w.clone());
        if let Some((_, lv)) = self.lv.iter().find(|(k, _)| *k == key) {
            return lv.clone();
        }
        // Beim Ziehen (offener Schritt) steht das Kostenblatt bis zum
        // Loslassen; Kopf und Geschossnamen ändern sich dabei nicht. Das
        // letzte LV derselben Wahl gilt weiter, statt jedes Bild alle Lose
        // neu zu ordnen (Review 3ao)
        if self.model.in_step() {
            if let Some((_, lv)) = self.lv.iter().find(|(k, _)| k.0 == key.0 && k.2 == key.2) {
                return lv.clone();
            }
        }
        // Indizes je Dicke aus dem Blatt des Projekts, in jedem Umfang
        // dieselben (Review #213 B2); nach dem Blatt des Umfangs, damit der
        // Kostenspeicher danach alle Typen des Projekts hält
        let lv = if u.alles() {
            sk_cost::lv::lv_aus(&self.model, &blatt, &kat, w)
        } else {
            let projekt = self.kostenblatt(firma, &Umfang::projekt());
            let rang = match &self.dickenrang {
                Some((b, r)) if Rc::ptr_eq(b, &projekt) => r.clone(),
                _ => {
                    let r = Rc::new(sk_cost::lv::Dickenrang::aus(&self.model, &projekt));
                    self.dickenrang = Some((projekt, r.clone()));
                    r
                }
            };
            sk_cost::lv::lv_im_umfang(&self.model, &blatt, &rang, &kat, w)
        };
        let lv = Rc::new(lv);
        // Ein anderes Blatt oder ein anderer Stand macht die alten ungültig
        self.lv.retain(|(k, _)| k.0 == key.0 && k.1 == key.1);
        self.lv.insert(0, (key, lv.clone()));
        self.lv.truncate(8);
        lv
    }

    /// Katalog der Firma oder des Werks ohne die Werte des Projekts
    /// (`lesen::firma_oder_werk`), gemerkt wie [`Scene::katalog`]: die
    /// Firmenwerte im Preisblatt und der Punkt „im Projekt geändert“.
    pub fn firmenkatalog(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
    ) -> Rc<sk_cost::Katalog> {
        let key = (
            self.model.ext_revision(),
            firma.map_or(0, |f| f.1),
            sk_cost::lesen::umfeld_stempel(&self.model),
        );
        if let Some((k, kat)) = &self.firmenkatalog {
            if *k == key {
                return kat.clone();
            }
        }
        let kat = Rc::new(sk_cost::lesen::firma_oder_werk(
            &self.model,
            firma.map(|f| f.0),
        ));
        self.firmenkatalog = Some((key, kat.clone()));
        kat
    }

    /// Kosten mit noch nicht ausgeführten Operationen (Preisblatt beim
    /// Tippen, paket-ka2 §4): der Katalog des Plans und je Umfang das
    /// Kostenblatt darauf. Modell, `revision` und Verlauf bleiben (Regel 94).
    pub fn kosten_live(
        &mut self,
        firma: Option<(&sk_model::Library, u64)>,
        ops: &[sk_cost::Op],
        umfaenge: &[&Umfang],
    ) -> Result<KostenLive, Vec<sk_cost::Befund>> {
        self.schedule();
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        // Preise und Aufwandswerte direkt auf dem wirksamen Katalog
        // (`Katalog::mit`), alles andere über den Plan
        let k = match self.katalog(firma).mit(ops) {
            Some(k) => k,
            None => sk_cost::vorschau(&self.model, firma.map(|f| f.0), self.rolle, ops)?.katalog,
        };
        let k = Rc::new(k);
        // Gleiche Umfänge (Blatt und Chips ohne Abwahl) nur einmal rechnen
        let mut b: Vec<Rc<sk_cost::Kostenblatt>> = Vec::new();
        // Ein Aufruf mit weniger Umfängen (Hover „übernehmen“) lässt die
        // Speicher der übrigen stehen (Review #213 H-213d)
        if self.kostenspeicher_live.len() < umfaenge.len() {
            self.kostenspeicher_live
                .resize_with(umfaenge.len(), Default::default);
        }
        let runs = self.schedule_runs;
        for (i, u) in umfaenge.iter().enumerate() {
            let gleich = umfaenge[..i].iter().position(|x| x == u);
            b.push(match gleich {
                Some(j) => b[j].clone(),
                None => {
                    let liste = if u.alles() {
                        sched.clone()
                    } else {
                        let l = self
                            .gemerkte_liste(runs, u)
                            .unwrap_or_else(|| Rc::new(sched.restrict(&self.model, u)));
                        self.kostenspeicher_live[i].liste = Some((runs, (*u).clone(), l.clone()));
                        l
                    };
                    let e = &mut self.kostenspeicher_live[i];
                    // die Liste ist schon im Umfang
                    let sp = std::mem::take(&mut e.speicher);
                    let (blatt, sp) =
                        sk_cost::lesen::kosten_mit(sp, &self.model, &liste, &k, &Umfang::projekt());
                    e.speicher = sp;
                    Rc::new(blatt)
                }
            });
        }
        Ok((k, b))
    }

    /// Die gemerkte Liste des Umfangs `i` der Vorschau.
    #[cfg(test)]
    pub fn kosten_live_liste(&self, i: usize) -> Option<Rc<Schedule>> {
        self.kostenspeicher_live
            .get(i)?
            .liste
            .as_ref()
            .map(|l| l.2.clone())
    }

    /// Je Umfang der Vorschau: wie viele Typen der letzte Aufruf neu
    /// zugeordnet hat.
    #[cfg(test)]
    pub fn kosten_live_neu(&self) -> Vec<u64> {
        self.kostenspeicher_live
            .iter()
            .map(|s| s.speicher.neu_zugeordnet())
            .collect()
    }

    /// Liegt die Liste hinter dem Modell („wird aktualisiert“)?
    pub fn schedule_stale(&self) -> bool {
        self.schedule
            .as_ref()
            .is_some_and(|(r, _)| *r != self.model.revision())
    }

    /// Anzahl der Berechnungen der Liste.
    pub fn schedule_runs(&self) -> u64 {
        self.schedule_runs
    }

    /// Kosten (KA-0d, Bausteingrenze §5): der eine Eingang der App für
    /// Stammdaten im Projekt. Eine Operation ist genau ein Rückgängig-Schritt
    /// mit `op.bezeichnung()`; ein Fehler ändert nichts und liefert die
    /// Befunde. Bis KA-3 immer als Administrator.
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-1)
    pub fn kosten(
        &mut self,
        firma: Option<&sk_model::Library>,
        herkunft: &sk_cost::Herkunft,
        op: sk_cost::Op,
    ) -> Result<(), Vec<sk_cost::Befund>> {
        let label = sk_model::step_label(op.bezeichnung());
        self.kosten_folge(label, firma, herkunft, std::slice::from_ref(&op))
    }

    /// Mehrere Kostenoperationen als **ein** Schritt `label`; scheitert eine,
    /// bleibt nichts (Abnahme 13).
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-1)
    pub fn kosten_folge(
        &mut self,
        label: &'static str,
        firma: Option<&sk_model::Library>,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<(), Vec<sk_cost::Befund>> {
        self.netz_fertig();
        // Erst prüfen: ein abgelehnter Plan öffnet keinen Schritt
        sk_cost::vorschau(&self.model, firma, self.rolle, ops)?;
        self.begin(label);
        match sk_cost::ausfuehren_folge(&mut self.model, firma, self.rolle, herkunft, ops) {
            Ok(()) => {
                self.commit();
                Ok(())
            }
            Err(b) => {
                self.rollback();
                Err(b)
            }
        }
    }

    /// Ausdrücklich „Auch für neue Häuser“ (Bausteingrenze §5, KA-2c2):
    /// prüfen, dann die Firma als neuer Stand mit `[log]`
    /// ([`crate::catalog::Company::fuer_firma`]), dann, nur mit
    /// Projektkopie, ein Projektschritt `StandUebernehmen` mit `label`. Die
    /// Firma ist nie Teil des Rückgängig-Schritts; Strg+Z nimmt nur das
    /// Projekt zurück. `Err`: nichts geschrieben, der Satz für die Meldung.
    /// `Ok`: ein Hinweis für die Statuszeile, wenn es einen gibt.
    ///
    /// Werte, die dieses Haus selbst abweichend hält, bleiben (Regel 89),
    /// etwa beim OK der Verwaltung. Wer an diesem Haus ausdrücklich „für
    /// dieses und neue Häuser“ ändert, nimmt [`Scene::fuer_firma_auch_hier`].
    pub fn fuer_firma(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        self.fuer_firma_mit(label, firma, herkunft, ops, None, false)
    }

    /// `check`-Schritte eines Ablaufs für dieses Haus (BIM §3.15 `rule`,
    /// Review 3ax): `ops` auf einer Kopie des Modells, dann der erste
    /// Befund mit einer der `regeln`, wie in der Verwaltung. `None`: nichts
    /// sperrt (ohne Regeln ohne Kopie).
    pub fn ablauf_pruefen(
        &self,
        firma: Option<&sk_model::Library>,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
        regeln: &[u16],
    ) -> Option<String> {
        if regeln.is_empty() {
            return None;
        }
        // Auf einer Kopie mit offenem Schritt; die Kopie wird verworfen
        let mut m = self.model.clone();
        if !m.in_step() {
            m.begin("Ablauf prüfen");
        }
        if let Err(b) = sk_cost::ausfuehren_folge(&mut m, firma, self.rolle, herkunft, ops) {
            return b.first().map(|b| b.satz.clone());
        }
        sk_cost::lesen::katalog(&m, firma)
            .befunde
            .into_iter()
            .find(|b| regeln.contains(&b.regel))
            .map(|b| b.satz)
    }

    /// „Der Firma vorschlagen“ (KA-3b4, Bausteingrenze §5, Regel 105):
    /// `ops` gelten in diesem Haus mit Marke (ein Rückgängig-Schritt mit
    /// `label`), und was sie an Preis, Stunden und Lohn ändern, steht als
    /// Vorschlag im Entwurf des Firmenkatalogs. Erst geprüft, dann der
    /// Vorschlag, dann das Haus; `Err`: nichts geändert. `name`:
    /// Projektname für die Verwaltung.
    pub fn der_firma_vorschlagen(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
        name: &str,
    ) -> Result<crate::meldung::Meldung, crate::meldung::Meldung> {
        use crate::meldung::Meldung;
        let satz = |b: Vec<sk_cost::Befund>| Meldung::aus_befunden(&b, "Nichts geändert.");
        sk_cost::vorschau(&self.model, Some(firma.library()), self.rolle, ops).map_err(satz)?;
        let k = sk_cost::lesen::firma_oder_werk(&sk_model::Model::new(), Some(firma.library()));
        let werte = sk_cost::vorschlag_werte(ops, &k);
        let vorgeschlagen = !werte.is_empty();
        if vorgeschlagen {
            let op = sk_cost::Op::VorschlagFuerFirma {
                projekt: self.model.project().guid,
                name: name.to_string(),
                werte,
            };
            firma.vorschlagen(herkunft, &op)?;
        }
        // Geprüft war es; scheitert das Haus trotzdem, steht der Vorschlag
        if let Err(b) = self.kosten_folge(label, Some(firma.library()), herkunft, ops) {
            if !vorgeschlagen {
                return Err(satz(b));
            }
            return Ok(Meldung::mit(
                "Der Firma vorgeschlagen, dieses Haus nicht geändert: {}",
                &[&satz(b)],
            ));
        }
        Ok(Meldung::satz(if vorgeschlagen {
            crate::catalog::VORGESCHLAGEN
        } else {
            crate::catalog::NICHT_VORGESCHLAGEN
        }))
    }

    /// „Freigeben als Stand n+1“ aus der Vorschau der Verwaltung (KA-3b3,
    /// [`crate::catalog::Company::freigeben`]); dieses Haus zieht nach wie
    /// bei [`Scene::fuer_firma`].
    pub fn freigeben(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        self.fuer_firma_mit(label, firma, herkunft, &[], None, true)
    }

    /// Wie [`Scene::fuer_firma`], aber dieses Haus übernimmt den gewählten
    /// Wert auch dort, wo es abweicht: Preisblatt und Lohnkarte, an denen
    /// der Nutzer genau diesen Wert für dieses und neue Häuser setzt (Regel
    /// 89, „außer der Nutzer wählt sie ausdrücklich“). `ops` sind die
    /// Operationen am Haus; die Firma bekommt nur den gewählten Wert
    /// ([`sk_cost::preis::auf_firma`]), und wo das Haus abweicht, behält es
    /// seine übrigen eigenen Werte.
    pub fn fuer_firma_auch_hier(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        // Mit Verwaltungskennwort geht es in den Entwurf: dann gelten dessen
        // Werte als Grundlage, sonst nähme die Bauleistung die übrigen Felder
        // des freigegebenen Stands mit und überschriebe den Entwurf (Review
        // 3au)
        let entwurf = firma
            .entwurf()
            .filter(|_| sk_cost::verwaltung::hat_kennwort(firma.library()))
            .and_then(|t| sk_model::read_szk_with(t, &sk_cost::lesen::ABSCHNITTE_SZK).ok())
            .map(|l| sk_cost::verwaltung::wie_freigegeben(&l));
        let fk = sk_cost::lesen::firma_oder_werk(
            &self.model,
            Some(entwurf.as_ref().unwrap_or(firma.library())),
        );
        let fuer_firma = sk_cost::preis::auf_firma(ops, &fk);
        // Dieses Haus nimmt dieselbe Zeile wie der Entwurf: Nach „Freigeben“
        // ist sie gleich der Firma, und der eigene Vermerk fällt weg. Mit
        // der Zeile des Hauses bliebe es bei dessen übrigen Feldern (Befund H).
        // Ein Satz mit eigenen Werten (Regel 89) behält die Zeile des Hauses
        // wie ohne Kennwort; nur der gewählte Wert kommt dazu (Review 3av)
        let hier: Vec<sk_cost::Op> = match sk_cost::verwaltung::hat_kennwort(firma.library()) {
            true => ops
                .iter()
                .zip(&fuer_firma)
                .map(|(haus, entwurf)| {
                    let eigen = ziel(haus).is_some_and(|z| {
                        sk_cost::op::ohne_abweichung(&self.model, vec![z]).is_empty()
                    });
                    if eigen { haus } else { entwurf }.clone()
                })
                .collect(),
            false => ops.to_vec(),
        };
        self.fuer_firma_mit(label, firma, herkunft, &fuer_firma, Some(&hier), false)
    }

    /// `hier`: die Operationen am Haus, wenn es den gewählten Wert auch
    /// dort übernimmt, wo es abweicht; `None`: es behält seine Abweichungen.
    /// `freigabe`: statt `ops` den Entwurf freigeben.
    fn fuer_firma_mit(
        &mut self,
        label: &'static str,
        firma: &mut crate::catalog::Company,
        herkunft: &sk_cost::Herkunft,
        ops: &[sk_cost::Op],
        hier: Option<&[sk_cost::Op]>,
        freigabe: bool,
    ) -> Result<Option<crate::meldung::Meldung>, crate::meldung::Meldung> {
        use crate::meldung::Meldung;
        let satz = |b: Vec<sk_cost::Befund>| Meldung::aus_befunden(&b, "Nichts geändert.");
        // Den Firmenkatalog ändert nur, wer verwaltet (KA-3b1)
        if self.rolle != sk_cost::Rolle::Admin {
            let was = match ops.first() {
                Some(op) => Some(op.bezeichnung()),
                None => freigabe.then(|| "Freigeben".to_string()),
            };
            if let Some(was) = was {
                return Err(satz(vec![sk_cost::Befund::fehler(
                    93,
                    sk_cost::befund::r93(&was, "nur in der Verwaltung"),
                    sk_cost::befund::Ort::Datei,
                )]));
            }
        }
        // Vorab am Haus geprüft; das Kennwort gilt nur in der Firma und
        // prüft `Company::fuer_firma`
        let am_haus: Vec<sk_cost::Op> = ops
            .iter()
            .filter(|o| !matches!(o, sk_cost::Op::KennwortSetzen { .. }))
            .cloned()
            .collect();
        if !am_haus.is_empty() {
            sk_cost::vorschau(&self.model, Some(firma.library()), self.rolle, &am_haus)
                .map_err(satz)?;
        }
        // Mit Verwaltungskennwort ändert nur die Freigabe den Firmenkatalog:
        // Die Änderung kommt in den Entwurf, dieses Haus nimmt den
        // gewählten Wert als eigenen (KA-3b2; bis KA-3b4 „Der Firma
        // vorschlagen“ bringt)
        if !freigabe && sk_cost::verwaltung::hat_kennwort(firma.library()) {
            firma.fuer_entwurf(herkunft, ops)?;
            if let Some(hier) = hier.filter(|h| !h.is_empty()) {
                if let Err(b) = self.kosten_folge(label, Some(firma.library()), herkunft, hier) {
                    return Ok(Some(Meldung::mit(
                        "Im Entwurf gespeichert, dieses Haus nicht geändert: {}",
                        &[&satz(b)],
                    )));
                }
            }
            return Ok(Some(Meldung::satz(crate::catalog::IM_ENTWURF)));
        }
        // Ohne Kopie rechnet dieses Haus mit der Firma. Damit Strg+Z genau
        // diese Änderung zurücknimmt (und nicht den letzten Bauschritt),
        // bekommt es vorher still die Kopie der bisherigen Werte; der
        // Schritt danach bringt die neuen. Rückgängig: dieses Haus wieder
        // mit den bisherigen Werten, neue Häuser mit den neuen.
        let vorher = (!sk_cost::op::hat_kopie(&self.model)).then(|| firma.library().clone());
        let neu = if freigabe {
            firma.freigeben(herkunft)?
        } else {
            firma.fuer_firma(herkunft, ops)?
        };
        let mut hinweis = None;
        if let Some(alt) = vorher.filter(|_| !neu.saetze.is_empty()) {
            self.begin(label);
            sk_cost::op::kopie_anlegen(&mut self.model, Some(&alt));
            // ohne eigenen Schritt: der Stand vor der Änderung
            let _ = self.model.try_commit();
            // Das Modell hat sich außerhalb des Verlaufs geändert: Ein
            // Wiederholen von vorher passte nicht mehr dazu (es legte z. B.
            // eine schon zurückgenommene Kopie ein zweites Mal an), auch
            // wenn der Schritt danach scheitert oder nichts ändert
            self.redo.clear();
        }
        // Wo das Haus abweicht, bleibt der Satz des Hauses; ausdrücklich
        // gewählt, kommt die Operation am Haus dazu (nur der gewählte Wert)
        let saetze = sk_cost::op::ohne_abweichung(&self.model, neu.saetze.clone());
        let mut folge: Vec<sk_cost::Op> = hier
            .unwrap_or_default()
            .iter()
            .filter(|op| ziel(op).is_some_and(|z| neu.saetze.contains(&z) && !saetze.contains(&z)))
            .cloned()
            .collect();
        if !saetze.is_empty() {
            folge.insert(0, sk_cost::Op::StandUebernehmen { saetze });
        }
        // Nach der Freigabe ist ein eigener Wert, den dieses Haus mit „Auch
        // für neue Häuser“ in den Entwurf gab, der Firmenwert: Vermerk lösen,
        // sonst folgte das Haus späteren Firmenänderungen nicht mehr
        if freigabe {
            let frei =
                sk_cost::abgleich::eigen_wie_firma(&self.model, Some(firma.library()), &neu.saetze);
            if !frei.is_empty() {
                folge.insert(0, sk_cost::Op::AbweichungZuruecknehmen { saetze: frei });
            }
        }
        if sk_cost::op::hat_kopie(&self.model) && !folge.is_empty() {
            if let Err(b) = self.kosten_folge(label, Some(firma.library()), herkunft, &folge) {
                hinweis = Some(Meldung::mit(
                    "Für neue Häuser gespeichert, dieses Haus nicht geändert: {}",
                    &[&satz(b)],
                ));
            }
        }
        Ok(hinweis)
    }

    /// Vorschau der Kostenoperationen: geänderte Sätze alt/neu, der wirksame
    /// Katalog und das Netto im Umfang `u` vorher und nachher; Modell,
    /// `revision` und Verlauf bleiben (Abnahme 12).
    #[cfg_attr(not(test), allow(dead_code))] // Kostenreiter (KA-2)
    pub fn kosten_vorschau(
        &mut self,
        firma: Option<&sk_model::Library>,
        ops: &[sk_cost::Op],
        u: &sk_model::qto::Umfang,
    ) -> Result<sk_cost::Plan, Vec<sk_cost::Befund>> {
        self.schedule();
        let sched = self.schedule.as_ref().expect("gerade berechnet").1.clone();
        sk_cost::vorschau_kosten(&self.model, &sched, firma, self.rolle, ops, u)
    }
}

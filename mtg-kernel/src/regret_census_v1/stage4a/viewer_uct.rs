//! Passive UCT presentation capture. These records never feed selection.
use super::{
    diag::SelSrc,
    labels,
    tree::{Canon, Key, Tree},
};
use crate::{
    ids::PlayerId,
    rl::PlayerSeatV1,
    rl_session::{FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};

pub(super) const SCHEMA: &str = "mtg-kernel-uct-viewer-capture/v1";

fn visible(s: &FastActorSessionV1) -> Result<Value, String> {
    let v = serde_json::to_value(s.viewer_observation_v1(PlayerSeatV1::P0)?)
        .map_err(|e| e.to_string())?;
    Ok(compact_public(v))
}

// Keep the full observed board while dropping redundant observation diagnostics.
// This representation never feeds engine state or action selection.
fn compact_public(v: Value) -> Value {
    fn fields(v: &Value, names: &[&str]) -> Value {
        Value::Object(
            names
                .iter()
                .filter_map(|&name| v.get(name).map(|value| (name.to_owned(), value.clone())))
                .collect(),
        )
    }
    fn objects(v: &Value) -> Value {
        if let Some(a) = v.as_array() {
            return Value::Array(a.iter().map(objects).collect());
        }
        let mut o = fields(
            v,
            &[
                "card_name",
                "face_index",
                "tapped",
                "damage",
                "counters",
                "is_token",
            ],
        );
        o["stable"] = fields(
            &v["stable"],
            &["arena_id", "card_db_id", "owner", "controller"],
        );
        o["characteristics"] = fields(
            &v["characteristics"],
            &["effective_power", "effective_toughness"],
        );
        o
    }
    let p = &v["projection"];
    let mut projection = fields(
        p,
        &[
            "turn",
            "phase",
            "active_player",
            "priority_player",
            "life_totals",
            "hand_counts",
            "library_counts",
            "mana_pools",
            "combat",
        ],
    );
    for zone in ["battlefield", "graveyards", "exile"] {
        projection[zone] = objects(&p[zone]);
    }
    projection["stack"] = Value::Array(
        p["stack"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                fields(
                    item,
                    &[
                        "source",
                        "source_name",
                        "card_name",
                        "controller",
                        "actor",
                        "targets",
                    ],
                )
            })
            .collect(),
    );
    json!({"acting_player":"p0","projection":projection,"own_hand":[]})
}

fn refs(v: &Value, out: &mut Vec<u64>) {
    match v {
        Value::Object(m) => {
            if let Some(id) = m.get("arena_id").and_then(Value::as_u64) {
                out.push(id);
            } else {
                for x in m.values() {
                    refs(x, out);
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                refs(x, out);
            }
        }
        _ => {}
    }
}

/// Only names transported in explicit public zones can describe a sampled action.
fn public_name(reference: &Value, fallback: &str) -> String {
    reference["card_db_id"]
        .as_u64()
        .and_then(|id| crate::card_def::CARD_DEFS.get(id as usize))
        .map_or_else(|| fallback.to_owned(), |card| card.name.to_owned())
}

fn action(v: &Value, board: &Value, index: usize) -> Value {
    let p = &board["projection"];
    let mut public = HashMap::<u64, String>::new();
    for zone in ["battlefield", "graveyards", "exile"] {
        fn objects(v: &Value, names: &mut HashMap<u64, String>) {
            if let Some(a) = v.as_array() {
                for x in a {
                    objects(x, names);
                }
            } else if let Some(id) = v["stable"]["arena_id"].as_u64() {
                names.insert(id, public_name(&v["stable"], "Public card"));
            }
        }
        objects(&p[zone], &mut public);
    }
    if let Some(stack) = p["stack"].as_array() {
        for x in stack {
            if let Some(id) = x["source"]["arena_id"].as_u64() {
                public.insert(id, public_name(&x["source"], "Public spell or ability"));
            }
        }
    }
    let mut ids = Vec::new();
    refs(v, &mut ids);
    ids.retain(|id| public.contains_key(id));
    ids.sort_unstable();
    ids.dedup();
    let kind = v["action_kind"].as_str().unwrap_or("recorded_action");
    let mut label = kind.replace('_', " ");
    let names = ids.iter().map(|id| public[id].as_str()).collect::<Vec<_>>();
    if !names.is_empty() {
        label.push_str(": ");
        label.push_str(&names.join(" / "));
    }
    if let Some(player) = v["target"]["player"].as_str() {
        label.push_str(if player == "p0" {
            " → yourself"
        } else {
            " → opponent"
        });
    }
    json!({"index":index,"kind":kind,"label":label,"refs":ids.iter().map(|id|format!("o{id}")).collect::<Vec<_>>()})
}

#[derive(Default)]
pub(crate) struct SearchCapture {
    pub(crate) scores: Vec<Option<f64>>,
    pub(crate) error: Option<String>,
    ids: HashMap<Key, u32>,
    snapshots: Vec<Value>,
    snapshot_ids: HashMap<[u8; 32], usize>,
    nodes: Vec<Value>,
    simulations: Vec<Value>,
    current: Value,
    pending: Option<Value>,
    bytes: usize,
}

impl SearchCapture {
    fn attempt(&mut self, f: impl FnOnce(&mut Self) -> Result<(), String>) {
        if self.error.is_none() {
            if let Err(e) = f(self) {
                self.error = Some(e);
            }
        }
    }
    fn charge(&mut self, v: &Value) -> Result<(), String> {
        self.bytes += serde_json::to_vec(v).map_err(|e| e.to_string())?.len();
        if self.bytes > 256 * 1024 * 1024 {
            return Err("UCT observer memory bound".into());
        }
        Ok(())
    }
    fn snapshot(&mut self, s: &FastActorSessionV1) -> Result<usize, String> {
        let v = visible(s)?;
        let bytes = serde_json::to_vec(&v).map_err(|e| e.to_string())?;
        let hash: [u8; 32] = Sha256::digest(&bytes).into();
        if let Some(&id) = self.snapshot_ids.get(&hash) {
            return Ok(id);
        }
        if self.snapshots.len() >= 8192 {
            return Err("UCT snapshot bound".into());
        }
        self.charge(&v)?;
        let id = self.snapshots.len();
        self.snapshots.push(v);
        self.snapshot_ids.insert(hash, id);
        Ok(id)
    }
    pub(crate) fn begin(&mut self, index: u64) {
        self.current = json!({"ordinal":index,"choices":[],"rollout":[]});
        self.pending = None;
    }
    pub(crate) fn rejected(&mut self, index: u64) {
        self.begin(index);
        self.end("rejected");
    }
    pub(crate) fn node(
        &mut self,
        key: &Key,
        id: u32,
        parent: Option<(u32, usize)>,
        cn: &Canon,
        s: &FastActorSessionV1,
    ) {
        self.attempt(|this| {
            if this.nodes.len()>=4096 || id as usize!=this.nodes.len() { return Err("UCT node identity bound".into()); }
            let board=this.snapshot(s)?;
            let d=match s.current_response() {FastActorResponseV1::Decision(d)=>d,_=>return Err("UCT node is terminal".into())};
            let (_,sem)=s.gameplay_trace_visible_v1(d,true).map_err(|e|format!("{e:?}"))?;
            let order=cn.edges.iter().map(|e|super::tree::live_index(cn,e).map_err(|e|format!("{e:?}"))).collect::<Result<Vec<_>,_>>()?;
            let actions=order.iter().enumerate().map(|(i,&live)|action(&serde_json::to_value(&sem[live as usize]).unwrap_or(Value::Null),&this.snapshots[board],i)).collect::<Vec<_>>();
            let row=json!({"id":id,"key":key.iter().map(|b|format!("{b:02x}")).collect::<String>(),"parent":parent,"board":board,"actions":actions,"live_order":order});
            this.charge(&row)?;this.nodes.push(row);this.ids.insert(*key,id);Ok(())
        });
    }
    pub(crate) fn choice(
        &mut self,
        s: &FastActorSessionV1,
        d: &FastActorDecisionV1,
        live: u32,
        at: Option<&(Key, usize)>,
        src: SelSrc,
    ) {
        if let Some((key, edge)) = at {
            self.attempt(|this| {
            let id=*this.ids.get(key).ok_or("UCT choice node missing")?; let board=this.snapshot(s)?;
            let v=json!({"node":id,"action":edge,"live":live,"board":board,"step":d.step,"physical":d.physical_decision_id,"substep":d.substep_index,"expanded":src==SelSrc::Expand,"scores":std::mem::take(&mut this.scores)});
            this.charge(&v)?;this.current["choices"].as_array_mut().ok_or("UCT attempt missing")?.push(v);Ok(())
        });
        }
    }
    pub(crate) fn before(&mut self, s: &FastActorSessionV1, d: FastActorDecisionV1, a: u32) {
        self.attempt(|this| {
            let board=visible(s)?;
            let (_,sem)=s.gameplay_trace_visible_v1(d,true).map_err(|e|format!("{e:?}"))?;
            let selected=sem.get(a as usize).ok_or("UCT selected action missing")?;
            let flags=super::diag::bits(selected,s,PlayerId::P0,&labels::spy_defs());
            let mut v=json!({"actor":d.acting_player,"step":d.step,"physical":d.physical_decision_id,"substep":d.substep_index,"action":action(&serde_json::to_value(selected).map_err(|e|e.to_string())?,&board,a as usize),"flags":flags});
            if flags!=0 { v["before"]=json!(this.snapshot(s)?); }
            this.pending=Some(v);Ok(())
        });
    }
    pub(crate) fn after(&mut self, s: &FastActorSessionV1, events: labels::Events) {
        self.attempt(|this| {
            let mut v = this
                .pending
                .take()
                .ok_or("UCT applied transition missing")?;
            let resolution =
                u8::from(events.self_target_resolved) * 2 + u8::from(events.dr_giant_resolved) * 4;
            v["resolution"] = json!(resolution);
            if v["flags"].as_u64().unwrap_or(0) != 0 || resolution != 0 {
                v["after"] = json!(this.snapshot(s)?);
            }
            if matches!(s.current_response(), FastActorResponseV1::Terminal(_)) {
                this.current["leaf"] = json!(this.snapshot(s)?);
                if let FastActorResponseV1::Terminal(t) = s.current_response() {
                    this.current["terminal"] = json!(t);
                }
            }
            this.charge(&v)?;
            this.current["rollout"]
                .as_array_mut()
                .ok_or("UCT attempt missing")?
                .push(v);
            Ok(())
        });
    }
    pub(crate) fn end(&mut self, reason: &str) {
        self.pending = None;
        self.attempt(|this| {
            if this.simulations.len() >= 4096 {
                return Err("UCT attempt bound".into());
            }
            this.current["reason"] = json!(if reason == "loss" {
                "natural_nonwin"
            } else {
                reason
            });
            this.current["backed_up"] = json!(matches!(reason, "win" | "loss"));
            this.simulations.push(std::mem::take(&mut this.current));
            Ok(())
        });
    }
    pub(crate) fn finish(&mut self, tree: &Tree) {
        self.attempt(|this| {
            for (key, &id) in &this.ids {
                let node = &mut this.nodes[id as usize];
                if let Some(n) = tree.nodes.get(key) {
                    node["committed"] = json!(true);
                    node["visits"] = json!(n.visits);
                    node["n"] = json!(n.n);
                    node["wins"] = json!(n.wins);
                } else {
                    node["committed"] = json!(false);
                    node["visits"] = json!(0);
                    let k = node["actions"].as_array().unwrap().len();
                    node["n"] = json!(vec![0u64; k]);
                    node["wins"] = json!(vec![0u64; k]);
                }
            }
            Ok(())
        });
    }
}

pub(super) struct Recorder {
    file: File,
    pub(super) error: Option<String>,
    bytes: usize,
    count: u64,
    before: Option<Value>,
}
impl Recorder {
    pub(super) fn open(path: &Path, header: Value) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        let mut r = Self {
            file,
            error: None,
            bytes: 0,
            count: 0,
            before: None,
        };
        r.write(header);
        Ok(r)
    }
    fn write(&mut self, mut value: Value) {
        if self.error.is_some() {
            return;
        }
        value["schema"] = json!(SCHEMA);
        let result = (|| -> Result<(), String> {
            let b = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
            if b.len() > 8 * 1024 * 1024 || self.bytes + b.len() + 1 > 512 * 1024 * 1024 {
                return Err("UCT recording byte bound".into());
            }
            self.file
                .write_all(&b)
                .and_then(|_| self.file.write_all(b"\n"))
                .map_err(|e| e.to_string())?;
            self.bytes += b.len() + 1;
            Ok(())
        })();
        if let Err(e) = result {
            self.error = Some(e);
        }
    }
    pub(super) fn before(&mut self, s: &FastActorSessionV1, d: FastActorDecisionV1) {
        if self.error.is_some() {
            return;
        }
        let result = (|| -> Result<Value, String> {
            let before = s.viewer_observation_v1(PlayerSeatV1::P0)?;
            let (obs, actions) = s
                .gameplay_trace_visible_v1(d, true)
                .map_err(|e| format!("{e:?}"))?;
            let labels = crate::human_bo3_v1::gameplay_trace_labels_v1(&obs, &actions)
                .map_err(|e| format!("{e:?}"))?;
            Ok(
                json!({"kind":"decision","decision":{"episode_id":d.episode_id,"step":d.step,"physical_decision_id":d.physical_decision_id,"substep_index":d.substep_index,"acting_player":d.acting_player},"before":before,"actions":actions,"labels":labels}),
            )
        })();
        match result {
            Ok(v) => self.before = Some(v),
            Err(e) => self.error = Some(e),
        }
    }
    pub(super) fn after(&mut self, s: &FastActorSessionV1, a: u32, evidence: Value) {
        if self.error.is_some() {
            return;
        }
        let Some(mut row) = self.before.take() else {
            self.error = Some("UCT gameplay frame missing".into());
            return;
        };
        match s.viewer_observation_v1(PlayerSeatV1::P0) {
            Ok(after) => row["after"] = json!(after),
            Err(e) => {
                self.error = Some(e);
                return;
            }
        }
        row["selected"] = json!(a);
        row["evidence"] = evidence;
        row["terminal"] = match s.current_response() {
            FastActorResponseV1::Terminal(t) => json!(t),
            _ => Value::Null,
        };
        self.write(row);
        self.count += 1;
    }
    pub(super) fn search(&mut self, index: usize, record: &Value, trace: Option<SearchCapture>) {
        if self.error.is_some() {
            return;
        }
        self.write(json!({"kind":"search","index":index,"record":record}));
        if let Some(c) = trace {
            if let Some(e) = c.error {
                self.error = Some(e);
                return;
            }
            for (id, board) in c.snapshots.into_iter().enumerate() {
                self.write(json!({"kind":"search_snapshot","search":index,"id":id,"board":board}));
            }
            for node in c.nodes {
                self.write(json!({"kind":"search_node","search":index,"node":node}));
            }
            for sim in c.simulations {
                self.write(json!({"kind":"search_simulation","search":index,"simulation":sim}));
            }
            self.write(json!({"kind":"search_end","search":index,"complete":true}));
        }
    }
    pub(super) fn finish(&mut self, complete: bool) {
        let footer = json!({"schema":SCHEMA,"kind":"footer","written":self.count,"complete":complete&&self.error.is_none(),"stop_reason":self.error});
        let _ = writeln!(self.file, "{footer}");
        let _ = self.file.flush();
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    path: std::path::PathBuf,
    sha256: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    spec: Input,
    focal: Input,
    opponent: Input,
    id: String,
    search_start: u64,
    output: std::path::PathBuf,
    primary: std::path::PathBuf,
    #[serde(default)]
    record: bool,
    max_steps: u64,
}
fn read(input: &Input) -> Result<Vec<u8>, String> {
    if !input.path.is_absolute() {
        return Err("Capture inputs must be absolute".into());
    }
    let bytes = std::fs::read(&input.path).map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != input.sha256 {
        return Err("Capture input pin differs".into());
    }
    Ok(bytes)
}
pub(crate) fn run(path: &Path) -> Result<(), String> {
    use super::{bank, play};
    let config: Config = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if !config.output.is_absolute()
        || !config.primary.is_absolute()
        || config.search_start >= config.max_steps
        || config.max_steps > 100000
    {
        return Err("Invalid bounded Spy capture configuration".into());
    }
    let spec: bank::BankSpec =
        serde_json::from_slice(&read(&config.spec)?).map_err(|e| e.to_string())?;
    spec.validate()?;
    let src = spec
        .sources
        .iter()
        .find(|s| s.id == config.id)
        .ok_or("Capture source missing")?
        .clone();
    if src.seat != 0 || src.opp != "T1" {
        return Err("Spy presentation expects the selected seat0/T1 setup".into());
    }
    let focal_config: Value =
        serde_json::from_slice(&read(&config.focal)?).map_err(|e| e.to_string())?;
    let opponent_config: Value =
        serde_json::from_slice(&read(&config.opponent)?).map_err(|e| e.to_string())?;
    let policy =
        super::super::load_policy_v1(config.focal.path.to_str().ok_or("Non-UTF8 focal path")?)?;
    let opp = super::super::load_policy_v1(
        config
            .opponent
            .path
            .to_str()
            .ok_or("Non-UTF8 opponent path")?,
    )?;
    let sh = bank::BankShared {
        spec,
        labels: vec!["T1".into()],
        opponents: vec![opp],
        prior: super::world::DeckPrior::new(
            &(0..crate::runtime_decks::RUNTIME_DECKS.len()).collect::<Vec<_>>(),
        ),
        limits: super::arms::Limits::FORMAL,
    };
    let runtime = crate::gameplay_trace_v1::runtime_identity()?;
    let cards = crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                i.to_string(),
                json!({"name":c.name,"type":format!("{:?}",c.types)}),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    let header = json!({"kind":"header","perspective":"p0","runtime":runtime,"cards":cards,"decks":[sh.spec.decks[src.own].label,sh.spec.decks[src.other].label],"focal_checkpoint":focal_config["checkpoint"]["sha256"],"opponent_checkpoint":opponent_config["checkpoint"]["sha256"],"search_start":config.search_start,"runtime_rules":sh.spec.runtime_rules,"capture_kind":"fresh-frozen-policy-game"});
    let mut recorder = if config.record {
        match Recorder::open(&config.output, header) {
            Ok(r) => Some(r),
            Err(e) => {
                eprintln!("Viewer recorder unavailable: {e}");
                None
            }
        }
    } else {
        None
    };
    let mut f = policy.fork_for_collection_v3()?;
    let mut o = sh.opponents[0].fork_for_collection_v3()?;
    f.reset_sampling_v1(src.source_policy_seeds);
    o.reset_sampling_v1(src.source_policy_seeds);
    let mut s = bank::source_session(&sh.spec, &src)?;
    let mut prefix = Vec::new();
    let mut c = play::Counters::default();
    let mut meter = play::Meter::new(u64::MAX);
    let defs = labels::spy_defs();
    while let Some(d) = play::decision(&s) {
        if d.step >= config.search_start {
            break;
        }
        if let Some(r) = recorder.as_mut() {
            r.before(&s, d);
        }
        let p = if play::acting(&d) == PlayerId::P0 {
            &mut f
        } else {
            &mut o
        };
        let a = bank::act_plain(p, &s, &d, &mut c)?;
        prefix.push(a);
        play::apply(&mut s, &d, a, &mut meter, PlayerId::P0, None, &defs).map_err(bank::err)?;
        if let Some(r) = recorder.as_mut() {
            r.after(&s, a, Value::Null);
        }
    }
    if play::decision(&s).is_none_or(|d| {
        d.step != config.search_start
            || play::acting(&d) != PlayerId::P0
            || d.legal_action_count < 2
    }) {
        if let Some(r) = recorder.as_mut() {
            r.finish(false);
        }
        return Err("Scheduled search decision is unavailable".into());
    }
    let mut roles = bank::bank_roles(&policy, &sh.opponents)?;
    let result = super::refresh::guide_viewer(
        &sh,
        &mut roles,
        &mut f,
        &mut o,
        &s,
        &src,
        recorder.as_mut(),
        config.max_steps,
    );
    let game = match result {
        Ok(v) => v,
        Err(e) => {
            if let Some(r) = recorder.as_mut() {
                r.error = Some(e.clone());
                r.finish(false);
            }
            return Err(e);
        }
    };
    let complete = game["valid"] == true;
    if let Some(r) = recorder.as_mut() {
        r.finish(complete);
    }
    let suffix: Vec<u32> =
        serde_json::from_value(game["actions"].clone()).map_err(|e| e.to_string())?;
    prefix.extend(suffix);
    let final_state = bank::walk(bank::source_session(&sh.spec, &src)?, &prefix, |_, _, _| {
        Ok(())
    })?;
    let searches=game["searches"].as_array().ok_or("Search records missing")?.iter().map(|r|json!({"index":r["search"],"step":r["step"],"budget":r["budget"],"transitions":r["transitions"],"completed":r["completed_natural"],"attempted":r["attempted"],"tree":r["tree"]})).collect::<Vec<_>>();
    if game["actual_core_hash"] != final_state.privileged_core_environment_hash() {
        return Err("Actual capture game and action replay core states differ".into());
    }
    let primary = json!({"actions":prefix,"searches":searches,"decisions":game["decisions"],"end":game["end"],"complete":complete,"core_hash":game["actual_core_hash"],"replay_core_hash":final_state.privileged_core_environment_hash()});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.primary)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{primary}").map_err(|e| e.to_string())?;
    eprintln!(
        "Spy capture: {} actions, complete={complete}, end={}",
        primary["actions"].as_array().unwrap().len(),
        game["end"]
    );
    Ok(())
}

//! DraftZero-shaped Limited match driver (issue #110 milestone 7). Its flags
//! and outputs follow gorge's `dzgorge coverage` and `dzgorge play`, so
//! DraftZero's engine wrapper can drive mtg-kernel the same way.
//!
//! ```text
//! kernel_limited_match_v1 version
//! kernel_limited_match_v1 coverage -decks DIR -pool FILE [-out FILE]
//! kernel_limited_match_v1 play -decks DIR -pool FILE -split S -seed N -pairs K -a SPEC -b SPEC -out FILE
//!     [-first-pair P] [-workers W] [-single] [-max-turns T] [-max-decisions D]
//!     [-belief-decks DIR] [-belief-pool FILE] [-belief-split S]
//! ```
//!
//! A deck directory holds `<deck>.json` files (`{name, cards: [{name, count}]}`);
//! a pool is a TSV whose header starts `deck split`. Pair `p` deals two
//! distinct playable decks of the split from `(seed, p)` and plays them twice
//! with the policies swapped (leg 0: policy A in seat 0), sharing the deal.
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::ids::PlayerId;
use mtg_kernel::limited_match_v1::{
    deal_pair_v1, play_game_v1, DeckFileV1, GameLimitsV1, GameOutcomeV1, LimitedBeliefV1,
    PolicySpecV1, LIMITED_MATCH_SCHEMA_V1,
};
use mtg_kernel::KERNEL_VERSION;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("version") => version(),
        Some("coverage") => Flags::parse(&args[1..]).and_then(|f| coverage(&f)),
        Some("play") => Flags::parse(&args[1..]).and_then(|f| play(&f)),
        _ => Err("usage: kernel_limited_match_v1 version | coverage ... | play ...".into()),
    };
    if let Err(error) = result {
        eprintln!("kernel_limited_match_v1: {error}");
        std::process::exit(2);
    }
}

struct Flags {
    values: HashMap<String, String>,
    switches: BTreeSet<String>,
}

impl Flags {
    const SWITCHES: [&'static str; 1] = ["single"];

    fn parse(args: &[String]) -> Result<Self, String> {
        let (mut values, mut switches) = (HashMap::new(), BTreeSet::new());
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            let name = arg
                .strip_prefix('-')
                .map(|n| n.trim_start_matches('-'))
                .ok_or_else(|| format!("unexpected argument {arg:?}"))?;
            if Self::SWITCHES.contains(&name) {
                switches.insert(name.to_string());
            } else {
                let value = it.next().ok_or_else(|| format!("-{name} needs a value"))?;
                if values.insert(name.to_string(), value.clone()).is_some() {
                    return Err(format!("-{name} given twice"));
                }
            }
        }
        Ok(Self { values, switches })
    }

    fn get(&self, name: &str) -> Result<&str, String> {
        self.values
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("missing -{name}"))
    }

    fn opt(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn num<T: std::str::FromStr>(&self, name: &str, default: Option<T>) -> Result<T, String> {
        match (self.opt(name), default) {
            (Some(v), _) => v.parse().map_err(|_| format!("-{name}: bad number {v:?}")),
            (None, Some(d)) => Ok(d),
            (None, None) => Err(format!("missing -{name}")),
        }
    }

    fn check_known(&self, known: &[&str]) -> Result<(), String> {
        match self.values.keys().find(|k| !known.contains(&k.as_str())) {
            Some(k) => Err(format!("unknown flag -{k}")),
            None => Ok(()),
        }
    }
}

fn version() -> Result<(), String> {
    println!(
        "{}",
        json!({
            "schema": LIMITED_MATCH_SCHEMA_V1,
            "engine": "mtg-kernel",
            "kernel_version": KERNEL_VERSION,
            "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        })
    );
    Ok(())
}

/// `(deck, split)` rows of a pool.tsv, in file order.
fn read_pool(path: &Path) -> Result<Vec<(String, String)>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    if header.len() < 2 || header[0] != "deck" || header[1] != "split" {
        return Err(format!(
            "{}: not a pool.tsv (header {:?})",
            path.display(),
            header
        ));
    }
    Ok(lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut cols = line.split('\t');
            let deck = cols.next().unwrap_or("").to_string();
            (deck, cols.next().unwrap_or("").to_string())
        })
        .collect())
}

fn read_deck(dir: &Path, name: &str) -> Result<DeckFileV1, String> {
    let path = dir.join(format!("{name}.json"));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Resolved Full-only decks of one split (None or "all": every row), in pool order.
fn playable(
    dir: &Path,
    pool: &Path,
    split: Option<&str>,
) -> Result<Vec<(String, Vec<u16>)>, String> {
    let mut out = Vec::new();
    for (deck, row_split) in read_pool(pool)? {
        if split.is_some_and(|s| s != "all" && s != row_split) {
            continue;
        }
        if let Ok(cards) = read_deck(dir, &deck).and_then(|d| d.resolve()) {
            out.push((deck, cards));
        }
    }
    Ok(out)
}

fn coverage(flags: &Flags) -> Result<(), String> {
    flags.check_known(&["decks", "pool", "out"])?;
    let dir = PathBuf::from(flags.get("decks")?);
    let rows = read_pool(Path::new(flags.get("pool")?))?;
    let mut cards = BTreeSet::new();
    let mut unsupported: BTreeMap<String, (usize, &'static str)> = BTreeMap::new();
    let (mut failed, mut supported) = (0usize, Vec::new());
    for (deck, _) in &rows {
        let file = match read_deck(&dir, deck) {
            Ok(file) => file,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let missing = file.unsupported();
        for row in &file.cards {
            cards.insert(row.name.clone());
        }
        let mut seen = BTreeSet::new();
        for (name, reason) in &missing {
            if seen.insert(name.clone()) {
                unsupported.entry(name.clone()).or_insert((0, *reason)).0 += 1;
            }
        }
        if missing.is_empty() {
            match file.resolve() {
                Ok(_) => supported.push(deck.clone()),
                Err(_) => failed += 1,
            }
        }
    }
    let mut by_decks: Vec<_> = unsupported.iter().collect();
    by_decks.sort_by(|a, b| b.1 .0.cmp(&a.1 .0).then(a.0.cmp(b.0)));
    for (name, (decks, reason)) in by_decks {
        println!("unsupported: {name} in {decks} decks missing [{reason}]");
    }
    println!(
        "{} distinct cards in {} decks; {} unsupported; {} decks failed to resolve; {} of {} decks play only supported cards",
        cards.len(),
        rows.len(),
        unsupported.len(),
        failed,
        supported.len(),
        rows.len()
    );
    if let Some(out) = flags.opt("out") {
        fs::write(out, supported.join("\n") + "\n").map_err(|e| format!("{out}: {e}"))?;
    }
    Ok(())
}

fn play(flags: &Flags) -> Result<(), String> {
    flags.check_known(&[
        "decks",
        "pool",
        "split",
        "seed",
        "pairs",
        "first-pair",
        "workers",
        "a",
        "b",
        "out",
        "max-turns",
        "max-decisions",
        "belief-decks",
        "belief-pool",
        "belief-split",
    ])?;
    let dir = PathBuf::from(flags.get("decks")?);
    let pool = PathBuf::from(flags.get("pool")?);
    let split = flags.opt("split").unwrap_or("train").to_string();
    let seed: u64 = flags.num("seed", None)?;
    let pairs: u64 = flags.num("pairs", None)?;
    let first_pair: u64 = flags.num("first-pair", Some(0))?;
    let workers: usize = flags.num::<usize>("workers", Some(1))?.max(1);
    let (spec_a, spec_b) = (flags.get("a")?.to_string(), flags.get("b")?.to_string());
    let policies = [PolicySpecV1::parse(&spec_a)?, PolicySpecV1::parse(&spec_b)?];
    let limits = GameLimitsV1 {
        max_turns: flags.num("max-turns", Some(100))?,
        max_decisions: flags.num("max-decisions", Some(20_000))?,
    };
    let out = PathBuf::from(flags.get("out")?);
    let legs: u64 = if spec_a == spec_b || flags.switches.contains("single") {
        1
    } else {
        2
    };

    let decks = playable(&dir, &pool, Some(&split))?;
    let mut belief_overlap = 0usize;
    let belief = if policies.iter().any(PolicySpecV1::needs_belief) {
        let belief_split = flags.opt("belief-split").unwrap_or("train");
        if belief_split == split && split != "train" {
            return Err(format!(
                "belief split {belief_split:?} is the games' own split: the belief would hold the opponents' real decks"
            ));
        }
        let belief_dir = flags.opt("belief-decks").map_or(dir.clone(), PathBuf::from);
        let belief_pool = flags.opt("belief-pool").map_or(pool.clone(), PathBuf::from);
        let corpus = playable(&belief_dir, &belief_pool, Some(belief_split))?;
        if corpus.is_empty() {
            return Err("the belief corpus has no playable deck".into());
        }
        // Belief decks that are also game decks (by name). Gorge allows this for
        // the train split only; anywhere else the belief would hold real decks.
        let names: BTreeSet<&str> = decks.iter().map(|(name, _)| name.as_str()).collect();
        belief_overlap = corpus
            .iter()
            .filter(|(name, _)| names.contains(name.as_str()))
            .count();
        if belief_overlap > 0 && split != "train" {
            return Err(format!(
                "{belief_overlap} belief decks are games' {split:?} decks: the belief would hold the opponents' real decks"
            ));
        }
        Some(LimitedBeliefV1::new(
            corpus.into_iter().map(|(_, cards)| cards),
        ))
    } else {
        None
    };

    let jobs: Vec<(u64, u64)> = (first_pair..first_pair + pairs)
        .flat_map(|p| (0..legs).map(move |leg| (p, leg)))
        .collect();
    let mut file = fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let next_job = AtomicUsize::new(0);
    // Rows are written in game order as soon as the prefix is complete.
    let pending: Mutex<(usize, BTreeMap<usize, String>, &mut fs::File)> =
        Mutex::new((0, BTreeMap::new(), &mut file));
    let tallies = Mutex::new(BTreeMap::<&'static str, u64>::new());
    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..workers.min(jobs.len().max(1)) {
            scope.spawn(|| loop {
                let job = next_job.fetch_add(1, Ordering::Relaxed);
                let Some(&(pair, leg)) = jobs.get(job) else {
                    break;
                };
                let row = play_one(
                    &decks,
                    &policies,
                    [&spec_a, &spec_b],
                    belief.as_ref(),
                    seed,
                    pair,
                    leg,
                    legs,
                    first_pair,
                    limits,
                );
                let result = row["result"].as_str().unwrap_or("error");
                *tallies
                    .lock()
                    .unwrap()
                    .entry(match result {
                        "A" => "a_wins",
                        "B" => "b_wins",
                        "draw" => "draws",
                        "stall" => "stalls",
                        _ => "errors",
                    })
                    .or_default() += 1;
                let mut guard = pending.lock().unwrap();
                let (next, ready, file) = &mut *guard;
                ready.insert(job, game_first(row));
                while let Some(line) = ready.remove(&*next) {
                    let _ = writeln!(file, "{line}");
                    *next += 1;
                }
                let _ = file.flush();
            });
        }
    });
    let wall_s = started.elapsed().as_secs_f64();
    let tallies = tallies.into_inner().unwrap();
    let count = |k: &str| tallies.get(k).copied().unwrap_or(0);
    let mut hash = Sha256::new();
    for (name, _) in &decks {
        hash.update(name.as_bytes());
        hash.update(b"\n");
    }
    let playable_sha256 = format!("{:x}", hash.finalize());
    let summary = json!({
        "schema": LIMITED_MATCH_SCHEMA_V1,
        "engine": "mtg-kernel",
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "a": spec_a, "b": spec_b, "split": split, "seed": seed,
        "first_pair": first_pair, "pairs": pairs, "legs": legs,
        "playable_decks": decks.len(),
        // Pair p deals from this list, which follows the engine's coverage.
        "playable_decks_sha256": playable_sha256,
        "belief_overlap": belief_overlap,
        "belief_decks": belief.as_ref().map(LimitedBeliefV1::len),
        "games": jobs.len(),
        "a_wins": count("a_wins"), "b_wins": count("b_wins"), "draws": count("draws"),
        "stalls": count("stalls"), "errors": count("errors"),
        "wall_s": wall_s,
        "games_per_hour": if wall_s > 0.0 { jobs.len() as f64 / wall_s * 3600.0 } else { 0.0 },
        "workers": workers,
    });
    let summary_path = format!("{}.summary.json", out.display());
    fs::write(
        &summary_path,
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .map_err(|e| format!("{summary_path}: {e}"))?;
    Ok(())
}

/// One games.jsonl line. DraftZero renumbers rows by rewriting a leading
/// `{"game":N`, so `game` comes first; the other keys follow sorted.
fn game_first(mut row: serde_json::Value) -> String {
    let map = row.as_object_mut().expect("game rows are objects");
    let game = map.remove("game").unwrap_or(json!(-1));
    let rest = row.to_string();
    match rest.strip_prefix('{').filter(|r| *r != "}") {
        Some(rest) => format!("{{\"game\":{game},{rest}"),
        None => format!("{{\"game\":{game}}}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn play_one(
    decks: &[(String, Vec<u16>)],
    policies: &[PolicySpecV1; 2],
    specs: [&String; 2],
    belief: Option<&LimitedBeliefV1>,
    seed: u64,
    pair: u64,
    leg: u64,
    legs: u64,
    first_pair: u64,
    limits: GameLimitsV1,
) -> serde_json::Value {
    let game = (pair - first_pair) * legs + leg;
    let base = |extra: serde_json::Value| {
        let mut row = json!({"game": game, "pair": pair, "leg": leg, "seed": seed});
        row.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        row
    };
    let (indices, env_seed, starting) = match deal_pair_v1(seed, pair, decks.len()) {
        Ok(deal) => deal,
        Err(error) => return base(json!({"result": "error", "error": error, "winner": -1})),
    };
    // Leg 0 seats policy A in seat 0; leg 1 swaps the policies, not the decks.
    let side = if leg == 0 { ["A", "B"] } else { ["B", "A"] };
    let pick = |s: &str| if s == "A" { 0 } else { 1 };
    let seat_policies = [&policies[pick(side[0])], &policies[pick(side[1])]];
    let seat_beliefs = seat_policies.map(|p| if p.needs_belief() { belief } else { None });
    let started = Instant::now();
    let result = play_game_v1(
        [
            decks[indices[0]].1.as_slice(),
            decks[indices[1]].1.as_slice(),
        ],
        seat_policies,
        seat_beliefs,
        env_seed,
        starting,
        limits,
    );
    let (winner, outcome, error) = match &result.outcome {
        GameOutcomeV1::Winner(p) => (p.index() as i64, side[p.index()], None),
        GameOutcomeV1::Draw => (-1, "draw", None),
        GameOutcomeV1::Stall => (-1, "stall", None),
        GameOutcomeV1::Error(e) => (-1, "error", Some(e.clone())),
    };
    let mut row = base(json!({
        "decks": [decks[indices[0]].0, decks[indices[1]].0],
        "side": side,
        "pols": [specs[pick(side[0])], specs[pick(side[1])]],
        "starting": if starting == PlayerId::P0 { 0 } else { 1 },
        "winner": winner,
        "result": outcome,
        "turns": result.turns,
        "decisions": result.decisions,
        "rejected_worlds": result.rejected_worlds,
        "rollout_errors": result.rollout_errors,
        "ms": started.elapsed().as_millis() as u64,
    }));
    if let Some(error) = error {
        row["error"] = json!(error);
    }
    if let Some(error) = &result.first_rollout_error {
        row["first_rollout_error"] = json!(error);
    }
    row
}

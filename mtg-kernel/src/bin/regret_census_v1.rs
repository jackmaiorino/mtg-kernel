use mtg_kernel::regret_census_v1::{run_v1, CensusConfigV1};

fn main() {
    let mut cfg = CensusConfigV1 {
        source: String::new(),
        out: String::new(),
        first_game: 0,
        games: 1,
        base_seed: 2026100611,
        root_prob: 0.0,
        rollouts: 8,
        max_actions: 8,
        workers: 1,
        decks: (0..9).collect(),
        mode: "decisions".into(),
        pilot_deck: 0,
    };
    let mut max_actions_set = false;
    for arg in std::env::args().skip(1) {
        let (k, v) = arg.split_once('=').expect("key=value arguments");
        match k {
            "source" => cfg.source = v.into(),
            "out" => cfg.out = v.into(),
            "first_game" => cfg.first_game = v.parse().unwrap(),
            "games" => cfg.games = v.parse().unwrap(),
            "base_seed" => cfg.base_seed = v.parse().unwrap(),
            "root_prob" => cfg.root_prob = v.parse().unwrap(),
            "rollouts" => cfg.rollouts = v.parse().unwrap(),
            "max_actions" => {
                cfg.max_actions = v.parse().unwrap();
                max_actions_set = true;
            }
            "workers" => cfg.workers = v.parse().unwrap(),
            "mode" => cfg.mode = v.into(),
            "pilot_deck" => cfg.pilot_deck = v.parse().unwrap(),
            "decks" => cfg.decks = v.split(',').map(|x| x.parse().unwrap()).collect(),
            other => panic!("unknown argument {other}"),
        }
    }
    // The search modes (roots, cond, cross) default to K = 4 root candidates.
    if !max_actions_set && matches!(cfg.mode.as_str(), "roots" | "cond" | "cross") {
        cfg.max_actions = 4;
    }
    if let Err(e) = run_v1(cfg) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

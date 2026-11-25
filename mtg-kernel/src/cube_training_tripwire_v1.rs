//! Signed A2.5 training tripwire over the completed census-b summary.

use super::{
    invalid, TrainingDriverError, TripwireFiring, TripwireRule, TRIPWIRE_TAIL_MERGE_GAMES,
    TRIPWIRE_WINDOW_GAMES,
};
use crate::cube_b_census_v1::stop_kind;
use crate::cube_draft_v1::GameEnd;
use crate::cube_pod_runner_v1::{is_sha256_hex, PodPins, PodRecord};
use crate::cube_read_plan_v1::sha256_hex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

const MIN_GAMES: u64 = 5;

#[derive(Debug, Clone, Copy)]
struct Rate {
    games: u64,
    total: u64,
}

impl Rate {
    fn exceeded_by(self, games: u64, total: u64, stop_kind_margin: bool) -> bool {
        if total == 0 {
            return false;
        }
        // A window has at most 2499 games. Integers preserve the strict comparison at equality.
        let observed = u128::from(games) * u128::from(self.total);
        let baseline = u128::from(self.games) * u128::from(total);
        observed > 2 * baseline
            && (!stop_kind_margin
                || observed * 1000 > baseline * 1000 + u128::from(self.total) * u128::from(total))
    }

    fn threshold_bits(self, stop_kind_margin: bool) -> u64 {
        let rate = self.games as f64 / self.total as f64;
        if stop_kind_margin {
            (2.0 * rate).max(rate + 0.001).to_bits()
        } else {
            (2.0 * rate).to_bits()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CensusBaselinePin {
    pub run: String,
    pub summary_sha256: String,
    pub manifest_sha256: String,
    pub build_sha256: String,
    pub fence_sha256: String,
    pub authorization_sha256: String,
}

impl CensusBaselinePin {
    pub(super) fn matches(&self, pins: &PodPins) -> bool {
        pins.binary.as_deref() == Some(&self.build_sha256)
            && pins.fence_record.as_deref() == Some(&self.fence_sha256)
    }

    pub(super) fn load(&self, path: &Path) -> Result<CensusRates, TrainingDriverError> {
        let valid_run = self.run == "census-b"
            || self
                .run
                .strip_prefix("census-b-r")
                .is_some_and(|n| n.parse::<u64>().is_ok_and(|n| n > 0));
        if !valid_run
            || [
                &self.summary_sha256,
                &self.manifest_sha256,
                &self.build_sha256,
                &self.fence_sha256,
                &self.authorization_sha256,
            ]
            .into_iter()
            .any(|pin| !is_sha256_hex(pin))
        {
            return Err(invalid("invalid census-b baseline pins"));
        }
        let bytes = fs::read(path)?;
        if sha256_hex(&bytes) != self.summary_sha256 {
            return Err(invalid("census-b summary digest differs from its pin"));
        }
        CensusRates::from_summary(serde_json::from_slice(&bytes)?)
    }
}

#[derive(Deserialize)]
struct CensusSummary {
    games: u64,
    completed: u64,
    halted: u64,
    order_errors: Vec<Value>,
    stop_kind_mismatches: Vec<Value>,
    stop_kinds: Vec<CensusStop>,
    cards: Vec<CensusCard>,
    #[serde(default)]
    tripwire: Option<CensusEligible>,
}

#[derive(Deserialize)]
struct CensusStop {
    stop_kind: String,
    games: u64,
}

#[derive(Deserialize)]
struct CensusCard {
    card: u16,
    games_present: u64,
    charged: u64,
}

#[derive(Deserialize)]
struct CensusEligible {
    games: u64,
    cards: Vec<CensusCard>,
}

#[derive(Debug)]
pub(super) struct CensusRates {
    stop_kinds: BTreeMap<String, Rate>,
    cards: BTreeMap<u16, Rate>,
}

fn excluded_slot(kind: &str) -> bool {
    kind.starts_with("process dies:") || matches!(kind, "lost" | "unrun")
}

impl CensusRates {
    fn from_summary(summary: CensusSummary) -> Result<Self, TrainingDriverError> {
        if summary.completed.checked_add(summary.halted) != Some(summary.games)
            || !summary.order_errors.is_empty()
            || !summary.stop_kind_mismatches.is_empty()
        {
            return Err(invalid(
                "census-b summary contains inconsistent games or checks",
            ));
        }
        let mut all_stops = BTreeMap::new();
        let mut halted = 0u64;
        let mut excluded = 0u64;
        for row in summary.stop_kinds {
            if row.stop_kind.is_empty()
                || row.games == 0
                || all_stops.insert(row.stop_kind.clone(), row.games).is_some()
            {
                return Err(invalid("invalid or duplicate census-b stop kind"));
            }
            halted = halted
                .checked_add(row.games)
                .ok_or_else(|| invalid("census-b halt count overflow"))?;
            if excluded_slot(&row.stop_kind) {
                excluded = excluded
                    .checked_add(row.games)
                    .ok_or_else(|| invalid("census-b excluded count overflow"))?;
            }
        }
        if halted != summary.halted || excluded >= summary.games {
            return Err(invalid(
                "census-b stop total or eligible denominator is invalid",
            ));
        }
        let total = summary.games - excluded;
        let card_rows = match summary.tripwire {
            Some(rates) if rates.games == total => rates.cards,
            None if excluded == 0 => summary.cards,
            _ => return Err(invalid("census-b summary lacks eligible card denominators")),
        };
        let mut cards = BTreeMap::new();
        for row in card_rows {
            // Count-only zero baselines also cover cards never present in the census.
            if row.games_present == 0
                || row.games_present > total
                || row.charged > total
                || cards
                    .insert(
                        row.card,
                        Rate {
                            games: row.charged,
                            total: row.games_present,
                        },
                    )
                    .is_some()
            {
                return Err(invalid("invalid or duplicate census-b card rate"));
            }
        }
        let stop_kinds = all_stops
            .into_iter()
            .filter(|(kind, _)| !excluded_slot(kind))
            .map(|(kind, games)| (kind, Rate { games, total }))
            .collect();
        Ok(Self { stop_kinds, cards })
    }
}

#[derive(Debug, Clone, Default)]
struct Window {
    games: u64,
    stop_games: BTreeMap<String, u64>,
    card_games: BTreeMap<u16, u64>,
    card_denominator: BTreeMap<u16, u64>,
}

#[derive(Debug, Clone)]
pub(super) struct TripwireTracker {
    window_sizes: Vec<u64>,
    closed_games: u64,
    closed_windows: usize,
    open: Window,
    pub(super) firing: Option<TripwireFiring>,
}

pub(super) fn tripwire_window_sizes(games: u64) -> Vec<u64> {
    let windows = games / TRIPWIRE_WINDOW_GAMES;
    let tail = games % TRIPWIRE_WINDOW_GAMES;
    let mut sizes = vec![TRIPWIRE_WINDOW_GAMES; windows as usize];
    if tail > 0 {
        match sizes.last_mut() {
            Some(last) if tail < TRIPWIRE_TAIL_MERGE_GAMES => *last += tail,
            _ => sizes.push(tail),
        }
    }
    sizes
}

#[derive(Deserialize)]
struct ReplayAttempt {
    end: GameEnd,
    halted: bool,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    last_action_source: Option<u16>,
}

impl TripwireTracker {
    pub(super) fn new(window_sizes: Vec<u64>) -> Self {
        Self {
            window_sizes,
            closed_games: 0,
            closed_windows: 0,
            open: Window::default(),
            firing: None,
        }
    }

    pub(super) fn games(&self) -> u64 {
        self.closed_games + self.open.games
    }

    fn record(&self, rule: TripwireRule, games: u64, denominator: u64) -> TripwireFiring {
        TripwireFiring {
            window: Some(self.closed_windows as u64),
            rule,
            games,
            denominator,
            observed_rate_bits: (denominator != 0)
                .then(|| (games as f64 / denominator as f64).to_bits()),
        }
    }

    fn firing(&self, baseline: &CensusRates, closed: bool) -> Option<TripwireFiring> {
        for (kind, &games) in &self.open.stop_games {
            let total = self.open.games;
            let threshold = baseline.stop_kinds.get(kind);
            if games >= MIN_GAMES
                && match threshold {
                    None => true,
                    Some(rate) => closed && rate.exceeded_by(games, total, true),
                }
            {
                return Some(self.record(
                    TripwireRule::StopKind {
                        stop_kind: kind.clone(),
                        min_games: MIN_GAMES,
                        rate_threshold_bits: threshold.map(|rate| rate.threshold_bits(true)),
                    },
                    games,
                    total,
                ));
            }
        }
        for (&card, &games) in &self.open.card_games {
            let total = self.open.card_denominator.get(&card).copied().unwrap_or(0);
            let threshold = baseline.cards.get(&card).filter(|rate| rate.games != 0);
            if games >= MIN_GAMES
                && match threshold {
                    None => true,
                    Some(rate) => closed && rate.exceeded_by(games, total, false),
                }
            {
                return Some(self.record(
                    TripwireRule::Card {
                        card,
                        min_games: MIN_GAMES,
                        rate_threshold_bits: threshold.map(|rate| rate.threshold_bits(false)),
                    },
                    games,
                    total,
                ));
            }
        }
        None
    }

    fn add_game<'a>(
        &mut self,
        cards: impl IntoIterator<Item = u16>,
        halted: impl IntoIterator<Item = (&'a GameEnd, Option<u16>)>,
        baseline: Option<&CensusRates>,
    ) {
        self.open.games += 1;
        let closed = self.window_sizes.get(self.closed_windows) == Some(&self.open.games);
        if let Some(baseline) = baseline {
            let mut kinds = BTreeSet::new();
            let mut sources = BTreeSet::new();
            for (end, source) in halted {
                if let Some(kind) = stop_kind(end) {
                    kinds.insert(kind);
                    sources.extend(source);
                }
            }
            for kind in kinds {
                *self.open.stop_games.entry(kind).or_default() += 1;
            }
            for card in sources {
                *self.open.card_games.entry(card).or_default() += 1;
            }
            for card in cards.into_iter().collect::<BTreeSet<_>>() {
                *self.open.card_denominator.entry(card).or_default() += 1;
            }
            if self.firing.is_none() {
                self.firing = self.firing(baseline, closed);
            }
        }
        if closed {
            self.closed_windows += 1;
            self.closed_games += std::mem::take(&mut self.open).games;
        }
    }

    pub(super) fn add_pod(&mut self, pod: &PodRecord, baseline: Option<&CensusRates>) {
        for game in &pod.games {
            let cards = game
                .decks
                .iter()
                .flat_map(|&id| pod.decks[id as usize].cards.iter().copied());
            let halts = game
                .attempts
                .iter()
                .filter(|attempt| attempt.halted)
                .map(|attempt| (&attempt.end, attempt.last_action_source));
            self.add_game(cards, halts, baseline);
        }
    }

    pub(super) fn add_pod_json(
        &mut self,
        pod: Value,
        baseline: Option<&CensusRates>,
    ) -> Result<(), TrainingDriverError> {
        #[derive(Deserialize)]
        struct Deck {
            cards: Vec<u16>,
        }
        #[derive(Deserialize)]
        struct Game {
            decks: [usize; 2],
            attempts: Vec<ReplayAttempt>,
        }
        #[derive(Deserialize)]
        struct ReplayPod {
            schema: String,
            decks: Vec<Deck>,
            games: Vec<Game>,
        }

        let pod: ReplayPod = serde_json::from_value(pod)?;
        if pod.schema != crate::cube_pod_runner_v1::POD_RECORD_SCHEMA
            || pod.games.len() != crate::cube_pod_runner_v1::TRAINING_POD_GAMES
        {
            return Err(invalid(
                "published pod schema or training game count differs",
            ));
        }
        for game in pod.games {
            let [a, b] = game.decks;
            let decks = [pod.decks.get(a), pod.decks.get(b)];
            let [Some(a), Some(b)] = decks else {
                return Err(invalid("game names a missing deck"));
            };
            if game.attempts.is_empty()
                || game
                    .attempts
                    .iter()
                    .any(|attempt| attempt.halted == matches!(attempt.end, GameEnd::Natural { .. }))
            {
                return Err(invalid("game has missing or inconsistent attempt records"));
            }
            self.add_game(
                a.cards.iter().chain(&b.cards).copied(),
                game.attempts
                    .iter()
                    .filter(|attempt| attempt.halted)
                    .map(|attempt| (&attempt.end, attempt.last_action_source)),
                baseline,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn baseline() -> CensusRates {
        CensusRates::from_summary(
            serde_json::from_value(json!({
                "games": 2000, "completed": 1794, "halted": 206,
                "order_errors": [], "stop_kind_mismatches": [],
                "stop_kinds": [
                    {"stop_kind": "known", "games": 5},
                    {"stop_kind": "rare", "games": 1},
                    {"stop_kind": "driver_decision_cap", "games": 200}
                ],
                "cards": [
                    {"card": 1, "games_present": 200, "charged": 10},
                    {"card": 2, "games_present": 100, "charged": 0}
                ]
            }))
            .unwrap(),
        )
        .unwrap()
    }

    fn terminal(reason: &str) -> GameEnd {
        GameEnd::Terminal {
            code: "halt".into(),
            classification: "engine_halted".into(),
            reason: reason.into(),
        }
    }

    fn add(
        tracker: &mut TripwireTracker,
        baseline: Option<&CensusRates>,
        games: u64,
        halt_games: u64,
        end: &GameEnd,
        source: Option<u16>,
        cards: &[u16],
    ) {
        for i in 0..games {
            // Repeat a halted attempt in the same game to exercise per-game deduplication.
            let attempts = (i < halt_games).then_some([(end, source); 2]);
            tracker.add_game(
                cards.iter().copied(),
                attempts.into_iter().flatten(),
                baseline,
            );
        }
    }

    #[test]
    fn known_stop_requires_five_and_strictly_exceeds_both_thresholds_at_close() {
        let rates = baseline();
        for (size, halts, expected) in [(100, 4, false), (2000, 10, false), (2000, 11, true)] {
            let mut tracker = TripwireTracker::new(vec![size]);
            add(
                &mut tracker,
                Some(&rates),
                size - 1,
                halts,
                &terminal("known"),
                None,
                &[],
            );
            assert!(tracker.firing.is_none());
            add(
                &mut tracker,
                Some(&rates),
                1,
                0,
                &terminal("known"),
                None,
                &[],
            );
            assert_eq!(tracker.firing.is_some(), expected);
        }
        let rare = rates.stop_kinds["rare"];
        assert!(!rare.exceeded_by(3, 2000, true)); // Exactly 0.05% + 0.1 percentage points.
        assert!(rare.exceeded_by(4, 2000, true));
    }

    #[test]
    fn unseen_stop_counts_games_once_and_normalizes_sources() {
        let rates = baseline();
        let mut tracker = TripwireTracker::new(vec![2000]);
        add(
            &mut tracker,
            Some(&rates),
            4,
            4,
            &terminal("new:source:12"),
            None,
            &[],
        );
        assert!(tracker.firing.is_none());
        add(
            &mut tracker,
            Some(&rates),
            1,
            1,
            &terminal("new:source:27"),
            None,
            &[],
        );
        let firing = tracker.firing.unwrap();
        assert_eq!(firing.games, 5);
        assert_eq!(
            firing.rule,
            TripwireRule::StopKind {
                stop_kind: "new:source:*".into(),
                min_games: 5,
                rate_threshold_bits: None,
            }
        );
    }

    #[test]
    fn card_rate_uses_games_present_and_needs_five_charges() {
        let rates = baseline();
        for (halts, expected) in [(4, false), (5, false), (6, true)] {
            // 50 games have the card, 100 games count in the stop-kind denominator.
            let mut tracker = TripwireTracker::new(vec![100]);
            add(
                &mut tracker,
                Some(&rates),
                50,
                halts,
                &GameEnd::DecisionCap,
                Some(1),
                &[1, 1],
            );
            assert!(tracker.firing.is_none());
            add(
                &mut tracker,
                Some(&rates),
                50,
                0,
                &GameEnd::DecisionCap,
                None,
                &[],
            );
            assert_eq!(tracker.firing.is_some(), expected);
            if let Some(firing) = tracker.firing {
                assert!(matches!(firing.rule, TripwireRule::Card { card: 1, .. }));
                assert_eq!((firing.games, firing.denominator), (6, 50));
            }
        }
        let mut tracker = TripwireTracker::new(vec![100]);
        add(
            &mut tracker,
            Some(&rates),
            20,
            4,
            &GameEnd::DecisionCap,
            Some(1),
            &[1],
        );
        add(
            &mut tracker,
            Some(&rates),
            80,
            0,
            &GameEnd::DecisionCap,
            None,
            &[],
        );
        assert!(tracker.firing.is_none()); // A rate above the threshold still needs five charges.
    }

    #[test]
    fn zero_and_absent_card_baselines_fire_at_five_with_any_rate() {
        let rates = baseline();
        for card in [2, 3] {
            let mut tracker = TripwireTracker::new(vec![2000]);
            add(
                &mut tracker,
                Some(&rates),
                4,
                4,
                &GameEnd::DecisionCap,
                Some(card),
                &[],
            );
            assert!(tracker.firing.is_none());
            add(
                &mut tracker,
                Some(&rates),
                1,
                1,
                &GameEnd::DecisionCap,
                None,
                &[],
            );
            assert!(tracker.firing.is_none());
            add(
                &mut tracker,
                Some(&rates),
                1,
                1,
                &GameEnd::DecisionCap,
                Some(card),
                &[],
            );
            let firing = tracker.firing.unwrap();
            assert_eq!(firing.games, 5);
            assert_eq!(firing.denominator, 0);
            assert_eq!(
                firing.rule,
                TripwireRule::Card {
                    card,
                    min_games: 5,
                    rate_threshold_bits: None
                }
            );
        }
    }

    #[test]
    fn windows_are_disjoint_and_only_tails_under_500_merge() {
        let rates = baseline();
        let end = terminal("unseen");
        for (total, sizes, fires) in [(2499, vec![2499], true), (2500, vec![2000, 500], false)] {
            assert_eq!(tripwire_window_sizes(total), sizes);
            let mut tracker = TripwireTracker::new(sizes);
            add(&mut tracker, Some(&rates), 1996, 0, &end, None, &[]);
            add(&mut tracker, Some(&rates), 5, 5, &end, None, &[]);
            assert_eq!(tracker.firing.is_some(), fires);
        }
        assert_eq!(tripwire_window_sizes(4000), [2000, 2000]);
        assert_eq!(tripwire_window_sizes(1), [1]);
    }

    #[test]
    fn q9_ignores_the_ordinary_halt_and_card_rates() {
        let mut tracker = TripwireTracker::new(vec![2000]);
        add(
            &mut tracker,
            None,
            2000,
            100,
            &terminal("unseen"),
            Some(2),
            &[2],
        );
        assert_eq!(tracker.games(), 2000);
        assert!(tracker.firing.is_none());
    }

    #[test]
    fn summary_uses_eligible_rates_and_checks_denominators() {
        let mut summary = json!({
            "games": 105, "completed": 98, "halted": 7,
            "order_errors": [], "stop_kind_mismatches": [],
            "stop_kinds": [
                {"stop_kind": "hang", "games": 1},
                {"stop_kind": "ordinary", "games": 1},
                {"stop_kind": "process dies: exit 1", "games": 1},
                {"stop_kind": "lost", "games": 2},
                {"stop_kind": "unrun", "games": 2}
            ],
            "cards": [{"card": 1, "games_present": 60, "charged": 1}],
        });
        let parse = |value| CensusRates::from_summary(serde_json::from_value(value).unwrap());
        assert!(parse(summary.clone()).is_err());
        summary["tripwire"] =
            json!({"games":100,"cards":[{"card":1,"games_present":55,"charged":1}]});
        let rates = parse(summary.clone()).unwrap();
        assert_eq!(rates.stop_kinds["ordinary"].total, 100);
        assert_eq!(rates.stop_kinds["hang"].total, 100);
        assert_eq!(rates.stop_kinds.len(), 2);
        assert_eq!(rates.cards[&1].total, 55);
        summary["tripwire"]["games"] = json!(105);
        assert!(parse(summary).is_err());
    }
    #[test]
    fn rebuild_refuses_pods_without_game_data_even_in_q9() {
        let mut tracker = TripwireTracker::new(vec![2000]);
        let schema = crate::cube_pod_runner_v1::POD_RECORD_SCHEMA;
        assert!(tracker
            .add_pod_json(json!({"schema":schema,"decks":[]}), None)
            .is_err());
        let game =
            json!({"decks":[0,1],"attempts":[{"end":{"kind":"decision_cap"},"halted":true}]});
        let mut pod = json!({
            "schema":schema, "decks":[{"cards":[1]},{"cards":[2]}],
            "games": vec![game; crate::cube_pod_runner_v1::TRAINING_POD_GAMES]
        });
        assert!(tracker.add_pod_json(pod.clone(), None).is_err());
        for game in pod["games"].as_array_mut().unwrap() {
            game["attempts"][0]["last_action_source"] = Value::Null;
        }
        tracker.add_pod_json(pod, None).unwrap();
        assert_eq!(
            tracker.games(),
            crate::cube_pod_runner_v1::TRAINING_POD_GAMES as u64
        );
    }
}

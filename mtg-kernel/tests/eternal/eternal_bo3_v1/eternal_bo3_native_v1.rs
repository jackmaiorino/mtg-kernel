//! In-process games under the native opening profile, which the JSONL
//! custom-deck session refuses (wave S1i). They take the session games'
//! seeded random actions: London mulligans and every later decision choose
//! among the policy surface's legal actions, the menu the JSONL session
//! offers, and a random seat starts no new spell or ability while it
//! controls two stack items. Each kept-hand choice takes a random legal
//! opening card (a Leyline enters, a Chancellor of the Annex is revealed) or
//! finishes, and a trigger order too large for the action list takes the
//! listed order. The session games' Ward filters exist for policy
//! observation v5, which these games don't build.

use crate::eternal_catalog_v1::splitmix64;
use mtg_kernel::card_def::CARD_DEFS;
use mtg_kernel::engine::{Action, Decision};
use mtg_kernel::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::{policy_legal_action_candidates_v5, ActionSemanticV1, PlayerSeatV1};
use mtg_kernel::state::{GameState, Target, Zone};
use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_STEPS: usize = 131_072;

/// The most pending triggers one listed order action covers.
const MAX_LISTED_TRIGGERS: usize = 7;

/// One native game, played to its natural terminal.
pub(super) struct NativeGame {
    /// The chosen actions, then the final state hash.
    pub(super) transcript_sha256: String,
    /// The winning seat, or `None` for a drawn game.
    pub(super) winner: Option<usize>,
    /// Spells cast per seat, by card definition id.
    pub(super) cast_cards: [BTreeMap<u16, usize>; 2],
    /// Cards put onto the battlefield from an opening hand, by definition id.
    pub(super) opening_entries: BTreeMap<u16, usize>,
}

/// Plays `state`, a game started with
/// `EternalBestOfThreeMatchV1::start_game_with_native_opening_v1`, to its
/// natural terminal with seeded random actions.
pub(super) fn play(mut state: GameState, seed: u64) -> NativeGame {
    let mut surface = PolicySurfaceV5::new();
    let mut random = seed;
    let mut transcript = Sha256::new();
    let mut cast_cards: [BTreeMap<u16, usize>; 2] = Default::default();
    let mut opening_entries = BTreeMap::new();
    // A game that loops without ending fails here instead of running until
    // the CI job times out.
    let started = std::time::Instant::now();
    for step in 0..MAX_STEPS {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(600),
            "seed={seed} step={step}: native game still running after 600 s"
        );
        let decision = surface
            .next_decision(&mut state)
            .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
        if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) =
            &decision
        {
            transcript.update(state.state_hash().to_le_bytes());
            return NativeGame {
                transcript_sha256: format!("{:x}", transcript.finalize()),
                winner: winner.map(|player| player.index()),
                cast_cards,
                opening_entries,
            };
        }
        assert!(
            !decision.is_terminal(),
            "seed={seed} step={step}: {decision:?}"
        );
        let mut opened = None;
        let action = match &decision {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseEternalOpeningV1 { legal_cards, .. },
            )) => {
                let pick = splitmix64(&mut random) % (legal_cards.len() as u64 + 1);
                opened = legal_cards.get(pick as usize).copied();
                PolicyActionV5::Surface(SurfaceAction::Action(
                    opened.map_or(Action::FinishEffectSelection, |card| {
                        Action::ChooseEffectTarget(Target::Object(card))
                    }),
                ))
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                pending,
                ..
            })) if pending.len() > MAX_LISTED_TRIGGERS => PolicyActionV5::Surface(
                SurfaceAction::Action(Action::OrderTriggers((0..pending.len()).collect())),
            ),
            _ => {
                let candidates = policy_legal_action_candidates_v5(&decision, &surface, &state)
                    .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
                assert!(
                    !candidates.is_empty(),
                    "seed={seed} step={step}: no legal action at {decision:?}"
                );
                let actor = decision
                    .actor(&state)
                    .expect("a nonterminal decision has an actor");
                let busy = state
                    .stack
                    .iter()
                    .filter(|item| item.controller == actor)
                    .count()
                    >= 2;
                let open: Vec<_> = candidates
                    .iter()
                    .filter(|candidate| {
                        !busy
                            || !matches!(
                                candidate.record.semantic,
                                ActionSemanticV1::CastSpell { .. }
                                    | ActionSemanticV1::ActivateAbility { .. }
                            )
                    })
                    .collect();
                let menu = if open.is_empty() {
                    candidates.iter().collect()
                } else {
                    open
                };
                let chosen = menu[(splitmix64(&mut random) % menu.len() as u64) as usize];
                if let ActionSemanticV1::CastSpell { actor, source } = &chosen.record.semantic {
                    let seat = usize::from(matches!(actor, PlayerSeatV1::P1));
                    *cast_cards[seat].entry(source.card_db_id).or_default() += 1;
                }
                transcript.update(chosen.record.stable_id.as_bytes());
                chosen.policy_action.clone()
            }
        };
        transcript.update(format!("{action:?}\n").as_bytes());
        surface
            .apply(&mut state, action)
            .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
        if let Some(card) = opened {
            let card = state.objects.get(card);
            if card.zone == Zone::Battlefield {
                *opening_entries.entry(card.card_def).or_default() += 1;
            } else {
                // Chancellor of the Annex is revealed and stays in its
                // owner's hand; every other opening card enters.
                assert_eq!(
                    (card.zone, CARD_DEFS[usize::from(card.card_def)].name),
                    (Zone::Hand, "Chancellor of the Annex"),
                    "seed={seed} step={step}"
                );
            }
        }
    }
    panic!("seed={seed}: no natural terminal in {MAX_STEPS} steps");
}

//! Public facts for the explicit native Premodern ordered-graveyard profile.
//!
//! This extension exposes the order prompt and full-text provenance. It does
//! not extend legacy observations or authorize hidden-world sampling.

pub use crate::graveyard_order_v1::BatchV1;
use crate::{
    card_def::{CardType, Keywords, CARD_DEFS, KERNEL_CARDDB_HASH},
    continuous_characteristics_v1, engine, full_text_v1, graveyard_order_v1,
    ids::{ObjectId, PlayerId},
    rl::{self, CardStableRefV1, PlayerSeatV1},
    state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone},
};
use serde::Serialize;

pub const SCHEMA_V1: &str = "mtg-kernel-premodern-ordered-public/v1";
pub const HUMAN_SCHEMA_V1: &str = "mtg-kernel-human-premodern-ordered-public/v1";
pub const HUMAN_ORDER_SCHEMA_V1: &str = "mtg-kernel-human-premodern-graveyard-order/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraveyardV1 {
    pub bottom_to_top: Vec<CardStableRefV1>,
    /// Publicly arrived cards whose relative order is still being chosen.
    /// This list is sorted by printing and public reference, not entry order.
    pub unplaced: Vec<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderChoiceV1 {
    pub batch: BatchV1,
    pub chosen_bottom_first: Vec<CardStableRefV1>,
    /// Exact native ordinals. If the engine suspends before a mixed batch
    /// commits, these identify the authenticated post-replacement arrivals.
    pub native_candidates: Vec<CardStableRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanOrderPromptV1 {
    schema: &'static str,
    viewer: PlayerSeatV1,
    chosen_bottom_first: Vec<&'static str>,
    /// Action indices are offsets into this display order and the exact
    /// native candidate order. Ordinals also distinguish identical printings.
    action_labels: Vec<String>,
    #[serde(skip)]
    expected: OrderChoiceV1,
}

impl HumanOrderPromptV1 {
    pub fn action_labels(&self) -> &[String] {
        &self.action_labels
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FullTextFormV1 {
    pub physical: CardStableRefV1,
    pub text_donor: Option<CardStableRefV1>,
    pub characteristics: FullTextCharacteristicsV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FullTextCharacteristicsV1 {
    pub text_name: &'static str,
    pub color_mask: u8,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtype_ids: Vec<u16>,
    pub keyword_bits: u32,
    pub protection_color_mask: u8,
    pub legendary: bool,
    pub power_toughness: Option<(i32, i32)>,
    pub printed_abilities_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FullTextProgramV1 {
    RetainedDiscard,
    DonatedActivation {
        printed_index: u8,
        effect: DonatedActivationV1,
    },
    PalinchronUntapLands,
    KarmicGuideReturnCreature,
    KarmicGuideEcho,
    DevourerSacrifice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum DonatedActivationV1 {
    /// {2}{U}{U}: return the physical host to its owner's hand.
    PalinchronReturnHost,
    /// Exile the library top to put its mana value in +1/+1 counters on the host.
    DevourerExileTopForCounters,
    /// Remove one host +1/+1 counter to deal one damage to any target.
    TriskelionRemoveCounterForDamage,
    /// At threshold, bottom three graveyard cards to give the host +3/+3 this turn.
    /// Restricted to one use per turn for the donated ability instance.
    ScroungerThresholdPump,
    /// Pay {G} and tap the host to reveal through the first basic land.
    /// The controller takes the basic land and puts the other cards into their graveyard.
    HermitDruidRevealThroughBasic,
    /// Discard a card to give the host +1/+1 this turn.
    PsychatogDiscardPump,
    /// Exile two graveyard cards to give the host +1/+1 this turn.
    PsychatogExilePump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProducerStageV1 {
    ActivationAnnouncing,
    TriggerPending { ordinal: usize },
    Stack { position: usize },
    Resolving,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FullTextProducerV1 {
    pub stage: ProducerStageV1,
    pub physical: CardStableRefV1,
    pub text_donor: CardStableRefV1,
    pub native_ability_index: Option<u8>,
    pub program: FullTextProgramV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObservationV1 {
    pub schema: &'static str,
    pub card_db_hash: u64,
    pub viewer: PlayerSeatV1,
    pub graveyards: [GraveyardV1; 2],
    pub order_choice: Option<OrderChoiceV1>,
    /// During ordering there is no stable post-batch battlefield text yet.
    pub full_text: Option<Vec<FullTextFormV1>>,
    pub producers: Vec<FullTextProducerV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGraveyardV1 {
    pub bottom_to_top: Vec<&'static str>,
    pub unplaced: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFullTextFormV1 {
    pub physical_name: &'static str,
    pub controller: PlayerSeatV1,
    pub text_donor_name: Option<&'static str>,
    pub characteristics: FullTextCharacteristicsV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFullTextProducerV1 {
    pub stage: ProducerStageV1,
    pub physical_name: &'static str,
    pub controller: PlayerSeatV1,
    pub text_donor_name: &'static str,
    pub program: FullTextProgramV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanObservationV1 {
    pub schema: &'static str,
    pub card_db_hash: u64,
    pub viewer: PlayerSeatV1,
    pub graveyards: [HumanGraveyardV1; 2],
    pub order_choice: Option<HumanOrderPromptV1>,
    pub full_text: Option<Vec<HumanFullTextFormV1>>,
    pub producers: Vec<HumanFullTextProducerV1>,
}

pub(crate) fn captured_card(source: AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}

fn graveyard(
    state: &GameState,
    owner: PlayerId,
    unplaced: &[ObjectLinkV4],
) -> Result<GraveyardV1, String> {
    let mut view = GraveyardV1 {
        bottom_to_top: Vec::new(),
        unplaced: Vec::new(),
    };
    for &object in &state.players[owner.index()].graveyard {
        let live = state
            .objects
            .try_get(object)
            .ok_or("graveyard card absent")?;
        if live.owner != owner || live.zone != Zone::Graveyard {
            return Err("ordered graveyard lost its public card".into());
        }
        let card = rl::card_ref(state, object).map_err(|error| error.to_string())?;
        let link = ObjectLinkV4 {
            object,
            zone_change_count: live.zone_change_count,
        };
        if unplaced.contains(&link) {
            view.unplaced.push(card);
        } else {
            view.bottom_to_top.push(card);
        }
    }
    view.unplaced
        .sort_by_key(|card| (card.card_db_id, card.arena_id));
    Ok(view)
}

fn form(state: &GameState, object: ObjectId) -> Result<Option<FullTextFormV1>, String> {
    let live = state
        .objects
        .try_get(object)
        .ok_or("full-text permanent absent")?;
    if live.v4.face_down_v1.is_some() {
        return Ok(None);
    }
    let physical = CARD_DEFS
        .get(usize::from(live.card_def))
        .ok_or("full-text physical printing absent")?;
    if !full_text_v1::owns_definition(physical) {
        return Ok(None);
    }
    let text = full_text_v1::active_text(state, object);
    let text_donor = text
        .map(|text| rl::card_ref(state, text.donor().object).map_err(|error| error.to_string()))
        .transpose()?;
    let types: Vec<_> = [
        CardType::Land,
        CardType::Creature,
        CardType::Instant,
        CardType::Sorcery,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Planeswalker,
    ]
    .into_iter()
    .filter(|kind| engine::object_has_type(state, object, *kind))
    .collect();
    let power_toughness = if types.contains(&CardType::Creature) {
        Some(engine::checked_effective_pt(state, object)?)
    } else {
        None
    };
    Ok(Some(FullTextFormV1 {
        physical: rl::card_ref(state, object).map_err(|error| error.to_string())?,
        text_donor,
        characteristics: FullTextCharacteristicsV1 {
            text_name: text.map_or(physical.name, |text| text.definition().name),
            color_mask: engine::object_color_mask(state, object),
            mana_value: engine::object_mana_value(state, object),
            types,
            subtype_ids: engine::effective_subtype_ids(state, object),
            keyword_bits: engine::effective_keywords(state, object, Keywords(u32::MAX)).0,
            protection_color_mask: crate::eternal_premodern_p2c_v1::statics::protection_mask(
                state, object,
            ),
            legendary: engine::effective_supertypes(state, object)
                .contains(&crate::card_def::Supertype::Legendary),
            power_toughness,
            printed_abilities_active: continuous_characteristics_v1::printed_abilities_active(
                state, object,
            ),
        },
    }))
}

/// Captures the public ordered-graveyard extension from a trusted native world
/// for a fixed viewer. The private checkpoint and its replay inputs stay native.
pub fn observe(state: &GameState, viewer: PlayerId) -> Result<ObservationV1, String> {
    let boundary = graveyard_order_v1::public_boundary(state, viewer)?;
    let none = [Vec::new(), Vec::new()];
    let unplaced = boundary.as_ref().map_or(&none, |row| &row.unplaced);
    let graveyards = [
        graveyard(state, PlayerId::P0, &unplaced[0])?,
        graveyard(state, PlayerId::P1, &unplaced[1])?,
    ];
    let (full_text, producers) = if boundary.is_some() {
        (None, Vec::new())
    } else {
        let mut forms = Vec::new();
        for &object in state.players.iter().flat_map(|player| &player.battlefield) {
            if let Some(form) = form(state, object)? {
                forms.push(form);
            }
        }
        (Some(forms), full_text_v1::public_producers(state)?)
    };
    Ok(ObservationV1 {
        schema: SCHEMA_V1,
        card_db_hash: KERNEL_CARDDB_HASH,
        viewer: viewer.into(),
        graveyards,
        order_choice: boundary.and_then(|row| row.choice),
        full_text,
        producers,
    })
}

/// Applies a chooser's exact observed ordinal through the native owner.
/// A changed menu, viewer, or incarnation must obtain a fresh observation.
pub fn choose_order(
    state: &mut GameState,
    viewer: PlayerId,
    expected: &OrderChoiceV1,
    ordinal: usize,
) -> Result<(), String> {
    let actual = graveyard_order_v1::public_boundary(state, viewer)?
        .and_then(|row| row.choice)
        .ok_or("no graveyard order choice for this viewer")?;
    if actual != *expected {
        return Err("graveyard order observation is stale".into());
    }
    let card = actual
        .native_candidates
        .get(ordinal)
        .ok_or("graveyard order ordinal is outside its native menu")?;
    engine::step(
        state,
        engine::Action::ChooseEffectTarget(Target::Object(ObjectId(card.arena_id))),
    )
}

pub fn observe_human_order(
    state: &GameState,
    viewer: PlayerId,
) -> Result<Option<HumanOrderPromptV1>, String> {
    graveyard_order_v1::public_boundary(state, viewer)?
        .and_then(|row| row.choice)
        .map(|choice| human_order_prompt(choice, viewer.into()))
        .transpose()
}

fn public_name(card: &CardStableRefV1) -> Result<&'static str, String> {
    CARD_DEFS
        .get(usize::from(card.card_db_id))
        .map(|definition| definition.name)
        .ok_or_else(|| "Premodern public printing absent".to_string())
}

fn human_order_prompt(
    expected: OrderChoiceV1,
    viewer: PlayerSeatV1,
) -> Result<HumanOrderPromptV1, String> {
    let chosen_bottom_first = expected
        .chosen_bottom_first
        .iter()
        .map(public_name)
        .collect::<Result<Vec<_>, _>>()?;
    let action_labels = expected
        .native_candidates
        .iter()
        .enumerate()
        .map(|(index, card)| {
            Ok(format!(
                "Place {} next (choice {})",
                public_name(card)?,
                index + 1
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(HumanOrderPromptV1 {
        schema: HUMAN_ORDER_SCHEMA_V1,
        viewer,
        chosen_bottom_first,
        action_labels,
        expected,
    })
}

/// Projects the same native facts for a human reader. Historical producers
/// use their captured printings, including after either object changes zones.
pub fn observe_human(state: &GameState, viewer: PlayerId) -> Result<HumanObservationV1, String> {
    let view = observe(state, viewer)?;
    let graveyard = |row: &GraveyardV1| -> Result<HumanGraveyardV1, String> {
        Ok(HumanGraveyardV1 {
            bottom_to_top: row
                .bottom_to_top
                .iter()
                .map(public_name)
                .collect::<Result<_, _>>()?,
            unplaced: row
                .unplaced
                .iter()
                .map(public_name)
                .collect::<Result<_, _>>()?,
        })
    };
    let full_text = view
        .full_text
        .map(|rows| {
            rows.into_iter()
                .map(|row| {
                    Ok(HumanFullTextFormV1 {
                        physical_name: public_name(&row.physical)?,
                        controller: row.physical.controller,
                        text_donor_name: row.text_donor.as_ref().map(public_name).transpose()?,
                        characteristics: row.characteristics,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;
    let producers = view
        .producers
        .into_iter()
        .map(|row| {
            Ok(HumanFullTextProducerV1 {
                stage: row.stage,
                physical_name: public_name(&row.physical)?,
                controller: row.physical.controller,
                text_donor_name: public_name(&row.text_donor)?,
                program: row.program,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(HumanObservationV1 {
        schema: HUMAN_SCHEMA_V1,
        card_db_hash: view.card_db_hash,
        viewer: view.viewer,
        graveyards: [
            graveyard(&view.graveyards[0])?,
            graveyard(&view.graveyards[1])?,
        ],
        order_choice: view
            .order_choice
            .map(|choice| human_order_prompt(choice, view.viewer))
            .transpose()?,
        full_text,
        producers,
    })
}

/// Uses the prompt's private native binding while the wire data carries names
/// and choice indices only.
pub fn choose_human_order(
    state: &mut GameState,
    viewer: PlayerId,
    prompt: &HumanOrderPromptV1,
    ordinal: usize,
) -> Result<(), String> {
    if prompt.viewer != viewer.into() {
        return Err("graveyard order prompt belongs to a different viewer".into());
    }
    choose_order(state, viewer, &prompt.expected, ordinal)
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::{event, policy_observation_v6::tests::put, state::Step};

    #[test]
    fn paired_hidden_worlds_preserve_public_order_and_both_native_presentations() {
        for owner in [PlayerId::P0, PlayerId::P1] {
            let build = |hidden: &str| {
                let mut state = GameState::new_from_libraries_with_graveyard_order_v1(
                    &[],
                    &[],
                    rl::card_name,
                    0x460_121,
                    owner,
                );
                state.step = Step::Main1;
                put(&mut state, owner.opponent(), hidden, Zone::Library);
                let first = put(&mut state, owner, "Palinchron", Zone::Library);
                let second = put(&mut state, owner, "Triskelion", Zone::Library);
                event::propose_and_commit_batch(
                    &mut state,
                    vec![
                        event::ProposedEvent::zone_change(first, Zone::Graveyard),
                        event::ProposedEvent::zone_change(second, Zone::Graveyard),
                    ],
                );
                (state, first, second)
            };
            let (mut a, first, second) = build("Island");
            let (mut b, _, _) = build("Mountain");
            assert_ne!(a.state_hash(), b.state_hash());
            for viewer in [owner, owner.opponent()] {
                assert_eq!(observe(&a, viewer).unwrap(), observe(&b, viewer).unwrap());
                assert_eq!(
                    observe_human_order(&a, viewer).unwrap(),
                    observe_human_order(&b, viewer).unwrap()
                );
                assert_eq!(
                    observe_human(&a, viewer).unwrap(),
                    observe_human(&b, viewer).unwrap()
                );
            }
            assert!(observe_human(&a, owner.opponent())
                .unwrap()
                .order_choice
                .is_none());
            let prompt = observe_human(&a, owner).unwrap().order_choice.unwrap();
            assert_eq!(
                Some(&prompt),
                observe_human_order(&a, owner).unwrap().as_ref()
            );
            assert_eq!(
                prompt.action_labels,
                [
                    "Place Palinchron next (choice 1)",
                    "Place Triskelion next (choice 2)",
                ]
            );
            let human = serde_json::to_value(&prompt).unwrap();
            assert!(human.get("expected").is_none());
            assert!(!human.to_string().contains("arena_id"));
            let choice = observe(&b, owner).unwrap().order_choice.unwrap();
            choose_human_order(&mut a, owner, &prompt, 1).unwrap();
            choose_order(&mut b, owner, &choice, 1).unwrap();
            for state in [&a, &b] {
                assert_eq!(state.players[owner.index()].graveyard, [second, first]);
            }
            assert_eq!(
                observe(&a, owner.opponent()).unwrap(),
                observe(&b, owner.opponent()).unwrap()
            );
        }
    }
}

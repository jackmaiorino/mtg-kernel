//! Explicit public Standard facts. Raw source links and timestamps never leave
//! this adapter; public relations use the same prompt-local card handles.

use super::*;
use crate::standard_creatures_v1 as creatures;

record!(HumanCardCharacteristicsV1, CardCharacteristicsV2, {
    #[serde(skip_serializing_if = "Option::is_none")]
    effective_identity: Option<EffectiveIdentityV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    legend_return_sources: Option<Vec<HumanCardRefV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    legend_rules: Option<crate::standard_legends_v1::LegendCharacteristicsV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_pt_until_end_of_turn: Option<[i16; 2]>,
    type_flags: CardTypeFlagsV2, base_power: Option<i32>, base_toughness: Option<i32>,
    effective_power: Option<i32>, effective_toughness: Option<i32>,
    effective_color_mask: u8, effective_subtype_ids: Vec<u16>, effective_keywords: KeywordFlagsV2,
});

// The identity is actor-visible knowledge, independent of the generic public
// face-down object's name. Keep the card database index out of human output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFaceDownKnowledgeV1 {
    pub object: HumanCardRefV1,
    pub name: String,
}
impl Project for FaceDownCardKnowledgeV1 {
    type Output = HumanFaceDownKnowledgeV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanFaceDownKnowledgeV1 {
            object: self.object.project(handles)?,
            name: crate::card_def::CARD_DEFS
                .get(self.card_db_id as usize)
                .ok_or(Error::InvalidVisibleReference)?
                .object_name
                .to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanRestrictedManaV1 {
    pub color: ManaColor,
    pub restriction: crate::card_def::ManaSpendRestrictionDef,
    pub source_name: String,
    pub source: Option<HumanCardRefV1>,
}
impl Project for PublicRestrictedManaV1 {
    type Output = HumanRestrictedManaV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanRestrictedManaV1 {
            color: self.color,
            restriction: self.restriction,
            source_name: crate::card_def::CARD_DEFS
                .get(self.source_card_def as usize)
                .ok_or(Error::InvalidVisibleReference)?
                .object_name
                .to_owned(),
            source: self.source.project(handles)?,
        })
    }
}

record!(HumanCounterDistributionV1, CounterDistributionPublicV1, {
    total: u32, targets: Vec<HumanTargetV1>, amounts: Vec<u32>,
});
record!(HumanCounterValuesV1, crate::state::Counters, {
    plus1_plus1: i32, minus1_minus1: i16, minus0_minus1: i16, stun: i16,
    lore: i16, oil: i16, charge: i32, net: i32,
});
record!(HumanCounterExtrasV1, creatures::CounterExtrasV1, {
    loyalty: u32, lifelink: i16, time: u8, finality: u16,
});
record!(HumanCounterTransferV1, creatures::CounterTransferV1, {
    counters: HumanCounterValuesV1, extras: HumanCounterExtrasV1,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTimedValueV1<T> {
    pub effect: T,
    pub timestamp_order: usize,
}
fn timed<T>(effect: T, timestamp: u64, handles: &Handles) -> Result<HumanTimedValueV1<T>, Error> {
    Ok(HumanTimedValueV1 {
        effect,
        timestamp_order: handles.timestamp_order(timestamp)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanSuppressionV1 {
    /// None means the exact suppressing permanent is no longer present.
    pub source: Option<HumanCardRefV1>,
    pub timestamp_order: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGraveyardAdventureV1 {
    pub holder: PlayerSeatV1,
    pub holder_turn_started: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAbilitySourceV1 {
    pub source: HumanCardRefV1,
    pub attached_to: Option<HumanCardRefV1>,
}
impl Project for crate::state::AbilitySourceContractV4 {
    type Output = HumanAbilitySourceV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanAbilitySourceV1 {
            source: CardStableRefV1 {
                arena_id: self.source.0,
                card_db_id: self.card_def,
                owner: self.owner.into(),
                controller: self.controller.into(),
                zone: self.zone,
                zone_change_count: self.zone_change_count,
            }
            .project(handles)?,
            attached_to: self.attached_to.and_then(|link| handles.current_link(link)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCreatureUpgradeV1 {
    pub finality: u16,
    pub temporary_creature: Option<HumanTimedValueV1<[i16; 2]>>,
    pub suppressed_by: Vec<HumanSuppressionV1>,
    pub graveyard_adventure: Option<HumanGraveyardAdventureV1>,
    pub haste_blockers_only: bool,
    pub creature_types: Option<HumanTimedValueV1<Vec<u16>>>,
    pub base_stats: Option<HumanTimedValueV1<[i16; 2]>>,
    pub color: Option<HumanTimedValueV1<u8>>,
    pub keyword_grants: Vec<HumanTimedValueV1<u32>>,
    pub keyword_losses: Vec<HumanTimedValueV1<u32>>,
    pub combat_impulse_order: Option<usize>,
    pub once_activated: Vec<u16>,
    pub wurmlet_resolved_turn: Option<(u32, PlayerSeatV1)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_impulse_source: Option<HumanAbilitySourceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat_impulse_donor: Option<HumanAbilitySourceV1>,
}
impl Project for creatures::CreatureUpgradeV1 {
    type Output = HumanCreatureUpgradeV1;
    fn project(&self, handles: &mut Handles) -> Result<Self::Output, Error> {
        Ok(HumanCreatureUpgradeV1 {
            finality: self.finality,
            temporary_creature: self
                .temporary_creature
                .map(|(p, t, stamp)| timed([p, t], stamp, handles))
                .transpose()?,
            suppressed_by: self
                .suppressed_by
                .iter()
                .map(|effect| {
                    Ok(HumanSuppressionV1 {
                        source: handles.current_link(effect.source),
                        timestamp_order: handles.timestamp_order(effect.timestamp)?,
                    })
                })
                .collect::<Result<_, Error>>()?,
            graveyard_adventure: self.graveyard_adventure.map(|permission| {
                HumanGraveyardAdventureV1 {
                    holder: permission.holder.into(),
                    holder_turn_started: permission.holder_turn_started,
                }
            }),
            haste_blockers_only: self.haste_blockers_only,
            creature_types: self
                .creature_types
                .as_ref()
                .map(|(types, stamp)| timed(types.clone(), *stamp, handles))
                .transpose()?,
            base_stats: self
                .base_stats
                .map(|(p, t, stamp)| timed([p, t], stamp, handles))
                .transpose()?,
            color: self
                .color
                .map(|(color, stamp)| timed(color, stamp, handles))
                .transpose()?,
            keyword_grants: self
                .keyword_grants
                .iter()
                .map(|(keywords, stamp)| timed(keywords.0, *stamp, handles))
                .collect::<Result<_, _>>()?,
            keyword_losses: self
                .keyword_losses
                .iter()
                .map(|(keywords, stamp)| timed(keywords.0, *stamp, handles))
                .collect::<Result<_, _>>()?,
            combat_impulse_order: self
                .combat_impulse
                .map(|stamp| handles.timestamp_order(stamp))
                .transpose()?,
            once_activated: self.once_activated.clone(),
            wurmlet_resolved_turn: self
                .wurmlet_resolved_turn
                .map(|(turn, player)| (turn, player.into())),
            combat_impulse_source: self.combat_impulse_source.project(handles)?,
            combat_impulse_donor: self.combat_impulse_donor.project(handles)?,
        })
    }
}

record!(HumanLegendGroupV1, LegendGroupSemanticV1, {
    controller: PlayerSeatV1, candidates: Vec<HumanCardRefV1>, kept: Option<HumanCardRefV1>,
});
record!(HumanPlaneswalkerV1, PlaneswalkerSemanticV1, { permanent: HumanCardRefV1, loyalty: u32 });
record!(HumanCombatPreventionV1, CombatPreventionSemanticV1, {
    permanent: HumanCardRefV1, turn: u32, active_player: PlayerSeatV1,
});
record!(HumanWidePlusOneCountersV1, WidePlusOneCountersSemanticV1, { permanent: HumanCardRefV1, count: i32 });
record!(HumanWideMarkedDamageV1, WideMarkedDamageSemanticV1, { permanent: HumanCardRefV1, damage: u32 });
record!(HumanTriggerUseV1, TriggerUseSemanticV1, {
    source: HumanCardRefV1, ability_index: u16, turn: u32, active_player: PlayerSeatV1, uses: u16,
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{ObjectId, PlayerId};
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::policy_surface_v5::PolicySurfaceV5;

    fn observation() -> v6::ObservationV6 {
        let mut state = ready_state();
        put(&mut state, PlayerId::P0, "Island", Zone::Battlefield);
        put(&mut state, PlayerId::P0, "Mountain", Zone::Battlefield);
        observe_policy_v6(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1).unwrap()
    }

    fn projected(observation: &v6::ObservationV6) -> HumanDecisionV1 {
        project_decision(
            observation,
            &[ActionSemanticV1::Pass {
                actor: PlayerSeatV1::P0,
            }],
            PlayerSeatV1::P0,
            1,
        )
        .unwrap()
        .0
    }

    fn assert_safe(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    assert!(
                        !matches!(
                            key.as_str(),
                            "arena_id"
                                | "zone_change_count"
                                | "zone_change_generation"
                                | "stack_item_id"
                                | "timestamp"
                                | "card_db_id"
                                | "card_def"
                        ),
                        "private field {key}"
                    );
                    assert_safe(child);
                }
            }
            serde_json::Value::Array(children) => {
                for child in children {
                    assert_safe(child);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn legacy_human_fields_keep_their_json_when_standard_facts_are_absent() {
        let observation = observation();
        let visible = projected(&observation);
        let value = serde_json::to_value(&visible.state).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            [
                "public",
                "own_hand",
                "known_library_cards",
                "known_hand_cards",
                "extensions",
                "policy_context"
            ]
            .map(str::to_owned)
            .into_iter()
            .collect()
        );
        for card in &visible.state.public.battlefield[0] {
            let original = observation.projection.surface.battlefield[0]
                .iter()
                .find(|original| original.card_name == card.stable.name)
                .unwrap();
            assert_eq!(
                serde_json::to_string(&card.characteristics).unwrap(),
                serde_json::to_string(&original.characteristics).unwrap()
            );
            assert!(serde_json::to_value(card)
                .unwrap()
                .get("creature_upgrade")
                .is_none());
        }
        let engine = &value["public"]["engine_context"];
        for field in [
            "pending_legend_rule",
            "planeswalkers",
            "combat_damage_prevention",
            "wide_plus_one_counters",
            "wide_marked_damage",
            "trigger_uses",
        ] {
            assert!(engine.get(field).is_none(), "new absent field {field}");
        }
        let activation = PendingActivationSemanticV2 {
            source: None,
            controller: PlayerSeatV1::P0,
            ability_index: 0,
            chosen_targets: vec![],
            cost_discard_paid: None,
            object_cost_chosen: vec![],
            crew_finished: false,
            loyalty_x: None,
            granted_ability: None,
        };
        assert_eq!(
            serde_json::to_value(
                activation
                    .project(&mut Handles::new(&observation).unwrap())
                    .unwrap()
            )
            .unwrap(),
            serde_json::json!({
                "source": null, "controller": PlayerSeatV1::P0, "ability_index": 0,
                "chosen_targets": [], "cost_discard_paid": null, "object_cost_chosen": []
            })
        );
        assert_safe(&value);
    }

    #[test]
    fn standard_human_facts_are_complete_and_independent_of_internal_ids() {
        let mut results = Vec::new();
        for offset in [0, 70000] {
            let mut observation = observation();
            for card in &mut observation.projection.surface.battlefield[0] {
                card.stable.arena_id += offset;
                card.stable.zone_change_count += offset;
            }
            let source = observation.projection.surface.battlefield[0][0]
                .stable
                .clone();
            let other = observation.projection.surface.battlefield[0][1]
                .stable
                .clone();
            let stamp = u64::from(offset) + 400;
            let card = &mut observation.projection.surface.battlefield[0][0];
            card.characteristics.effective_identity = Some(EffectiveIdentityV1 {
                names: vec!["Unlocked Door".into()],
                mana_value: 2,
                supertypes: vec![],
            });
            card.characteristics.legend_return_sources = Some(vec![other.clone()]);
            card.characteristics.base_pt_until_end_of_turn = Some([4, 4]);
            card.counters.oil = 2;
            card.counters.charge = 3;
            card.counters.net = 4;
            card.creature_upgrade = Some(creatures::CreatureUpgradeV1 {
                finality: 1,
                temporary_creature: Some((4, 4, stamp)),
                suppressed_by: vec![creatures::TidebinderSuppressionV1 {
                    source: crate::state::ObjectLinkV4 {
                        object: ObjectId(other.arena_id),
                        zone_change_count: other.zone_change_count,
                    },
                    timestamp: stamp + 1,
                }],
                graveyard_adventure: Some(creatures::GraveyardAdventurePermissionV1 {
                    holder: PlayerId::P0,
                    holder_turn_started: true,
                }),
                combat_impulse: Some(stamp + 2),
                combat_impulse_source: Some(crate::state::AbilitySourceContractV4 {
                    source: ObjectId(other.arena_id),
                    card_def: other.card_db_id,
                    owner: PlayerId::P0,
                    controller: PlayerId::P0,
                    zone: other.zone,
                    zone_change_count: other.zone_change_count,
                    attached_to: Some(crate::state::ObjectLinkV4 {
                        object: ObjectId(source.arena_id),
                        zone_change_count: source.zone_change_count,
                    }),
                }),
                ..Default::default()
            });
            observation
                .known_face_down_cards
                .push(FaceDownCardKnowledgeV1 {
                    object: source.clone(),
                    card_db_id: other.card_db_id,
                    card_name: "untrusted text is unused".into(),
                });
            observation.projection.poison_counters = Some([1, 2]);
            observation.projection.poison_prevention = Some([false, true]);
            observation.projection.creatures_attacked_this_turn = Some(2);
            observation.projection.ninja_emblems = Some([1, 0]);
            observation.projection.restricted_mana = Some([
                vec![PublicRestrictedManaV1 {
                    color: ManaColor::U,
                    restriction: crate::card_def::ManaSpendRestrictionDef::CreatureSpell,
                    source_card_def: other.card_db_id,
                    source: Some(other.clone()),
                }],
                vec![],
            ]);
            let target = TargetRefV1::Object {
                object: source.clone(),
            };
            let counters = CounterDistributionPublicV1 {
                total: 3,
                targets: vec![target.clone()],
                amounts: vec![3],
            };
            let transfer = creatures::CounterTransferV1 {
                counters: crate::state::Counters {
                    plus1_plus1: 12,
                    charge: 2,
                    ..Default::default()
                },
                extras: creatures::CounterExtrasV1 {
                    finality: 1,
                    loyalty: 3,
                    ..Default::default()
                },
            };
            observation
                .projection
                .surface
                .stack
                .push(StackItemPublicV2 {
                    stack_index: 0,
                    source: source.clone(),
                    controller: PlayerSeatV1::P0,
                    targets: vec![target],
                    stack_item_kind: StackItemKindV2::ActivatedAbility,
                    is_copy: false,
                    is_flashback: false,
                    mode_chosen: 0,
                    madness_offer: false,
                    kicked: false,
                    cast_method: None,
                    face_index: 0,
                    x_value: 0,
                    paid_cost_refs: vec![],
                    counter_distribution: Some(counters.clone()),
                    counter_transfer: Some(transfer),
                    granted_ability: Some((other.clone(), 2)),
                });
            let engine = &mut observation.projection.surface.engine_context;
            engine.pending_activation = Some(PendingActivationSemanticV2 {
                source: Some(source.clone()),
                controller: PlayerSeatV1::P0,
                ability_index: 1,
                chosen_targets: vec![],
                cost_discard_paid: None,
                object_cost_chosen: vec![],
                crew_finished: true,
                loyalty_x: Some(3),
                granted_ability: Some((other.clone(), 2)),
            });
            engine.pending_triggers.push(PendingTriggerSemanticV2 {
                source: Some(source.clone()),
                controller: PlayerSeatV1::P0,
                trigger_kind: PendingTriggerKindV2::TriggeredAbility,
                kicked: false,
                counter_distribution: Some(counters),
                counter_transfer: Some(transfer),
            });
            engine.planeswalkers = Some(vec![PlaneswalkerSemanticV1 {
                permanent: source.clone(),
                loyalty: 3,
            }]);
            let visible = projected(&observation);
            let value = serde_json::to_value(&visible).unwrap();
            assert_safe(&value);
            let state = &visible.state;
            assert_eq!(state.poison_counters, Some([1, 2]));
            assert_eq!(state.poison_prevention, Some([false, true]));
            assert_eq!(state.creatures_attacked_this_turn, Some(2));
            assert_eq!(state.ninja_emblems, Some([1, 0]));
            assert_eq!(state.known_face_down_cards[0].name, "Mountain");
            assert_eq!(
                state.restricted_mana.as_ref().unwrap()[0][0].source_name,
                "Mountain"
            );
            let card = state.public.battlefield[0]
                .iter()
                .find(|card| card.stable.name == "Island")
                .unwrap();
            assert_eq!(card.counters.charge, 3);
            assert_eq!(
                card.characteristics.legend_return_sources.as_ref().unwrap()[0].name,
                "Mountain"
            );
            let upgrade = card.creature_upgrade.as_ref().unwrap();
            assert_eq!(upgrade.finality, 1);
            assert_eq!(
                upgrade.suppressed_by[0].source.as_ref().unwrap().name,
                "Mountain"
            );
            assert_eq!(upgrade.suppressed_by[0].timestamp_order, 1);
            let stack = &state.public.stack[0];
            assert_eq!(
                stack.counter_distribution.as_ref().unwrap().amounts,
                vec![3]
            );
            assert_eq!(stack.counter_transfer.as_ref().unwrap().extras.finality, 1);
            assert_eq!(stack.granted_ability.as_ref().unwrap().0.name, "Mountain");
            let activation = state
                .public
                .engine_context
                .pending_activation
                .as_ref()
                .unwrap();
            assert!(activation.crew_finished);
            assert_eq!(activation.loyalty_x, Some(3));
            assert_eq!(
                state.public.engine_context.pending_triggers[0]
                    .counter_transfer
                    .as_ref()
                    .unwrap()
                    .counters
                    .plus1_plus1,
                12
            );
            results.push(value);
        }
        assert_eq!(results[0], results[1]);
    }

    #[test]
    fn departed_suppression_cannot_bind_a_new_incarnation() {
        let mut observation = observation();
        let other = observation.projection.surface.battlefield[0][1]
            .stable
            .clone();
        observation.projection.surface.battlefield[0][0].creature_upgrade =
            Some(creatures::CreatureUpgradeV1 {
                suppressed_by: vec![creatures::TidebinderSuppressionV1 {
                    source: crate::state::ObjectLinkV4 {
                        object: ObjectId(other.arena_id),
                        zone_change_count: other.zone_change_count + 1,
                    },
                    timestamp: 99,
                }],
                ..Default::default()
            });
        let visible = projected(&observation);
        let card = visible.state.public.battlefield[0]
            .iter()
            .find(|card| card.stable.name == "Island")
            .unwrap();
        assert!(card.creature_upgrade.as_ref().unwrap().suppressed_by[0]
            .source
            .is_none());
    }

    #[test]
    fn free_cast_permissions_and_creature_choice_meaning_are_visible() {
        let observation = observation();
        let mut handles = Handles::new(&observation).unwrap();
        let mut permission = ExilePlayPermissionPublicV2 {
            without_mana_cost: crate::engine::FreeCastV1(false),
            object: observation.projection.surface.battlefield[0][0]
                .stable
                .clone(),
            holder: PlayerSeatV1::P0,
            play_or_cast: PlayOrCastV2::Cast,
            zone_change_generation: 9000,
            expiry: PlayPermissionExpiryV2::UntilHoldersNextEndStep,
        };
        let old = serde_json::to_value(permission.project(&mut handles).unwrap()).unwrap();
        assert!(old.get("without_mana_cost").is_none());
        permission.without_mana_cost = crate::engine::FreeCastV1(true);
        let free = serde_json::to_value(permission.project(&mut handles).unwrap()).unwrap();
        assert_eq!(free["without_mana_cost"], true);
        assert_safe(&free);
        let choice = PendingEffectChoiceSemanticV4::Options {
            player: PlayerSeatV1::P0,
            structural_path: vec![],
            option_count: 2,
            creature_options: Some(vec![
                crate::standard_creature_choices_v1::CreatureChoiceOptionV1::Decline,
                crate::standard_creature_choices_v1::CreatureChoiceOptionV1::PayGreen(3),
            ]),
        };
        let projected = serde_json::to_value(choice.project(&mut handles).unwrap()).unwrap();
        assert_eq!(
            projected["creature_options"],
            serde_json::json!(["decline or stop", "pay {G}{G}{G}"])
        );
    }

    #[test]
    fn combat_impulse_donor_uses_a_historical_human_handle_and_is_absent_by_default() {
        let mut observation = observation();
        observation.projection.surface.battlefield[0][0].creature_upgrade =
            Some(Default::default());
        let before = serde_json::to_value(projected(&observation)).unwrap();
        assert!(!before.to_string().contains("combat_impulse_donor"));
        let mut results = Vec::new();
        for offset in [0, 80000] {
            observation.projection.surface.battlefield[0][0]
                .creature_upgrade
                .as_mut()
                .unwrap()
                .combat_impulse_donor = Some(crate::state::AbilitySourceContractV4 {
                source: ObjectId(9123 + offset),
                card_def: crate::card_def::card_id_by_name("Mountain").unwrap(),
                owner: PlayerId::P1,
                controller: PlayerId::P1,
                zone: Zone::Exile,
                zone_change_count: 3 + offset,
                attached_to: None,
            });
            let visible = projected(&observation);
            let card = visible.state.public.battlefield[0]
                .iter()
                .find(|card| card.stable.name == "Island")
                .unwrap();
            let donor = card
                .creature_upgrade
                .as_ref()
                .unwrap()
                .combat_impulse_donor
                .as_ref()
                .unwrap();
            assert_eq!(donor.source.name, "Mountain");
            assert_eq!(donor.source.zone, Zone::Exile);
            assert_eq!(donor.source.owner, PlayerSeatV1::P1);
            assert!(donor.source.handle.starts_with('c'));
            let value = serde_json::to_value(visible).unwrap();
            assert_safe(&value);
            results.push(value);
        }
        assert_eq!(results[0], results[1]);
    }
}

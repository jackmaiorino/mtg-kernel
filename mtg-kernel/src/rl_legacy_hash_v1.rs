//! Hash compatibility for public records extended by the Standard catalog.
use super::*;
use std::hash::{Hash, Hasher};

impl Hash for CountersV1 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.plus1_plus1.hash(state);
        self.minus1_minus1.hash(state);
        self.minus0_minus1.hash(state);
        self.stun.hash(state);
        self.lore.hash(state);
        if self.oil != 0 {
            "CountersV1/oil/v1".hash(state);
            self.oil.hash(state);
        }
        if self.charge != 0 {
            "CountersV1/charge/v1".hash(state);
            self.charge.hash(state);
        }
        if self.net != 0 {
            "CountersV1/net/v1".hash(state);
            self.net.hash(state);
        }
    }
}

impl Hash for CardCharacteristicsV2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_flags.hash(state);
        self.base_power.hash(state);
        self.base_toughness.hash(state);
        self.effective_power.hash(state);
        self.effective_toughness.hash(state);
        self.effective_color_mask.hash(state);
        self.effective_subtype_ids.hash(state);
        self.effective_keywords.hash(state);
        if let Some(value) = &self.legend_return_sources {
            "CardCharacteristicsV2/legend_return_sources/v1".hash(state);
            value.hash(state);
        }
        if let Some(value) = &self.legend_rules {
            "CardCharacteristicsV2/legend_rules/v1".hash(state);
            value.hash(state);
        }
        if let Some(value) = &self.base_pt_until_end_of_turn {
            "CardCharacteristicsV2/base_pt_until_end_of_turn/v1".hash(state);
            value.hash(state);
        }
    }
}

impl Hash for CardPublicV2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.stable.hash(state);
        self.card_name.hash(state);
        self.tapped.hash(state);
        self.summoning_sick.hash(state);
        self.damage.hash(state);
        self.counters.hash(state);
        self.attachments.hash(state);
        self.plotted_turn.hash(state);
        self.is_token.hash(state);
        self.face_index.hash(state);
        self.chosen_color.hash(state);
        self.entered_battlefield_turn.hash(state);
        self.ability_uses_this_turn.hash(state);
        self.skip_next_untap.hash(state);
        self.goaded_by.hash(state);
        self.characteristics.hash(state);
        if let Some(value) = &self.creature_upgrade {
            "CardPublicV2/creature_upgrade/v1".hash(state);
            value.hash(state);
        }
    }
}

impl Hash for StackItemPublicV2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.stack_index.hash(state);
        self.source.hash(state);
        self.controller.hash(state);
        self.targets.hash(state);
        self.stack_item_kind.hash(state);
        self.is_copy.hash(state);
        self.is_flashback.hash(state);
        self.mode_chosen.hash(state);
        self.madness_offer.hash(state);
        self.kicked.hash(state);
        self.cast_method.hash(state);
        self.face_index.hash(state);
        self.x_value.hash(state);
        self.paid_cost_refs.hash(state);
        if let Some(value) = &self.counter_distribution {
            "StackItemPublicV2/counter_distribution/v1".hash(state);
            value.hash(state);
        }
        if let Some(value) = &self.counter_transfer {
            "StackItemPublicV2/counter_transfer/v1".hash(state);
            value.hash(state);
        }
    }
}

impl Hash for PendingTriggerSemanticV2 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.controller.hash(state);
        self.trigger_kind.hash(state);
        self.kicked.hash(state);
        if let Some(value) = &self.counter_distribution {
            "PendingTriggerSemanticV2/counter_distribution/v1".hash(state);
            value.hash(state);
        }
        if let Some(value) = &self.counter_transfer {
            "PendingTriggerSemanticV2/counter_transfer/v1".hash(state);
            value.hash(state);
        }
    }
}

impl Hash for PendingEffectChoiceSemanticV4 {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Options {
                player,
                structural_path,
                option_count,
                creature_options,
            } => {
                player.hash(state);
                structural_path.hash(state);
                option_count.hash(state);
                if let Some(options) = creature_options {
                    "PendingEffectChoiceSemanticV4/creature_options/v1".hash(state);
                    options.hash(state);
                }
            }
            Self::Targets {
                player,
                structural_path,
                selected_targets,
                legal_targets,
                min_targets,
                max_targets,
                can_finish,
                ordered,
                purpose,
            } => {
                player.hash(state);
                structural_path.hash(state);
                selected_targets.hash(state);
                legal_targets.hash(state);
                min_targets.hash(state);
                max_targets.hash(state);
                can_finish.hash(state);
                ordered.hash(state);
                purpose.hash(state);
            }
            Self::Color {
                player,
                structural_path,
                legal_colors,
            } => {
                player.hash(state);
                structural_path.hash(state);
                legal_colors.hash(state);
            }
            Self::Number {
                player,
                structural_path,
                minimum,
                maximum,
            } => {
                player.hash(state);
                structural_path.hash(state);
                minimum.hash(state);
                maximum.hash(state);
            }
            Self::Boolean {
                player,
                structural_path,
                default,
                purpose,
            } => {
                player.hash(state);
                structural_path.hash(state);
                default.hash(state);
                purpose.hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These schemas are the public records from ddff546a, before the
    // Standard completion. Their derive implementations are the oracle,
    // including the original enum discriminants and field ordering.
    #[derive(Default)]
    struct HashBytes(Vec<u8>);
    impl Hasher for HashBytes {
        fn finish(&self) -> u64 {
            unreachable!("compare hash inputs directly")
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    fn bytes(value: &impl Hash) -> Vec<u8> {
        let mut hash = HashBytes::default();
        value.hash(&mut hash);
        hash.0
    }
    fn preserves_legacy<L: Hash + Serialize + serde::de::DeserializeOwned>(
        value: &(impl Hash + Serialize),
    ) {
        let wire = serde_json::to_string(value).unwrap();
        let legacy: L = serde_json::from_str(&wire).unwrap();
        assert_eq!(wire, serde_json::to_string(&legacy).unwrap());
        assert_eq!(bytes(value), bytes(&legacy));
    }
    fn card() -> CardPublicV2 {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let state =
            GameState::new_from_libraries(&[forest; 12], &[forest; 12], |_| "Forest".into(), 9);
        public_card_v2(
            &state,
            state.players[0].library[0],
            ObservationTextModeV2::FullArtifact,
        )
        .unwrap()
    }

    #[test]
    fn absent_standard_facts_preserve_public_bytes_and_rust_hash_streams() {
        let card = card();
        preserves_legacy::<LegacyCountersV1>(&card.counters);
        preserves_legacy::<LegacyCardCharacteristicsV2>(&card.characteristics);
        preserves_legacy::<LegacyCardPublicV2>(&card);
        let stack = StackItemPublicV2 {
            stack_index: 3,
            source: card.stable.clone(),
            controller: PlayerSeatV1::P1,
            targets: vec![TargetRefV1::Player {
                player: PlayerSeatV1::P0,
            }],
            stack_item_kind: StackItemKindV2::Spell,
            is_copy: false,
            is_flashback: false,
            mode_chosen: 1,
            madness_offer: false,
            kicked: true,
            cast_method: Some(CastMethodV4::Normal),
            face_index: 0,
            x_value: 5,
            paid_cost_refs: vec![card.stable.clone()],
            counter_distribution: None,
            counter_transfer: None,
        };
        preserves_legacy::<LegacyStackItemPublicV2>(&stack);
        let trigger = PendingTriggerSemanticV2 {
            source: Some(card.stable),
            controller: PlayerSeatV1::P1,
            trigger_kind: PendingTriggerKindV2::TriggeredAbility,
            kicked: true,
            counter_distribution: None,
            counter_transfer: None,
        };
        preserves_legacy::<LegacyPendingTriggerSemanticV2>(&trigger);
        for choice in [
            PendingEffectChoiceSemanticV4::Options {
                player: PlayerSeatV1::P1,
                structural_path: vec![2, 1],
                option_count: 3,
                creature_options: None,
            },
            PendingEffectChoiceSemanticV4::Targets {
                player: PlayerSeatV1::P0,
                structural_path: vec![1],
                selected_targets: vec![],
                legal_targets: vec![TargetRefV1::Player {
                    player: PlayerSeatV1::P1,
                }],
                min_targets: 0,
                max_targets: 1,
                can_finish: true,
                ordered: false,
                purpose: TargetSelectionPurposeV4::PlayerSelection,
            },
            PendingEffectChoiceSemanticV4::Color {
                player: PlayerSeatV1::P1,
                structural_path: vec![],
                legal_colors: vec![ManaColor::R, ManaColor::G],
            },
            PendingEffectChoiceSemanticV4::Number {
                player: PlayerSeatV1::P1,
                structural_path: vec![],
                minimum: -1,
                maximum: 3,
            },
            PendingEffectChoiceSemanticV4::Boolean {
                player: PlayerSeatV1::P0,
                structural_path: vec![4],
                default: Some(false),
                purpose: BooleanChoicePurposeV4::OptionalEffect,
            },
        ] {
            preserves_legacy::<LegacyPendingEffectChoiceSemanticV4>(&choice);
        }
    }

    #[test]
    fn present_standard_facts_remain_distinguishable_in_public_hashes() {
        let original = card();
        let mut changed = original.clone();
        changed.counters.oil = 1;
        assert_ne!(bytes(&original), bytes(&changed));
        changed = original.clone();
        changed.counters.charge = 1;
        assert_ne!(bytes(&original), bytes(&changed));
        changed = original.clone();
        changed.counters.net = 1;
        assert_ne!(bytes(&original), bytes(&changed));
        changed = original.clone();
        changed.creature_upgrade = Some(Default::default());
        assert_ne!(bytes(&original), bytes(&changed));
        changed = original.clone();
        changed.characteristics.base_pt_until_end_of_turn = Some([3, 3]);
        assert_ne!(bytes(&original), bytes(&changed));
    }

    #[derive(Hash, Serialize, Deserialize)]
    struct LegacyCountersV1 {
        pub plus1_plus1: i16,
        pub minus1_minus1: i16,
        pub minus0_minus1: i16,
        pub stun: i16,
        pub lore: i16,
    }
    #[derive(Hash, Serialize, Deserialize)]
    struct LegacyCardCharacteristicsV2 {
        pub type_flags: CardTypeFlagsV2,
        pub base_power: Option<i32>,
        pub base_toughness: Option<i32>,
        pub effective_power: Option<i32>,
        pub effective_toughness: Option<i32>,
        pub effective_color_mask: u8,
        pub effective_subtype_ids: Vec<u16>,
        pub effective_keywords: KeywordFlagsV2,
    }
    #[derive(Hash, Serialize, Deserialize)]
    struct LegacyCardPublicV2 {
        pub stable: CardStableRefV1,
        pub card_name: String,
        pub tapped: bool,
        pub summoning_sick: bool,
        pub damage: u16,
        pub counters: CountersV1,
        pub attachments: Vec<u32>,
        pub plotted_turn: Option<u32>,
        pub is_token: bool,
        pub face_index: u8,
        pub chosen_color: Option<ManaColor>,
        pub entered_battlefield_turn: Option<u32>,
        pub ability_uses_this_turn: Vec<AbilityUsePublicV4>,
        pub skip_next_untap: bool,
        pub goaded_by: Vec<GoadPublicV4>,
        pub characteristics: CardCharacteristicsV2,
    }
    #[derive(Hash, Serialize, Deserialize)]
    struct LegacyStackItemPublicV2 {
        pub stack_index: u32,
        pub source: CardStableRefV1,
        pub controller: PlayerSeatV1,
        pub targets: Vec<TargetRefV1>,
        pub stack_item_kind: StackItemKindV2,
        pub is_copy: bool,
        pub is_flashback: bool,
        pub mode_chosen: u8,
        pub madness_offer: bool,
        pub kicked: bool,
        pub cast_method: Option<CastMethodV4>,
        pub face_index: u8,
        pub x_value: u16,
        pub paid_cost_refs: Vec<CardStableRefV1>,
    }
    #[derive(Hash, Serialize, Deserialize)]
    struct LegacyPendingTriggerSemanticV2 {
        pub source: Option<CardStableRefV1>,
        pub controller: PlayerSeatV1,
        pub trigger_kind: PendingTriggerKindV2,
        pub kicked: bool,
    }
    #[derive(Hash, Serialize, Deserialize)]
    #[serde(tag = "choice_kind", rename_all = "snake_case")]
    enum LegacyPendingEffectChoiceSemanticV4 {
        Options {
            player: PlayerSeatV1,
            structural_path: Vec<u16>,
            option_count: u16,
        },
        Targets {
            player: PlayerSeatV1,
            structural_path: Vec<u16>,
            selected_targets: Vec<TargetRefV1>,
            legal_targets: Vec<TargetRefV1>,
            min_targets: u16,
            max_targets: u16,
            can_finish: bool,
            ordered: bool,
            purpose: TargetSelectionPurposeV4,
        },
        Color {
            player: PlayerSeatV1,
            structural_path: Vec<u16>,
            legal_colors: Vec<ManaColor>,
        },
        Number {
            player: PlayerSeatV1,
            structural_path: Vec<u16>,
            minimum: i32,
            maximum: i32,
        },
        Boolean {
            player: PlayerSeatV1,
            structural_path: Vec<u16>,
            default: Option<bool>,
            purpose: BooleanChoicePurposeV4,
        },
    }
}

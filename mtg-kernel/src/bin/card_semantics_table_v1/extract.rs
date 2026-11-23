//! Typed walk of one card definition into feature tokens.
//!
//! Every token is either a count token (how many times the feature occurs
//! on the card) or a value token (the largest amount seen for it). Value
//! tokens always sit beside the count token that names their kind, so a
//! value of zero is never confused with an absent feature.
//!
//! Effect combinators (`Sequence`, `Conditional`, `Choice` and the other
//! variants that hold a nested `EffectOp`) are matched on the typed enum.
//! The fields of every other `EffectOp` leaf are read through the leaf's
//! derived serde representation: field names and typed values (integers,
//! unit-variant names, tagged values), never `Debug` text. `CostComponent`
//! and `TriggerCondition` derive neither serde nor an introspection helper,
//! so their variant names come from the head of their `Debug` text (the
//! identifier before any field); their amounts are read by typed match.

use std::collections::{BTreeMap, BTreeSet};

use mtg_kernel::card_def::{
    CardDef, CardType, CostComponent, Keywords, ModeDef, Supertype, TargetSpec, CARD_DEFS,
};
use mtg_kernel::effect::{EffectCond, EffectOp};
use mtg_kernel::mana::{Cost, ManaColor, Pip};
use mtg_kernel::trigger::{triggers_for, TriggeredAbilityDef};
use serde_json::Value;

use super::mapping::{Emit, Mapping};

/// Every keyword bit `card_def::Keywords` defines, in bit order.
pub const KEYWORDS: [(&str, Keywords); 20] = [
    ("FLYING", Keywords::FLYING),
    ("REACH", Keywords::REACH),
    ("HASTE", Keywords::HASTE),
    ("VIGILANCE", Keywords::VIGILANCE),
    ("TRAMPLE", Keywords::TRAMPLE),
    ("FIRST_STRIKE", Keywords::FIRST_STRIKE),
    ("DOUBLE_STRIKE", Keywords::DOUBLE_STRIKE),
    ("DEATHTOUCH", Keywords::DEATHTOUCH),
    ("MENACE", Keywords::MENACE),
    ("DEFENDER", Keywords::DEFENDER),
    ("LIFELINK", Keywords::LIFELINK),
    ("HEXPROOF", Keywords::HEXPROOF),
    ("INDESTRUCTIBLE", Keywords::INDESTRUCTIBLE),
    (
        "PROTECTION_FROM_MONOCOLORED",
        Keywords::PROTECTION_FROM_MONOCOLORED,
    ),
    ("ISLANDWALK", Keywords::ISLANDWALK),
    ("FLASH", Keywords::FLASH),
    ("CANT_BE_BLOCKED", Keywords::CANT_BE_BLOCKED),
    ("TOXIC_1", Keywords::TOXIC_1),
    ("SHROUD", Keywords::SHROUD),
    (
        "PROTECTION_FROM_EVERYTHING",
        Keywords::PROTECTION_FROM_EVERYTHING,
    ),
];

/// Leaf fields that hold object, player or history identities rather than
/// amounts. They add nothing a card's text would say.
const IDENTITY_FIELDS: [&str; 14] = [
    "target_index",
    "source_index",
    "index",
    "capture",
    "capture_history_index",
    "damage_history_index",
    "entry_history_index",
    "attack_history_index",
    "schedule_history_index",
    "observer",
    "observer_index",
    "card_def",
    "other_than",
    "targeting_stack_item",
];

/// Amounts are clamped into this range so one odd value cannot dominate.
pub const VALUE_LIMIT: i64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Count,
    Value,
}

#[derive(Debug, Clone)]
pub struct Acc {
    pub kind: TokenKind,
    pub count: i64,
    pub value: i64,
    /// True when at least one contribution came through the mapping layer.
    pub via_mapping: bool,
    /// True when at least one contribution did not.
    pub direct: bool,
}

impl Acc {
    /// The integer this token contributes to the card's vector.
    pub fn encoded(&self) -> i64 {
        match self.kind {
            TokenKind::Count => self.count,
            TokenKind::Value => self.value,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CardTokens {
    pub tokens: BTreeMap<String, Acc>,
    /// Mapping-layer sources this card resolved through.
    pub mapped_sources: BTreeSet<String>,
    /// Places where a variant name came from a `Debug` head.
    pub debug_heads: BTreeSet<String>,
}

/// Token families that describe the card's effect programs.
pub const EFFECT_FAMILIES: [&str; 3] = ["leaf:", "op:", "cond:"];

impl CardTokens {
    pub fn effect_tree_token_count(&self) -> usize {
        self.tokens
            .keys()
            .filter(|token| {
                EFFECT_FAMILIES
                    .iter()
                    .any(|family| token.starts_with(family))
            })
            .count()
    }
}

pub struct Walker<'m> {
    mapping: Option<&'m Mapping>,
    out: CardTokens,
    /// Depth of mapping emission, so mapped tokens are marked.
    in_mapping: bool,
}

impl<'m> Walker<'m> {
    fn count(&mut self, token: String) {
        self.count_n(token, 1);
    }

    fn count_n(&mut self, token: String, n: i64) {
        let in_mapping = self.in_mapping;
        let acc = self.out.tokens.entry(token.clone()).or_insert(Acc {
            kind: TokenKind::Count,
            count: 0,
            value: 0,
            via_mapping: false,
            direct: false,
        });
        assert_eq!(
            acc.kind,
            TokenKind::Count,
            "token {token} used as both kinds"
        );
        acc.count += n;
        acc.via_mapping |= in_mapping;
        acc.direct |= !in_mapping;
    }

    fn value(&mut self, token: String, value: i64) {
        let value = value.clamp(-VALUE_LIMIT, VALUE_LIMIT);
        let in_mapping = self.in_mapping;
        let fresh = !self.out.tokens.contains_key(&token);
        let acc = self.out.tokens.entry(token.clone()).or_insert(Acc {
            kind: TokenKind::Value,
            count: 0,
            value,
            via_mapping: false,
            direct: false,
        });
        assert_eq!(
            acc.kind,
            TokenKind::Value,
            "token {token} used as both kinds"
        );
        acc.count += 1;
        if !fresh {
            acc.value = acc.value.max(value);
        }
        acc.via_mapping |= in_mapping;
        acc.direct |= !in_mapping;
    }

    /// Emits a mapping entry's tokens for `source` and returns true, or
    /// returns false when the mapping layer has no entry for it.
    fn mapped(&mut self, source: &str) -> bool {
        let Some(mapping) = self.mapping else {
            return false;
        };
        let Some(entry) = mapping.entries.get(source) else {
            return false;
        };
        self.out.mapped_sources.insert(source.to_string());
        let previous = self.in_mapping;
        self.in_mapping = true;
        for emit in &entry.emit {
            match emit {
                Emit::Count(token, n) => self.count_n(token.clone(), *n),
                Emit::Value(token, value) => self.value(token.clone(), *value),
            }
        }
        self.in_mapping = previous;
        true
    }
}

/// The identifier at the head of a `Debug` rendering: the variant name of
/// an enum value. Used only for enums without serde or a name helper.
fn debug_head(text: &str) -> String {
    text.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// The variant name of a serde externally tagged enum value.
fn serde_tag(value: &Value) -> Option<&str> {
    match value {
        Value::String(tag) => Some(tag),
        Value::Object(map) if map.len() == 1 => map.keys().next().map(String::as_str),
        _ => None,
    }
}

fn color_name(color: ManaColor) -> &'static str {
    match color {
        ManaColor::W => "W",
        ManaColor::U => "U",
        ManaColor::B => "B",
        ManaColor::R => "R",
        ManaColor::G => "G",
        ManaColor::C => "C",
    }
}

fn tag_of<T: serde::Serialize>(value: &T) -> String {
    let json = serde_json::to_value(value).expect("serializable");
    serde_tag(&json)
        .expect("externally tagged enum value")
        .to_string()
}

fn keyword_names(keywords: Keywords) -> Vec<String> {
    let mut names: Vec<String> = KEYWORDS
        .iter()
        .filter(|(_, bit)| keywords.has(*bit))
        .map(|(name, _)| (*name).to_string())
        .collect();
    let known = KEYWORDS.iter().fold(0u32, |acc, (_, bit)| acc | bit.0);
    let unknown = keywords.0 & !known;
    for bit in 0..32 {
        if unknown & (1 << bit) != 0 {
            names.push(format!("bit{bit}"));
        }
    }
    names
}

impl<'m> Walker<'m> {
    fn mana_cost(&mut self, prefix: &str, cost: &Cost) {
        let mut pips: BTreeMap<String, i64> = BTreeMap::new();
        for pip in cost.pips {
            let key = match pip {
                Pip::Colored(color) => format!("{prefix}pip:{}", color_name(*color)),
                Pip::Hybrid(a, b) => {
                    format!("{prefix}hybrid:{}{}", color_name(*a), color_name(*b))
                }
                Pip::Phyrexian(color) | Pip::PhyrexianAnyColor(color) => {
                    format!("{prefix}phyrexian:{}", color_name(*color))
                }
            };
            *pips.entry(key).or_insert(0) += 1;
        }
        for (key, n) in pips {
            self.count_n(key, n);
        }
        if cost.generic > 0 {
            self.value(format!("{prefix}generic"), i64::from(cost.generic));
        }
        if cost.x_count > 0 {
            self.value(format!("{prefix}x"), i64::from(cost.x_count));
        }
    }

    fn cost_components(&mut self, components: &[CostComponent]) {
        for component in components {
            self.cost_component(component);
        }
    }

    fn cost_component(&mut self, component: &CostComponent) {
        let head = debug_head(&format!("{component:?}"));
        let source = format!("costpart:{head}");
        if self.mapped(&source) {
            return;
        }
        self.out
            .debug_heads
            .insert(format!("CostComponent::{head}"));
        self.count(source.clone());
        match component {
            CostComponent::DiscardCards(n)
            | CostComponent::SacrificeLands(n)
            | CostComponent::PayLife(n)
            | CostComponent::ExileOtherCardsFromOwnGraveyard(n)
            | CostComponent::RemovePlusOneCountersFromControlledCreatures(n)
            | CostComponent::SacrificeOtherControlledCreatures(n)
            | CostComponent::Crew(n)
            | CostComponent::PayEnergyV1(n)
            | CostComponent::ExileOtherGraveyardCardsWithCardTypesV1(n)
            | CostComponent::DiscardLandCardsV1(n) => {
                self.value(format!("{source}.n"), i64::from(*n));
            }
            CostComponent::SacrificeControlled { count, .. }
            | CostComponent::TapControlled { count, .. }
            | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { count, .. }
            | CostComponent::TapUntappedControlledPermanentsV1 { count, .. }
            | CostComponent::ExileCardsFromHandWithColorV1 { count, .. } => {
                self.value(format!("{source}.n"), i64::from(*count));
            }
            CostComponent::Loyalty(delta) => {
                self.value(format!("{source}.n"), i64::from(*delta));
            }
            CostComponent::Mana(cost) | CostComponent::ConvokeMana(cost) => {
                self.mana_cost("costpart:mana:", cost);
            }
            CostComponent::SacrificeCreatureOrPayMana(options) => {
                self.cost_components(options);
            }
            _ => {}
        }
    }

    fn target_spec(&mut self, spec: TargetSpec) {
        if spec != TargetSpec::None {
            self.count(format!("target:{}", tag_of(&spec)));
        }
    }

    fn cond(&mut self, cond: &EffectCond) {
        if let EffectCond::And(a, b) = cond {
            self.cond(a);
            self.cond(b);
            return;
        }
        let tag = tag_of(cond);
        let source = format!("cond:{tag}");
        if self.mapped(&source) {
            return;
        }
        self.count(source);
    }

    /// Walks one effect program.
    pub fn effect(&mut self, op: &EffectOp) {
        match op {
            EffectOp::Sequence(steps) => {
                for step in steps {
                    self.effect(step);
                }
            }
            EffectOp::Conditional { cond, then, else_ } => {
                self.count("op:Conditional".to_string());
                self.cond(cond);
                self.effect(then);
                self.effect(else_);
            }
            EffectOp::Choice { options, .. } => {
                self.count("op:Choice".to_string());
                self.value("op:Choice.options".to_string(), options.len() as i64);
                for option in options {
                    self.effect(option);
                }
            }
            EffectOp::MayPayCostThen {
                discard,
                sacrifice_lands,
                return_permanent,
                then,
                otherwise,
            } => {
                self.count("op:MayPayCostThen".to_string());
                if *discard > 0 {
                    self.value("op:MayPayCostThen.discard".to_string(), i64::from(*discard));
                }
                if *sacrifice_lands > 0 {
                    self.value(
                        "op:MayPayCostThen.sacrifice_lands".to_string(),
                        i64::from(*sacrifice_lands),
                    );
                }
                if return_permanent.is_some() {
                    self.count("op:MayPayCostThen.return_permanent".to_string());
                }
                self.effect(then);
                if let Some(otherwise) = otherwise {
                    self.effect(otherwise);
                }
            }
            EffectOp::MayExileFromPlayersGraveyardMatchingThen {
                card_type, then, ..
            } => {
                self.count("op:MayExileFromPlayersGraveyardMatchingThen".to_string());
                self.count(format!(
                    "op:MayExileFromPlayersGraveyardMatchingThen.card_type={}",
                    tag_of(card_type)
                ));
                self.effect(then);
            }
            EffectOp::MayPayManaThen {
                colored,
                generic,
                then,
                ..
            } => {
                self.count("op:MayPayManaThen".to_string());
                for color in colored {
                    self.count(format!("op:MayPayManaThen.pip:{}", color_name(*color)));
                }
                if *generic > 0 {
                    self.value("op:MayPayManaThen.generic".to_string(), i64::from(*generic));
                }
                self.effect(then);
            }
            EffectOp::LookAtTopMayRevealThen { predicate, then } => {
                self.count("op:LookAtTopMayRevealThen".to_string());
                self.count(format!(
                    "op:LookAtTopMayRevealThen.predicate={}",
                    tag_of(predicate)
                ));
                self.effect(then);
            }
            EffectOp::BindEntrantOutgrowsSourceThen { then }
            | EffectOp::IfEntrantOutgrowsSourceThen { then, .. } => {
                self.count("op:IfEntrantOutgrowsSourceThen".to_string());
                self.effect(then);
            }
            EffectOp::MayDiscardCardOtherwiseV1 { unpaid, .. } => {
                self.count("op:MayDiscardCardOtherwise".to_string());
                self.effect(unpaid);
            }
            EffectOp::MaySacrificeNonlandOtherwiseV1 { unpaid, .. } => {
                self.count("op:MaySacrificeNonlandOtherwise".to_string());
                self.effect(unpaid);
            }
            leaf => self.leaf(leaf),
        }
    }

    fn leaf(&mut self, op: &EffectOp) {
        let json = serde_json::to_value(op).expect("EffectOp serializes");
        let tag = serde_tag(&json)
            .expect("EffectOp is externally tagged")
            .to_string();
        let source = format!("leaf:{tag}");
        if self.mapped(&source) {
            return;
        }
        self.count(source.clone());
        if let Value::Object(map) = &json {
            if let Some(fields) = map.get(&tag) {
                self.fields(&source, fields, 0);
            }
        }
    }

    /// Generic reading of a leaf's typed fields from its serde value.
    fn fields(&mut self, path: &str, value: &Value, depth: u32) {
        match value {
            Value::Object(map) => {
                for (field, inner) in map {
                    if IDENTITY_FIELDS.contains(&field.as_str()) {
                        continue;
                    }
                    self.field(&format!("{path}.{field}"), field, inner, depth);
                }
            }
            // A newtype leaf (`StandardV1(..)` and similar) or tuple leaf.
            other => self.field(&format!("{path}.0"), "0", other, depth),
        }
    }

    fn field(&mut self, path: &str, name: &str, value: &Value, depth: u32) {
        if name == "token_def" {
            if let Some(id) = value.as_u64() {
                self.token_def(path, id);
            }
            return;
        }
        if name == "keyword" || name == "keywords" {
            if let Some(bits) = value.as_u64() {
                for keyword in keyword_names(Keywords(bits as u32)) {
                    self.count(format!("{path}={keyword}"));
                }
            }
            return;
        }
        match value {
            Value::Null => {}
            Value::Bool(true) => self.count(path.to_string()),
            Value::Bool(false) => {}
            Value::Number(number) => {
                if let Some(n) = number.as_i64() {
                    self.value(path.to_string(), n);
                }
            }
            Value::String(variant) => self.count(format!("{path}={variant}")),
            Value::Array(items) => {
                for item in items {
                    match item {
                        Value::String(variant) => self.count(format!("{path}={variant}")),
                        Value::Number(_) => self.count(format!("{path}[]")),
                        nested if depth < 3 => self.fields(path, nested, depth + 1),
                        _ => {}
                    }
                }
            }
            Value::Object(map) => {
                if map.len() == 1 {
                    let (tag, inner) = map.iter().next().expect("one entry");
                    self.count(format!("{path}={tag}"));
                    match inner {
                        Value::Number(number) if tag != "Target" => {
                            if let Some(n) = number.as_i64() {
                                self.value(format!("{path}.{tag}"), n);
                            }
                        }
                        Value::Object(_) if depth < 3 => {
                            self.fields(&format!("{path}.{tag}"), inner, depth + 1)
                        }
                        _ => {}
                    }
                } else if depth < 3 {
                    self.fields(path, value, depth + 1);
                }
            }
        }
    }

    /// A created token is described by its own power and toughness, not by
    /// its registry id.
    fn token_def(&mut self, path: &str, id: u64) {
        self.count(path.to_string());
        if let Some(token) = CARD_DEFS.get(id as usize) {
            if let Some(power) = token.power {
                self.value(format!("{path}.power"), i64::from(power));
            }
            if let Some(toughness) = token.toughness {
                self.value(format!("{path}.toughness"), i64::from(toughness));
            }
            for keyword in keyword_names(token.keywords) {
                self.count(format!("{path}.kw={keyword}"));
            }
        }
    }

    fn mode(&mut self, mode: &ModeDef) {
        self.count("ability:mode".to_string());
        self.target_spec(mode.target_spec);
        self.effect(&(mode.effect)());
    }

    fn trigger(&mut self, trigger: &TriggeredAbilityDef) {
        self.count("ability:triggered".to_string());
        let head = debug_head(&format!("{:?}", trigger.condition));
        let source = format!("trigger:{head}");
        if !self.mapped(&source) {
            self.out
                .debug_heads
                .insert(format!("TriggerCondition::{head}"));
            self.count(source);
        }
        let zone = tag_of(&trigger.home_zone);
        if zone != "Battlefield" {
            self.count(format!("ability:triggered.zone={zone}"));
        }
        if trigger.intervening_if_kicked {
            self.count("ability:triggered.if_kicked".to_string());
        }
        if trigger.intervening_if_controls_another_source_card {
            self.count("ability:triggered.if_controls_another".to_string());
        }
        self.effect(&(trigger.effect)());
    }
}

/// True for the program every permanent spell runs: put itself onto the
/// battlefield. It says nothing about the card beyond being a permanent.
fn is_plain_permanent_entry(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::MoveObject {
            object: mtg_kernel::effect::ObjectRef::ThisSource,
            to_zone: mtg_kernel::state::Zone::Battlefield,
        }
    )
}

/// All tokens of one registry card.
pub fn card_tokens(id: u16, mapping: Option<&Mapping>) -> CardTokens {
    let def: &CardDef = &CARD_DEFS[usize::from(id)];
    let mut w = Walker {
        mapping,
        out: CardTokens::default(),
        in_mapping: false,
    };

    // Characteristics.
    for card_type in def.types {
        w.count(format!("type:{}", tag_of(card_type)));
    }
    for supertype in def.supertypes {
        let name = match supertype {
            Supertype::Basic => "Basic",
            Supertype::Snow => "Snow",
            Supertype::Legendary => "Legendary",
        };
        w.count(format!("supertype:{name}"));
    }
    for subtype in def.subtypes {
        w.count(format!("subtype:{}", tag_of(subtype)));
    }
    for color in def.colors {
        w.count(format!("color:{}", color_name(*color)));
    }
    w.value("stat:mana_value".to_string(), i64::from(def.mana_value));
    if let Some(power) = def.power {
        w.value("stat:power".to_string(), i64::from(power));
    }
    if let Some(toughness) = def.toughness {
        w.value("stat:toughness".to_string(), i64::from(toughness));
    }
    if let Some(loyalty) = def.starting_loyalty {
        w.value("stat:loyalty".to_string(), i64::from(loyalty));
    }
    for keyword in keyword_names(def.keywords) {
        w.count(format!("kw:{keyword}"));
    }

    // Costs.
    w.mana_cost("cost:", &def.cost);
    if let Some(alt) = &def.alt_cost {
        w.count("flag:alt_cost".to_string());
        let head = debug_head(&format!("{:?}", alt.condition));
        w.out
            .debug_heads
            .insert(format!("AltCostCondition::{head}"));
        w.count(format!("flag:alt_cost.condition={head}"));
        w.cost_components(alt.components);
    }
    if let Some(kicker) = &def.kicker_cost {
        w.count("flag:kicker".to_string());
        w.mana_cost("kicker:", kicker);
    }
    if let Some(additional) = def.additional_cost {
        w.count("flag:additional_cost".to_string());
        w.cost_components(additional);
    }
    if let Some(optional) = &def.optional_additional_cost {
        w.count(format!(
            "flag:optional_additional_cost={}",
            tag_of(optional)
        ));
    }
    if let Some(flashback) = &def.flashback {
        w.count("flag:flashback".to_string());
        w.cost_components(flashback.cost);
    }
    if let Some(escape) = &def.escape {
        w.count("flag:escape".to_string());
        w.cost_components(escape.cost);
    }
    if let Some(madness) = &def.madness_cost {
        w.count("flag:madness".to_string());
        w.mana_cost("madness:", madness);
    }
    if let Some(plot) = &def.plot_cost {
        w.count("flag:plot".to_string());
        w.mana_cost("plot:", plot);
    }
    if def.delve {
        w.count("flag:delve".to_string());
    }
    if def.generic_cost_reduction.is_some() {
        w.count("flag:generic_cost_reduction".to_string());
    }

    // Spell program and modes.
    w.target_spec(def.target_spec);
    if let Some(spell) = (def.spell_effect)() {
        if is_plain_permanent_entry(&spell) {
            w.count("flag:permanent_entry".to_string());
        } else {
            w.count("flag:spell_program".to_string());
            w.effect(&spell);
        }
    }
    if let Some(kicked) = def.kicked_target_spec {
        w.count("flag:kicked_target".to_string());
        w.target_spec(kicked);
    }
    for mode in def
        .mode2
        .iter()
        .chain(def.mode3.iter())
        .chain(def.additional_modes)
    {
        w.mode(mode);
    }
    if let Some(adventure) = &def.adventure {
        w.count("flag:adventure".to_string());
        w.mana_cost("adventure:", &adventure.cost);
        w.target_spec(adventure.target_spec);
        w.effect(&(adventure.effect)());
    }
    if let Some(omen) = &def.omen {
        w.count("flag:omen".to_string());
        w.mana_cost("omen:", &omen.cost);
        w.target_spec(omen.target_spec);
        w.effect(&(omen.effect)());
    }
    if let Some(bestow) = &def.bestow {
        w.count("flag:bestow".to_string());
        w.mana_cost("bestow:", &bestow.cost);
        w.target_spec(bestow.target_spec);
    }

    // Activated abilities.
    for ability in def.activated_abilities {
        w.count("ability:activated".to_string());
        let zone = tag_of(&ability.activation_zone);
        if zone != "Battlefield" {
            w.count(format!("ability:activated.zone={zone}"));
        }
        if ability.sorcery_speed_only {
            w.count("ability:activated.sorcery_speed".to_string());
        }
        if let Some(limit) = ability.max_activations_per_turn {
            w.value("ability:activated.per_turn".to_string(), i64::from(limit));
        }
        w.cost_components(ability.cost);
        w.target_spec(ability.target_spec);
        w.effect(&(ability.effect)());
    }

    // Mana abilities.
    let has_mana = (def.mana_ability)().is_some()
        || def.mana_ability_def.is_some()
        || !def.mana_ability_choices.is_empty()
        || !def.additional_mana_abilities.is_empty()
        || !def.restricted_mana_abilities.is_empty();
    if has_mana {
        w.count("ability:mana".to_string());
        for color in def.mana_ability_choices {
            w.count(format!("ability:mana.color={}", color_name(*color)));
        }
        if let Some(rich) = &def.mana_ability_def {
            let head = debug_head(&format!("{:?}", rich.cost));
            w.out
                .debug_heads
                .insert(format!("ManaAbilityCostDef::{head}"));
            w.count(format!("ability:mana.cost={head}"));
            let amount = debug_head(&format!("{:?}", rich.amount));
            w.out
                .debug_heads
                .insert(format!("ManaAbilityAmountDef::{amount}"));
            w.count(format!("ability:mana.amount={amount}"));
            if rich.controller_damage > 0 {
                w.value(
                    "ability:mana.controller_damage".to_string(),
                    i64::from(rich.controller_damage),
                );
            }
        }
        for extra in def.additional_mana_abilities {
            w.count("ability:mana.additional".to_string());
            for color in extra.colors {
                w.count(format!("ability:mana.color={}", color_name(*color)));
            }
            w.mana_cost("ability:mana.additional:", &extra.mana_cost);
        }
        if !def.restricted_mana_abilities.is_empty() {
            w.count("ability:mana.restricted".to_string());
        }
        if def.conditional_tap_yield.is_some() {
            w.count("ability:mana.conditional_yield".to_string());
        }
    }
    if !def.produces_mana.is_empty() && !has_mana {
        for color in def.produces_mana {
            w.count(format!("flag:produces_mana={}", color_name(*color)));
        }
    }

    // Triggered abilities.
    for trigger in triggers_for(id) {
        w.trigger(trigger);
    }

    // Permanent data.
    if let Some(equipment) = &def.equipment {
        w.count("flag:equipment".to_string());
        if equipment.power_delta != 0 {
            w.value(
                "flag:equipment.power".to_string(),
                i64::from(equipment.power_delta),
            );
        }
        if equipment.toughness_delta != 0 {
            w.value(
                "flag:equipment.toughness".to_string(),
                i64::from(equipment.toughness_delta),
            );
        }
        for keyword in keyword_names(Keywords(
            equipment.controller_turn_keywords.0 | equipment.other_turn_keywords.0,
        )) {
            w.count(format!("flag:equipment.kw={keyword}"));
        }
        if let Some(granted) = &equipment.granted_activated_ability {
            w.count("ability:granted".to_string());
            w.cost_components(granted.cost);
            w.target_spec(granted.target_spec);
            w.effect(&(granted.effect)());
        }
    }
    if let Some(attachment) = &def.attachment {
        let head = debug_head(&format!("{attachment:?}"));
        w.out.debug_heads.insert(format!("AttachmentDef::{head}"));
        w.count(format!("flag:attachment={head}"));
        if let mtg_kernel::card_def::AttachmentDef::AuraCreatureStatic(aura) = attachment {
            w.value("flag:attachment.power".to_string(), i64::from(aura.power));
            w.value(
                "flag:attachment.toughness".to_string(),
                i64::from(aura.toughness),
            );
            for keyword in keyword_names(aura.keywords) {
                w.count(format!("flag:attachment.kw={keyword}"));
            }
        }
    }
    if let Some(animation) = &def.animation {
        w.count("flag:animation".to_string());
        w.value(
            "flag:animation.power".to_string(),
            i64::from(animation.power),
        );
        w.value(
            "flag:animation.toughness".to_string(),
            i64::from(animation.toughness),
        );
    }
    if let Some(counters) = &def.enters_with_plus_one_counters {
        w.value(
            "flag:enters_with_counters".to_string(),
            i64::from(counters.count),
        );
    }
    let flags: [(&str, bool); 16] = [
        ("flag:land", def.is_land),
        ("flag:enters_tapped", def.enters_battlefield_tapped),
        (
            "flag:enters_tapped_unless",
            def.enters_battlefield_tapped_unless.is_some()
                || def.enters_tapped_unless_controller.is_some(),
        ),
        ("flag:uncounterable", def.spell_cannot_be_countered),
        ("flag:ward", def.ward_cost.is_some()),
        ("flag:changeling", def.changeling),
        ("flag:transform", def.transform_face.is_some()),
        ("flag:saga", def.saga.is_some()),
        (
            "flag:controlled_creature_grant",
            def.controlled_creature_grant.is_some(),
        ),
        (
            "flag:controlled_counter_keyword",
            def.controlled_counter_keyword.is_some(),
        ),
        (
            "flag:game_outcome_protection",
            def.game_outcome_protection_v1.is_some(),
        ),
        ("flag:minimum_blockers", def.minimum_blockers > 1),
        ("flag:records_mana_spent", def.records_mana_spent),
        (
            "flag:cant_be_blocked_by_monarch",
            def.cant_be_blocked_by_monarchs_creatures,
        ),
        (
            "flag:damage_replacement",
            def.damage_replacement_v1.is_some(),
        ),
        (
            "flag:activated_ability_reduction",
            !def.activated_ability_generic_reductions.is_empty(),
        ),
    ];
    for (name, on) in flags {
        if on {
            w.count(name.to_string());
        }
    }
    let permanent = def.types.iter().any(|t| {
        matches!(
            t,
            CardType::Creature
                | CardType::Artifact
                | CardType::Enchantment
                | CardType::Land
                | CardType::Planeswalker
        )
    });
    if permanent {
        w.count("flag:permanent".to_string());
    }
    w.out
}

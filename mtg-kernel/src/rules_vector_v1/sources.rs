//! Per-card source walk: every place the engine keeps a card's rules.
//!
//! `CardDef` is destructured with every field named, so a new field fails to
//! compile here until someone decides whether it carries rules. Rules the
//! engine keys by card outside `CardDef` (triggers, trigger target specs,
//! name-keyed statics) are read through the engine's own lookup functions.
//! The source-inventory test in `tests.rs` fails if a rules module gains a
//! card-name branch this walk does not account for.

use serde::Serialize;
use serde_json::{json, Value};

use super::facets::*;
use super::meaning::{self, reads, targets, triggers_costs, Env};
use crate::card_def::{
    ActivatedAbilityDef, AttachmentDef, CardDef, CostComponent, ManaAbilityAmountDef,
    ManaAbilityCostDef, ManaAbilityDef, TargetSpec, CARD_DEFS,
};
use crate::effect::{EffectOp, ObjectRef};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::Zone;

/// Number of ids in the frozen Pauper registry. The `limited-fdn-fixtures`
/// build appends its cards after these, so ids below this bound are the same
/// cards in both builds.
pub const PAUPER_REGISTRY_LEN_V1: usize = 192;

/// A printed characteristic, read from the card definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum PrintedF {
    Type(CardTypeF),
    Subtype(crate::card_def::Subtype),
    Supertype(SupertypeF),
    Color(ColorF),
    Colorless,
    /// Mana value, bucketed like `AmtF::fixed`.
    ManaValue(u8),
    /// Colored pips of one color in the mana cost.
    Pips(ColorF, u8),
    HybridPip,
    PhyrexianPip,
    GenericMana(u8),
    XInCost,
    Power(i8),
    Toughness(i8),
    /// Power or toughness defined by the game (`*`), absent in the printed row.
    NoPowerToughness,
    Loyalty(u8),
    /// Keyword bit index from `Keywords`.
    Keyword(u8),
    ProducesMana(ColorF),
    IsLand,
    IsToken,
    Changeling,
    EntersTapped,
    EntersTappedUnlessControlsSubtype,
    CannotBeCountered,
    Ward(u8),
    /// Ward—Collect evidence N, mana value bucketed like `AmtF::fixed`.
    WardCollectEvidence(u8),
    /// Ward—Pay N life, printed on the transform back face only.
    WardBackFacePayLife(u8),
    /// Ward—Discard a card.
    WardDiscardCard,
    MinimumBlockers(u8),
    CantBeBlockedByMonarchsCreatures,
    EntersWithPlusOneCounters {
        count: u8,
        if_kicked: bool,
    },
    /// Grants a keyword to controlled creatures with +1/+1 counters.
    ControlledCounterKeyword(u8),
    ChoosesColorAsEnters,
    ManaAbilityAddsChosenColor,
    Delve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SupertypeF {
    Basic,
    Snow,
    Legendary,
}

/// Everything the extractor read for one card.
#[derive(Debug, Clone, Serialize)]
pub struct CardRulesV1 {
    pub card_id: u16,
    pub printed: Vec<PrintedF>,
    pub abilities: Vec<AbilityFacts>,
    /// Token definitions this card creates (their facts are added prefixed).
    pub created_tokens: Vec<u16>,
    /// Card definitions referenced by identity (not recursed).
    pub referenced_cards: Vec<u16>,
    /// Rules the engine implements in code that no lookup returns as data.
    /// Listed in the report; each also emits `Atom::Opaque`.
    pub opaque_rules: Vec<&'static str>,
    /// Canonical typed record of every rules source, with no name. Two cards
    /// with equal records are functional reprints.
    pub record: Value,
}

fn keyword_bits(keywords: crate::card_def::Keywords) -> impl Iterator<Item = u8> {
    (0..32u8).filter(move |bit| keywords.0 & (1 << bit) != 0)
}

fn bucket(n: i64) -> u8 {
    match AmtF::fixed(n) {
        AmtF::Fixed(b) => b,
        _ => unreachable!(),
    }
}

fn clamp_i8(n: i16) -> i8 {
    n.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8
}

fn printed_cost(cost: &Cost, out: &mut Vec<PrintedF>) {
    let Cost {
        pips,
        generic,
        x_count,
    } = *cost;
    let mut per_color = [0u8; 6];
    for pip in pips {
        match *pip {
            Pip::Colored(color) => per_color[color as usize] += 1,
            Pip::Hybrid(_, _) => out.push(PrintedF::HybridPip),
            Pip::Phyrexian(_) => out.push(PrintedF::PhyrexianPip),
        }
    }
    for color in ManaColor::ALL {
        let count = per_color[color as usize];
        if count > 0 {
            out.push(PrintedF::Pips(color.into(), count));
        }
    }
    if generic > 0 {
        out.push(PrintedF::GenericMana(bucket(i64::from(generic))));
    }
    if x_count > 0 {
        out.push(PrintedF::XInCost);
    }
}

fn cost_record(cost: &Cost) -> Value {
    json!({
        "pips": cost.pips.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>(),
        "generic": cost.generic,
        "x": cost.x_count,
    })
}

fn components_record(components: &[CostComponent]) -> Value {
    Value::Array(
        components
            .iter()
            .map(|c| Value::String(format!("{c:?}")))
            .collect(),
    )
}

fn program_value(op: &EffectOp) -> Value {
    serde_json::to_value(op).expect("EffectOp serializes")
}

/// Builder for one card's abilities and record.
struct Walk {
    abilities: Vec<AbilityFacts>,
    tokens: Vec<u16>,
    referenced: Vec<u16>,
    opaque: Vec<&'static str>,
    record: serde_json::Map<String, Value>,
}

impl Walk {
    fn ability(&mut self, ctx: CtxF, fill: impl FnOnce(&mut Collector)) {
        let mut out = Collector::default();
        fill(&mut out);
        self.tokens.extend(out.created_tokens.iter().copied());
        self.referenced.extend(out.referenced_cards.iter().copied());
        if !out.atoms.is_empty() {
            self.abilities.push(AbilityFacts {
                ctx,
                atoms: out.atoms,
            });
        }
    }

    fn rec(&mut self, key: &str, value: Value) {
        let slot = self
            .record
            .entry(key.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        slot.as_array_mut()
            .expect("record slots are arrays")
            .push(value);
    }

    /// A resolution program plus the target spec governing it.
    fn program(&mut self, ctx: CtxF, key: &str, spec: TargetSpec, op: &EffectOp) {
        self.rec(
            key,
            json!({"target_spec": spec, "program": program_value(op)}),
        );
        let env = Env { target_spec: spec };
        self.ability(ctx, |out| {
            if is_plain_permanent_resolution(op) {
                out.permanent_resolution();
            } else {
                meaning::effect_op(op, &env, out);
            }
            targets::target_spec(spec, out);
        });
    }

    fn costs(&mut self, ctx: CtxF, key: &str, components: &[CostComponent], extra: Option<Atom>) {
        self.rec(key, components_record(components));
        self.ability(ctx, |out| {
            if let Some(atom) = extra {
                out.atoms.push(atom);
            }
            for &component in components {
                triggers_costs::cost_component(component, out);
            }
        });
    }
}

/// `MoveObject { ThisSource -> Battlefield }`: the ordinary permanent-spell
/// resolution shared by every permanent. Context, not a distinguishing part.
fn is_plain_permanent_resolution(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        }
    )
}

fn mana_ability_facts(def: &ManaAbilityDef, out: &mut Collector) {
    let ManaAbilityDef {
        cost,
        amount,
        controller_damage,
        max_activations_per_turn,
    } = *def;
    match cost {
        ManaAbilityCostDef::TapSelf => out.cost(CostAtom::Tap),
        ManaAbilityCostDef::SacrificeSelf => out.cost(CostAtom::MoveObject {
            from: ZoneF::Battlefield,
            to: ZoneF::Graveyard,
            obj: ObjF::ThisObject,
            amount: AmtF::Unit,
        }),
        ManaAbilityCostDef::TapSelfAndOtherUntappedControlledCreature => {
            out.cost(CostAtom::Tap);
            out.cost(CostAtom::TapOthers);
        }
        ManaAbilityCostDef::PutMinus0Minus1CounterOnSelf => {
            out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::fixed(1)),
            );
        }
        ManaAbilityCostDef::TapAndSacrificeSelf => {
            out.cost(CostAtom::Tap);
            out.cost(CostAtom::MoveObject {
                from: ZoneF::Battlefield,
                to: ZoneF::Graveyard,
                obj: ObjF::ThisObject,
                amount: AmtF::Unit,
            });
        }
        ManaAbilityCostDef::TapSelfPayLife(life) => {
            out.cost(CostAtom::Tap);
            out.cost(CostAtom::PayLife(life));
        }
        ManaAbilityCostDef::None => {}
    }
    let amount = match amount {
        ManaAbilityAmountDef::Fixed(n) => AmtF::fixed(i64::from(n)),
        ManaAbilityAmountDef::ControlledCreaturesWithKeyword(_) => {
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Typed(CardTypeF::Creature)),
                AggF::Count,
            );
            AmtF::Dynamic
        }
        ManaAbilityAmountDef::Dynamic(value) => reads::dynamic_value(value, out),
    };
    out.effect(
        EffectAtom::new(EvF::AddMana)
            .player(RelF::You)
            .amount(amount),
    );
    if controller_damage > 0 {
        out.effect(
            EffectAtom::new(EvF::Damage)
                .player(RelF::You)
                .obj(ObjF::Player)
                .amount(AmtF::fixed(i64::from(controller_damage))),
        );
    }
    if max_activations_per_turn.is_some() {
        out.control(ControlF::Repeat);
    }
}

/// The condition gating one additional mana ability.
fn mana_ability_condition_facts(
    condition: crate::card_def::ManaAbilityConditionDef,
    out: &mut Collector,
) {
    out.control(ControlF::Conditional);
    match condition {
        crate::card_def::ManaAbilityConditionDef::ControllerControlsPermanentWithEitherSubtype {
            first,
            second,
        } => {
            // Vocabulary gap: ObjF has no subtype class.
            let _ = (first, second);
            out.read(
                RelF::You,
                Some(ZoneF::Battlefield),
                Some(ObjF::Permanent),
                AggF::Any,
            );
        }
        crate::card_def::ManaAbilityConditionDef::SourceEnteredThisTurn => out.read(
            RelF::You,
            Some(ZoneF::Battlefield),
            Some(ObjF::ThisObject),
            AggF::EventThisTurn,
        ),
    }
}

fn activated(walk: &mut Walk, ctx: CtxF, key: &str, ability: &ActivatedAbilityDef) {
    let ActivatedAbilityDef {
        cost,
        target_spec,
        effect,
        activation_zone,
        sorcery_speed_only,
        activation_target_filter,
        max_activations_per_turn,
        face,
    } = *ability;
    let program = effect();
    if let Some(face) = face {
        // Keep every historical single-faced record byte-identical.
        walk.rec(&format!("{key}/face"), json!(face));
        walk.opaque
            .push("activated ability is restricted to a printed face");
    }
    walk.rec(
        key,
        json!({
            "cost": components_record(cost),
            "target_spec": target_spec,
            "program": program_value(&program),
            "zone": activation_zone,
            "sorcery_speed": sorcery_speed_only,
            "target_filter": format!("{activation_target_filter:?}"),
            "max_per_turn": max_activations_per_turn,
        }),
    );
    if cost.iter().any(|component| {
        matches!(
            component,
            CostComponent::SacrificeOtherControlledCreatures(_)
        )
    }) {
        walk.opaque.push("sacrifice cost excludes its source");
    }
    let env = Env { target_spec };
    walk.ability(ctx, |out| {
        for &component in cost {
            triggers_costs::cost_component(component, out);
        }
        meaning::effect_op(&program, &env, out);
        targets::target_spec(target_spec, out);
        out.atoms.push(Atom::ActivatedFrom(activation_zone.into()));
        if sorcery_speed_only {
            out.atoms.push(Atom::SorcerySpeed);
        }
        if max_activations_per_turn.is_some() {
            out.control(ControlF::Repeat);
        }
        match activation_target_filter {
            crate::card_def::ActivationTargetFilter::TargetSpecOnly => {}
            crate::card_def::ActivationTargetFilter::CreatureBlockedBySource => {
                out.target(TargetAtom::Object {
                    obj: ObjF::Typed(CardTypeF::Creature),
                    controller: Some(RelF::Opponent),
                    zone: ZoneF::Battlefield,
                    color: None,
                    mana_value_at_most: None,
                    excludes: None,
                });
            }
        }
    });
}

/// Extracts every rules source of one card definition.
pub fn card_rules(card_id: u16) -> CardRulesV1 {
    let def: &CardDef = &CARD_DEFS[usize::from(card_id)];
    let CardDef {
        // Identity, not rules.
        name,
        // Engine readiness, not rules (the report lists it).
        capability: _,
        cost,
        generic_cost_reduction,
        ward_cost,
        spell_cannot_be_countered,
        equipment,
        types,
        subtypes,
        supertypes,
        power,
        toughness,
        starting_loyalty,
        enters_with_plus_one_counters,
        controlled_counter_keyword,
        is_land,
        produces_mana,
        colors,
        target_spec,
        keywords,
        spell_effect,
        mana_ability: _, // Superseded by `mana_ability_program_for` below.
        alt_cost,
        kicker_cost,
        additional_cost,
        flashback,
        activated_abilities,
        plot_cost,
        madness_cost,
        mode2,
        mode3,
        is_token,
        escape,
        mana_ability_choices,
        enters_battlefield_tapped,
        mana_ability_def,
        minimum_blockers,
        omen,
        mana_value,
        mana_ability_includes_chosen_color,
        as_enters_choose_color_other_than,
        additional_mana_abilities,
        // A copiable display name, not rules.
        object_name: _,
        enters_battlefield_tapped_unless,
        attachment,
        transform_face,
        saga,
        optional_additional_cost,
        changeling,
        bestow,
        delve,
        adventure,
        cant_be_blocked_by_monarchs_creatures,
        conditional_tap_yield,
        enters_tapped_unless_controller,
        additional_mana_ability_conditions,
        restricted_mana_abilities,
        animation,
        activated_ability_generic_reductions,
        // Cast bookkeeping for a program's `ManaSpentToCast` count, which
        // the program's own facets already carry.
        records_mana_spent: _,
    } = def;

    let mut printed = Vec::new();
    for &t in types.iter() {
        printed.push(PrintedF::Type(t.into()));
    }
    for &s in subtypes.iter() {
        printed.push(PrintedF::Subtype(s));
    }
    for s in supertypes.iter() {
        printed.push(PrintedF::Supertype(match s {
            crate::card_def::Supertype::Basic => SupertypeF::Basic,
            crate::card_def::Supertype::Snow => SupertypeF::Snow,
            crate::card_def::Supertype::Legendary => SupertypeF::Legendary,
        }));
    }
    if colors.is_empty() {
        printed.push(PrintedF::Colorless);
    }
    for &c in colors.iter() {
        printed.push(PrintedF::Color(c.into()));
    }
    printed.push(PrintedF::ManaValue(bucket(i64::from(*mana_value))));
    printed_cost(cost, &mut printed);
    match (power, toughness) {
        (Some(p), Some(t)) => {
            printed.push(PrintedF::Power(clamp_i8(*p)));
            printed.push(PrintedF::Toughness(clamp_i8(*t)));
        }
        _ if types.contains(&crate::card_def::CardType::Creature) => {
            printed.push(PrintedF::NoPowerToughness)
        }
        _ => {}
    }
    if let Some(loyalty) = starting_loyalty {
        printed.push(PrintedF::Loyalty(bucket(i64::from(*loyalty))));
    }
    printed.extend(keyword_bits(*keywords).map(PrintedF::Keyword));
    for &c in produces_mana.iter() {
        printed.push(PrintedF::ProducesMana(c.into()));
    }
    if *is_land {
        printed.push(PrintedF::IsLand);
    }
    if *is_token {
        printed.push(PrintedF::IsToken);
    }
    if *changeling {
        printed.push(PrintedF::Changeling);
    }
    if *enters_battlefield_tapped {
        printed.push(PrintedF::EntersTapped);
    }
    if enters_battlefield_tapped_unless.is_some() {
        printed.push(PrintedF::EntersTappedUnlessControlsSubtype);
    }
    if *spell_cannot_be_countered {
        printed.push(PrintedF::CannotBeCountered);
    }
    if let Some(ward) = ward_cost {
        match ward {
            crate::card_def::WardCostDef::Generic(n) => {
                printed.push(PrintedF::Ward(bucket(i64::from(*n))))
            }
            crate::card_def::WardCostDef::CollectEvidence(n) => {
                printed.push(PrintedF::WardCollectEvidence(bucket(i64::from(*n))))
            }
            crate::card_def::WardCostDef::BackFacePayLife(n) => {
                printed.push(PrintedF::WardBackFacePayLife(bucket(i64::from(*n))))
            }
            crate::card_def::WardCostDef::DiscardCard => printed.push(PrintedF::WardDiscardCard),
        }
    }
    if *minimum_blockers > 1 {
        printed.push(PrintedF::MinimumBlockers(*minimum_blockers));
    }
    if *cant_be_blocked_by_monarchs_creatures {
        printed.push(PrintedF::CantBeBlockedByMonarchsCreatures);
    }
    if let Some(counters) = enters_with_plus_one_counters {
        printed.push(PrintedF::EntersWithPlusOneCounters {
            count: bucket(i64::from(counters.count)),
            if_kicked: counters.if_kicked,
        });
    }
    if let Some(keyword) = controlled_counter_keyword {
        printed.extend(keyword_bits(*keyword).map(PrintedF::ControlledCounterKeyword));
    }
    if as_enters_choose_color_other_than.is_some() {
        printed.push(PrintedF::ChoosesColorAsEnters);
    }
    if *mana_ability_includes_chosen_color {
        printed.push(PrintedF::ManaAbilityAddsChosenColor);
    }
    if *delve {
        printed.push(PrintedF::Delve);
    }
    printed.sort();
    printed.dedup();

    let mut walk = Walk {
        abilities: Vec::new(),
        tokens: Vec::new(),
        referenced: Vec::new(),
        opaque: Vec::new(),
        record: serde_json::Map::new(),
    };
    walk.rec(
        "printed",
        serde_json::to_value(&printed).expect("printed facts serialize"),
    );
    walk.rec(
        "printed_detail",
        json!({
            "cost": cost_record(cost),
            "power": power,
            "toughness": toughness,
            "subtypes": subtypes.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
            "keywords": keywords.0,
            "ward": format!("{ward_cost:?}"),
            "enters_tapped_unless": format!("{enters_battlefield_tapped_unless:?}"),
            "as_enters_choose_color_other_than": format!("{as_enters_choose_color_other_than:?}"),
        }),
    );

    // Resolution programs.
    if let Some(op) = spell_effect() {
        walk.program(CtxF::Spell, "spell", *target_spec, &op);
    }
    for mode in [mode2, mode3].into_iter().flatten() {
        let program = (mode.effect)();
        walk.program(CtxF::Mode, "mode", mode.target_spec, &program);
    }
    if let Some(form) = omen {
        printed_form(&mut walk, "omen", &form.cost, form.types);
        let program = (form.effect)();
        walk.program(CtxF::AltForm, "omen_program", form.target_spec, &program);
    }
    if let Some(form) = adventure {
        // `form.name` is identity, not rules.
        printed_form(&mut walk, "adventure", &form.cost, form.types);
        let program = (form.effect)();
        walk.program(
            CtxF::AltForm,
            "adventure_program",
            form.target_spec,
            &program,
        );
    }
    if let Some(form) = bestow {
        walk.rec(
            "bestow",
            json!({"cost": cost_record(&form.cost), "target_spec": form.target_spec}),
        );
        let spec = form.target_spec;
        walk.ability(CtxF::AltForm, |out| {
            out.cost(CostAtom::Mana);
            out.effect(EffectAtom::new(EvF::Attach).obj(ObjF::ThisObject));
            targets::target_spec(spec, out);
        });
    }
    if let Some(saga) = saga {
        for chapter in saga.chapter_effects {
            let program = chapter();
            walk.program(CtxF::Chapter, "chapter", TargetSpec::None, &program);
        }
    }

    // Alternative and additional costs.
    if let Some(alt) = alt_cost {
        walk.rec(
            "alt_cost_condition",
            Value::String(format!("{:?}", alt.condition)),
        );
        walk.costs(CtxF::AltForm, "alt_cost", alt.components, None);
        if let crate::card_def::AltCostCondition::ControlsPermanentWithSubtype(_) = alt.condition {
            walk.ability(CtxF::AltForm, |out| {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Permanent),
                    AggF::Any,
                )
            });
        }
    }
    if let Some(kicker) = kicker_cost {
        walk.rec("kicker", cost_record(kicker));
        walk.ability(CtxF::AltForm, |out| {
            out.cost(CostAtom::Mana);
            out.control(ControlF::Optional);
        });
    }
    if let Some(components) = additional_cost {
        walk.costs(CtxF::Spell, "additional_cost", components, None);
    }
    if let Some(optional) = optional_additional_cost {
        walk.rec(
            "optional_additional_cost",
            Value::String(format!("{optional:?}")),
        );
        walk.ability(CtxF::Spell, |out| {
            optional_cost_facts(*optional, out);
        });
    }
    if let Some(fb) = flashback {
        walk.costs(
            CtxF::AltZoneCast,
            "flashback",
            fb.cost,
            Some(Atom::CastFrom(ZoneF::Graveyard)),
        );
    }
    if let Some(esc) = escape {
        walk.costs(
            CtxF::AltZoneCast,
            "escape",
            esc.cost,
            Some(Atom::CastFrom(ZoneF::Graveyard)),
        );
    }
    if let Some(plot) = plot_cost {
        walk.rec("plot", cost_record(plot));
        walk.ability(CtxF::AltZoneCast, |out| {
            out.cost(CostAtom::Mana);
            out.atoms.push(Atom::CastFrom(ZoneF::Exile));
        });
    }
    if let Some(madness) = madness_cost {
        walk.rec("madness", cost_record(madness));
        walk.ability(CtxF::AltZoneCast, |out| {
            out.cost(CostAtom::Mana);
            out.atoms.push(Atom::CastFrom(ZoneF::Exile));
            out.trigger(TrigF::SelfLeaves { to: None });
        });
    }
    if let Some(reduction) = generic_cost_reduction {
        walk.rec("cost_reduction", Value::String(format!("{reduction:?}")));
        let count = reduction.count;
        walk.ability(CtxF::Static, |out| {
            out.cost(CostAtom::Reduced);
            reads::dynamic_count(count, out);
        });
    }
    if *delve {
        walk.ability(CtxF::Static, |out| {
            out.cost(CostAtom::Reduced);
            out.cost(CostAtom::MoveObject {
                from: ZoneF::Graveyard,
                to: ZoneF::Exile,
                obj: ObjF::AnyCard,
                amount: AmtF::Dynamic,
            });
        });
    }

    // Activated and mana abilities.
    for ability in activated_abilities.iter() {
        activated(&mut walk, CtxF::Activated, "activated", ability);
    }
    for &color in mana_ability_choices.iter() {
        if let Some(program) = def.mana_ability_program_for(color, None) {
            walk.program(CtxF::Mana, "mana", TargetSpec::None, &program);
        }
    }
    if let Some(rich) = mana_ability_def {
        walk.rec("mana_ability_def", Value::String(format!("{rich:?}")));
        walk.ability(CtxF::Mana, |out| mana_ability_facts(rich, out));
    }
    for (index, additional) in additional_mana_abilities.iter().enumerate() {
        walk.rec("additional_mana", Value::String(format!("{additional:?}")));
        let condition = additional_mana_ability_conditions
            .get(index)
            .copied()
            .flatten();
        if let Some(condition) = condition {
            walk.rec(
                "additional_mana_condition",
                Value::String(format!("{condition:?}")),
            );
        }
        walk.ability(CtxF::Mana, |out| {
            out.cost(CostAtom::Mana);
            mana_ability_facts(&additional.ability, out);
            if let Some(condition) = condition {
                mana_ability_condition_facts(condition, out);
            }
        });
    }
    for restricted in restricted_mana_abilities.iter() {
        walk.rec("restricted_mana", Value::String(format!("{restricted:?}")));
        walk.ability(CtxF::Mana, |out| {
            // `{T}: Add one of these colors`, spendable only while paying a
            // creature spell's total cost. Vocabulary gap: no spending
            // restriction; it is marked conditional.
            out.cost(CostAtom::Tap);
            for &color in restricted.colors {
                out.effect(
                    EffectAtom::new(EvF::AddMana)
                        .player(RelF::You)
                        .amount(AmtF::fixed(1))
                        .color(color.into()),
                );
            }
            match restricted.restriction {
                crate::card_def::ManaSpendRestrictionDef::CreatureSpell => {
                    out.control(ControlF::Conditional)
                }
            }
        });
    }
    if let Some(amount) = conditional_tap_yield {
        walk.rec(
            "conditional_tap_yield",
            Value::String(format!("{amount:?}")),
        );
    }
    for reduction in activated_ability_generic_reductions.iter() {
        walk.rec(
            "activated_ability_generic_reduction",
            Value::String(format!("{reduction:?}")),
        );
        walk.ability(CtxF::Static, |out| {
            out.cost(CostAtom::Reduced);
            match reduction.per {
                crate::card_def::ActivatedAbilityReductionCountDef::ControlledLegendaryCreatures => {
                    // Vocabulary gap: no legendary filter.
                    out.read(
                        RelF::You,
                        Some(ZoneF::Battlefield),
                        Some(ObjF::Typed(CardTypeF::Creature)),
                        AggF::Count,
                    )
                }
            }
        });
    }
    if let Some(rule) = enters_tapped_unless_controller {
        walk.rec(
            "enters_tapped_unless_controller",
            Value::String(format!("{rule:?}")),
        );
        walk.ability(CtxF::Static, |out| {
            out.control(ControlF::Conditional);
            match *rule {
                crate::card_def::EntersTappedUnlessControllerDef::ControlsAtMostOtherLands(n)
                | crate::card_def::EntersTappedUnlessControllerDef::ControlsAtLeastOtherLands(n) => {
                    out.read(
                        RelF::You,
                        Some(ZoneF::Battlefield),
                        Some(ObjF::Typed(CardTypeF::Land)),
                        AggF::AtLeast(bucket(i64::from(n))),
                    )
                }
                crate::card_def::EntersTappedUnlessControllerDef::WithinOwnFirstTurns(_) => {
                    // Vocabulary gap: no turn-number read.
                    out.atoms.push(Atom::Opaque)
                }
            }
            out.effect(EffectAtom::new(EvF::Tap).obj(ObjF::ThisObject));
        });
    }
    if let Some(animation) = animation {
        let crate::card_def::AnimationDef {
            power,
            toughness,
            artifact,
            colors,
            subtypes,
            keywords,
        } = *animation;
        walk.rec(
            "animation",
            json!({
                "power": power,
                "toughness": toughness,
                "artifact": artifact,
                "colors": colors.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
                "subtypes": subtypes.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
                "keywords": keywords.0,
            }),
        );
    }

    // Triggered abilities, keyed by the engine outside `CardDef`.
    for trigger in crate::trigger::triggers_for(card_id) {
        let crate::trigger::TriggeredAbilityDef {
            condition,
            home_zone,
            intervening_if_kicked,
            intervening_if_controls_another_source_card,
            effect,
            face_index,
        } = *trigger;
        let event_programs = crate::trigger::event_time_trigger_programs(card_id, condition);
        let programs = if event_programs.is_empty() {
            vec![effect()]
        } else {
            event_programs
        };
        for program in programs {
            let spec = crate::trigger::target_spec_for_trigger(card_id, &program)
                .unwrap_or(TargetSpec::None);
            let optional_cost =
                crate::trigger::required_optional_additional_cost_for_trigger(card_id, &program);
            walk.rec(
                "trigger",
                json!({
                    "condition": format!("{condition:?}"),
                    "home_zone": home_zone,
                    "if_kicked": intervening_if_kicked,
                    "if_controls_another_source_card": intervening_if_controls_another_source_card,
                    "target_spec": spec,
                    "program": program_value(&program),
                    "optional_cost": format!("{optional_cost:?}"),
                }),
            );
            if face_index != 0 {
                // A transform back face's own trigger.
                walk.rec("trigger_face_index", json!(face_index));
            }
            if matches!(
                condition,
                crate::trigger::TriggerCondition::ControllerFirstLifeGain { .. }
            ) {
                walk.opaque
                    .push("global first life gain ordinal and optional own-turn gate");
            }
            let env = Env { target_spec: spec };
            if matches!(
                condition,
                crate::trigger::TriggerCondition::CastNoncreatureOrSubtype(_)
            ) {
                walk.opaque
                    .push("noncreature spell or selected subtype cast union predicate");
            }
            walk.ability(CtxF::Trigger, |out| {
                triggers_costs::trigger_condition(condition, out);
                if home_zone != Zone::Battlefield {
                    out.atoms.push(Atom::TriggersFrom(home_zone.into()));
                }
                if intervening_if_kicked {
                    out.control(ControlF::Conditional);
                }
                if intervening_if_controls_another_source_card {
                    out.control(ControlF::Conditional);
                    out.read(
                        RelF::You,
                        Some(ZoneF::Battlefield),
                        Some(ObjF::SpecificCard),
                        AggF::Any,
                    );
                }
                if let Some(optional) = optional_cost {
                    optional_cost_facts(optional, out);
                }
                match crate::trigger::unselected_trigger_modes(card_id, &program) {
                    // A modal trigger: each mode carries its own target spec.
                    Some(modes) => {
                        out.control(ControlF::ChooseBranch);
                        for (mode_spec, mode) in modes {
                            let mode_env = Env {
                                target_spec: mode_spec,
                            };
                            meaning::effect_op(&mode, &mode_env, out);
                            targets::target_spec(mode_spec, out);
                        }
                    }
                    None => {
                        meaning::effect_op(&program, &env, out);
                        targets::target_spec(spec, out);
                        if crate::trigger::distributes_last_known_plus_one_counters(
                            card_id, condition,
                        ) {
                            // Built at trigger creation: one controller
                            // choice per last-known +1/+1 counter, each a
                            // +1/+1 counter on a creature they control.
                            out.read(
                                RelF::You,
                                Some(ZoneF::Graveyard),
                                Some(ObjF::ThisObject),
                                AggF::Characteristic,
                            );
                            out.control(ControlF::Repeat);
                            out.control(ControlF::ChooseObjects);
                            for ev in [EvF::PlaceCounter, EvF::StatChange] {
                                out.effect(
                                    EffectAtom::new(ev)
                                        .player(RelF::You)
                                        .obj(ObjF::Typed(CardTypeF::Creature))
                                        .amount(AmtF::Dynamic)
                                        .duration(DurF::Permanent),
                                );
                            }
                        }
                    }
                }
            });
        }
    }
    if crate::engine::has_storm(def) {
        walk.rec("storm", Value::Bool(true));
        walk.ability(CtxF::Trigger, |out| {
            out.trigger(TrigF::SelfCast);
            out.read(RelF::EachPlayer, None, Some(ObjF::Spell), AggF::Count);
            out.effect(
                EffectAtom::new(EvF::Copy)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::Dynamic),
            );
        });
    }

    // Static abilities the engine keys by name outside `CardDef`.
    if let Some(boost) = crate::engine::static_self_boost_for(name) {
        let crate::engine::StaticSelfBoostDef {
            condition: _, // An opaque predicate; see `opaque_rules`.
            power,
            toughness,
            grant_haste,
            battlefield_only,
        } = boost;
        if battlefield_only {
            walk.rec("static_self_boost_home_zone", json!(Zone::Battlefield));
        }
        walk.opaque
            .push("static self boost gated on an engine predicate");
        walk.rec(
            "static_self_boost",
            json!({"power": power, "toughness": toughness, "haste": grant_haste}),
        );
        walk.ability(CtxF::Static, |out| {
            out.control(ControlF::Conditional);
            out.atoms.push(Atom::Opaque);
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::stat(i64::from(power), i64::from(toughness)))
                    .duration(DurF::WhileOnBattlefield),
            );
            if grant_haste {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .obj(ObjF::ThisObject)
                        .duration(DurF::WhileOnBattlefield)
                        .keyword(
                            keyword_bits(crate::card_def::Keywords::HASTE)
                                .next()
                                .unwrap(),
                        ),
                );
            }
        });
    }
    if let Some(boost) = crate::engine::static_controlled_subtype_boost_for(name) {
        let crate::engine::StaticControlledSubtypeBoostDef {
            subtype,
            exclude_source,
            power,
            toughness,
        } = boost;
        walk.rec(
            "static_subtype_boost",
            json!({"subtype": subtype, "exclude_source": exclude_source, "power": power, "toughness": toughness}),
        );
        walk.ability(CtxF::Static, |out| {
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::stat(i64::from(power), i64::from(toughness)))
                    .duration(DurF::WhileOnBattlefield),
            );
        });
    }

    if let Some(reduction) = crate::engine::static_instant_sorcery_reduction_for_v1(name) {
        walk.rec(
            "static_instant_sorcery_generic_reduction",
            json!({"generic": reduction}),
        );
        walk.ability(CtxF::Static, |out| {
            for card_type in [CardTypeF::Instant, CardTypeF::Sorcery] {
                out.effect(
                    EffectAtom::new(EvF::CostChange)
                        .player(RelF::You)
                        .obj(ObjF::Typed(card_type))
                        .amount(AmtF::fixed(-i64::from(reduction)))
                        .duration(DurF::WhileOnBattlefield),
                );
            }
        });
    }

    if let Some(boost) = crate::engine::static_controlled_creature_boost_for_v1(name) {
        let keyword = match boost.filter {
            crate::engine::StaticControlledCreatureFilterV1::All => None,
            crate::engine::StaticControlledCreatureFilterV1::WithKeyword(keyword) => {
                Some(keyword.0)
            }
        };
        walk.rec(
            "static_controlled_creature_boost",
            json!({"keyword": keyword, "exclude_source": boost.exclude_source, "power": boost.power, "toughness": boost.toughness}),
        );
        if keyword.is_some() || boost.exclude_source {
            walk.opaque
                .push("static team predicate: effective keyword or exact source exclusion");
        }
        walk.ability(CtxF::Static, |out| {
            if keyword.is_some() {
                out.control(ControlF::Conditional);
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Typed(CardTypeF::Creature)),
                    AggF::Characteristic,
                );
            }
            if keyword.is_some() || boost.exclude_source {
                out.atoms.push(Atom::Opaque);
            }
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .player(RelF::You)
                    .obj(ObjF::Typed(CardTypeF::Creature))
                    .amount(AmtF::stat(
                        i64::from(boost.power),
                        i64::from(boost.toughness),
                    ))
                    .duration(DurF::WhileOnBattlefield),
            );
        });
    }

    if let Some((minimum, keywords)) = crate::engine::static_graveyard_threshold_keyword_for(name) {
        walk.rec(
            "static_graveyard_threshold_keyword",
            json!({"minimum_cards": minimum, "keywords": keywords.0}),
        );
        walk.ability(CtxF::Static, |out| {
            out.control(ControlF::Conditional);
            out.read(
                RelF::You,
                Some(ZoneF::Graveyard),
                Some(ObjF::AnyCard),
                AggF::AtLeast(bucket(i64::from(minimum))),
            );
            for bit in keyword_bits(keywords) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .obj(ObjF::ThisObject)
                        .duration(DurF::WhileOnBattlefield)
                        .keyword(bit),
                );
            }
        });
    }

    if crate::continuous_characteristics_v1::has_printed_cant_block(name) {
        walk.rec(
            "static_cant_block",
            json!({"printed_source_abilities": true}),
        );
        walk.ability(CtxF::Static, |out| {
            out.effect(
                EffectAtom::new(EvF::Restrict)
                    .obj(ObjF::ThisObject)
                    .duration(DurF::WhileOnBattlefield),
            );
            // Restrict has no predicate distinguishing blocking from other actions.
            out.atoms.push(Atom::Opaque);
        });
        walk.opaque.push("printed source cannot block");
    }

    #[cfg(feature = "standard-magezero-fixtures")]
    standard_statics(name, &mut walk);
    #[cfg(feature = "standard-magezero-fixtures")]
    standard_keyword_statics(name, &mut walk);

    // Permanents with a continuous effect on their host.
    if let Some(equip) = equipment {
        walk.rec("equipment", Value::String(format!("{equip:?}")));
        let crate::card_def::EquipmentDef {
            power_delta,
            toughness_delta,
            pt_controller_turn_only,
            add_subtype,
            controller_turn_keywords,
            other_turn_keywords,
            noncreature_spell_damage_to_each_opponent,
            job_select,
            granted_activated_ability,
        } = *equip;
        walk.ability(CtxF::Static, |out| {
            if power_delta != 0 || toughness_delta != 0 {
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .obj(ObjF::AttachedObject)
                        .amount(AmtF::stat(
                            i64::from(power_delta),
                            i64::from(toughness_delta),
                        ))
                        .duration(DurF::WhileOnBattlefield),
                );
            }
            if pt_controller_turn_only {
                out.control(ControlF::Conditional);
            }
            if add_subtype.is_some() {
                out.effect(EffectAtom::new(EvF::SetCharacteristic).obj(ObjF::AttachedObject));
            }
            for bit in keyword_bits(controller_turn_keywords | other_turn_keywords) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .obj(ObjF::AttachedObject)
                        .duration(DurF::WhileOnBattlefield)
                        .keyword(bit),
                );
            }
            if noncreature_spell_damage_to_each_opponent > 0 {
                triggers_costs::trigger_condition(
                    crate::trigger::TriggerCondition::CastNoncreatureSpell,
                    out,
                );
                out.effect(
                    EffectAtom::new(EvF::Damage)
                        .player(RelF::EachOpponent)
                        .obj(ObjF::Player)
                        .amount(AmtF::fixed(i64::from(
                            noncreature_spell_damage_to_each_opponent,
                        ))),
                );
            }
            if job_select {
                out.effect(EffectAtom::new(EvF::CreateToken).player(RelF::You));
                out.effect(EffectAtom::new(EvF::Attach).obj(ObjF::Token));
            }
        });
        if let Some(granted) = granted_activated_ability {
            let program = (granted.effect)();
            walk.rec(
                "granted_activated",
                json!({"cost": components_record(granted.cost), "target_spec": granted.target_spec, "program": program_value(&program)}),
            );
            let env = Env {
                target_spec: granted.target_spec,
            };
            let cost = granted.cost;
            let spec = granted.target_spec;
            walk.ability(CtxF::Granted, |out| {
                for &component in cost {
                    triggers_costs::cost_component(component, out);
                }
                meaning::effect_op(&program, &env, out);
                targets::target_spec(spec, out);
            });
        }
    }
    if let Some(aura) = attachment {
        walk.rec("attachment", Value::String(format!("{aura:?}")));
        walk.ability(CtxF::Static, |out| attachment_facts(*aura, out));
    }
    if let Some(face) = transform_face {
        // `face.name` is identity, not rules.
        walk.rec(
            "transform_face",
            json!({
                "types": face.types.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
                "subtypes": face.subtypes.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>(),
                "colors": face.colors.iter().map(|c| format!("{c:?}")).collect::<Vec<_>>(),
                "power": face.power,
                "toughness": face.toughness,
                "keywords": face.keywords.0,
            }),
        );
        walk.ability(CtxF::Static, |out| {
            out.effect(EffectAtom::new(EvF::Transform).obj(ObjF::ThisObject));
        });
    }

    let mut created_tokens = walk.tokens;
    created_tokens.sort_unstable();
    created_tokens.dedup();
    let mut referenced_cards = walk.referenced;
    referenced_cards.sort_unstable();
    referenced_cards.dedup();
    CardRulesV1 {
        card_id,
        printed,
        abilities: walk.abilities,
        created_tokens,
        referenced_cards,
        opaque_rules: walk.opaque,
        record: Value::Object(walk.record),
    }
}

fn printed_form(walk: &mut Walk, key: &str, cost: &Cost, types: &[crate::card_def::CardType]) {
    walk.rec(
        key,
        json!({
            "cost": cost_record(cost),
            "types": types.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
        }),
    );
}

fn optional_cost_facts(cost: crate::card_def::OptionalAdditionalCostDef, out: &mut Collector) {
    out.control(ControlF::Optional);
    match cost {
        crate::card_def::OptionalAdditionalCostDef::CollectEvidence { minimum_mana_value } => out
            .cost(CostAtom::MoveObject {
                from: ZoneF::Graveyard,
                to: ZoneF::Exile,
                obj: ObjF::AnyCard,
                amount: AmtF::fixed(i64::from(minimum_mana_value)),
            }),
        crate::card_def::OptionalAdditionalCostDef::Bargain => out.cost(CostAtom::MoveObject {
            from: ZoneF::Battlefield,
            to: ZoneF::Graveyard,
            obj: ObjF::Permanent,
            amount: AmtF::fixed(1),
        }),
        crate::card_def::OptionalAdditionalCostDef::Casualty(minimum_power) => {
            // Sacrifice one creature with power at least `minimum_power`;
            // when paid, casting puts a copy of the spell, with the same
            // targets, on the stack (`engine::copy_spell_keeping_targets`).
            // Vocabulary gap: the moved class carries no power bound.
            let _ = minimum_power;
            out.cost(CostAtom::MoveObject {
                from: ZoneF::Battlefield,
                to: ZoneF::Graveyard,
                obj: ObjF::Typed(CardTypeF::Creature),
                amount: AmtF::fixed(1),
            });
            out.effect(
                EffectAtom::new(EvF::Copy)
                    .player(RelF::You)
                    .obj(ObjF::Spell)
                    .amount(AmtF::fixed(1)),
            );
        }
    }
}

fn attachment_facts(aura: AttachmentDef, out: &mut Collector) {
    match aura {
        AttachmentDef::AuraCreature { prevents_untap } => {
            out.effect(EffectAtom::new(EvF::Attach).obj(ObjF::Typed(CardTypeF::Creature)));
            if prevents_untap {
                out.effect(
                    EffectAtom::new(EvF::Restrict)
                        .obj(ObjF::AttachedObject)
                        .duration(DurF::WhileOnBattlefield),
                );
            }
        }
        AttachmentDef::AuraCreatureOverride(over) => {
            let crate::card_def::CreatureCharacteristicsOverrideDef {
                name: _, // The overriding name is identity, not rules.
                subtype: _,
                colors: _,
                power,
                toughness,
                loses_abilities,
            } = over;
            out.effect(EffectAtom::new(EvF::Attach).obj(ObjF::Typed(CardTypeF::Creature)));
            out.effect(
                EffectAtom::new(EvF::SetCharacteristic)
                    .obj(ObjF::AttachedObject)
                    .amount(AmtF::stat(i64::from(power), i64::from(toughness)))
                    .duration(DurF::WhileOnBattlefield),
            );
            if loses_abilities {
                out.effect(
                    EffectAtom::new(EvF::Restrict)
                        .obj(ObjF::AttachedObject)
                        .duration(DurF::WhileOnBattlefield),
                );
            }
        }
        AttachmentDef::AuraCreatureStatic(stat) => {
            let crate::card_def::AuraCreatureStaticDef {
                power,
                toughness,
                keywords,
                per_controlled_subtype,
            } = stat;
            out.effect(EffectAtom::new(EvF::Attach).obj(ObjF::Typed(CardTypeF::Creature)));
            let amount = if per_controlled_subtype.is_some() {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Permanent),
                    AggF::Count,
                );
                AmtF::Dynamic
            } else {
                AmtF::stat(i64::from(power), i64::from(toughness))
            };
            out.effect(
                EffectAtom::new(EvF::StatChange)
                    .obj(ObjF::AttachedObject)
                    .amount(amount)
                    .duration(DurF::WhileOnBattlefield),
            );
            for bit in keyword_bits(keywords) {
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .obj(ObjF::AttachedObject)
                        .duration(DurF::WhileOnBattlefield)
                        .keyword(bit),
                );
            }
        }
    }
}

/// The MageZero Standard keyword rules `standard_keywords_v1` keys by name:
/// its statics, and each Spree mode set's surcharge and resolution prelude.
#[cfg(feature = "standard-magezero-fixtures")]
fn standard_keyword_statics(name: &str, walk: &mut Walk) {
    use crate::standard_keywords_v1::StandardKeywordStaticV1;
    for fact in crate::standard_keywords_v1::rules_vector_statics(name) {
        walk.rec(
            "standard_keyword_static",
            Value::String(format!("{fact:?}")),
        );
        walk.ability(CtxF::Static, |out| match *fact {
            StandardKeywordStaticV1::StartYourEnginesMaxSpeedDoubleStrike => {
                // Vocabulary gap: no speed designation or read; the max-speed
                // gate is marked conditional and the speed rules opaque.
                out.atoms.push(Atom::Opaque);
                out.control(ControlF::Conditional);
                for bit in keyword_bits(crate::card_def::Keywords::DOUBLE_STRIKE) {
                    out.effect(
                        EffectAtom::new(EvF::GrantKeyword)
                            .player(RelF::You)
                            .obj(ObjF::ThisObject)
                            .duration(DurF::WhileOnBattlefield)
                            .keyword(bit),
                    );
                }
            }
            StandardKeywordStaticV1::CantBlock => out.effect(
                EffectAtom::new(EvF::Restrict)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .duration(DurF::WhileOnBattlefield),
            ),
            StandardKeywordStaticV1::DayboundNightbound => {
                // Transforms as the day/night designation changes.
                // Vocabulary gap: no day/night read.
                out.control(ControlF::Conditional);
                out.effect(EffectAtom::new(EvF::Transform).obj(ObjF::ThisObject));
            }
            StandardKeywordStaticV1::PlusOnePerControlledForest => {
                // Vocabulary gap: ObjF has no subtype class; nearest is the
                // controller's lands.
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Typed(CardTypeF::Land)),
                    AggF::Count,
                );
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(RelF::You)
                        .obj(ObjF::ThisObject)
                        .amount(AmtF::Dynamic)
                        .duration(DurF::WhileOnBattlefield),
                );
            }
        });
    }
    for mode in 0..3u8 {
        let Some(extra) = crate::standard_keywords_v1::spree_extra_generic(name, mode) else {
            continue;
        };
        let prelude = crate::standard_keywords_v1::spree_mode_prelude(name, mode);
        walk.rec(
            "spree_mode",
            json!({
                "mode": mode,
                "extra_generic": extra,
                "prelude": prelude.as_ref().map(program_value),
            }),
        );
        let env = Env {
            target_spec: TargetSpec::None,
        };
        walk.ability(CtxF::Mode, |out| {
            // The mode set's `+{N}` surcharge on top of the mana cost.
            // Vocabulary gap: `CostAtom::Mana` carries no amount.
            out.cost(CostAtom::Mana);
            if let Some(op) = &prelude {
                meaning::effect_op(op, &env, out);
            }
        });
    }
}

/// The MageZero Standard statics `standard_statics_v1` keys by name.
#[cfg(feature = "standard-magezero-fixtures")]
fn standard_statics(name: &str, walk: &mut Walk) {
    use crate::standard_statics_v1::StandardStaticV1;
    for fact in crate::standard_statics_v1::rules_vector_statics(name) {
        walk.rec("standard_static", Value::String(format!("{fact:?}")));
        walk.ability(CtxF::Static, |out| match *fact {
            StandardStaticV1::ConditionalSelfKeywords(keywords) => {
                // While its controller's turn or its counter count holds.
                // Vocabulary gap: neither condition has a read.
                out.control(ControlF::Conditional);
                for bit in keyword_bits(keywords) {
                    out.effect(
                        EffectAtom::new(EvF::GrantKeyword)
                            .player(RelF::You)
                            .obj(ObjF::ThisObject)
                            .duration(DurF::WhileOnBattlefield)
                            .keyword(bit),
                    );
                }
            }
            StandardStaticV1::EntersWithPlusOneCounterIfControlsManaValueFour => {
                // Vocabulary gap: the read carries no mana-value bound.
                out.control(ControlF::Conditional);
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Permanent),
                    AggF::Any,
                );
                for ev in [EvF::PlaceCounter, EvF::StatChange] {
                    out.effect(
                        EffectAtom::new(ev)
                            .player(RelF::You)
                            .obj(ObjF::ThisObject)
                            .amount(AmtF::fixed(1))
                            .duration(DurF::Permanent),
                    );
                }
            }
            StandardStaticV1::EntersWithOilCounter => out.effect(
                EffectAtom::new(EvF::PlaceCounter)
                    .player(RelF::You)
                    .obj(ObjF::ThisObject)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::Permanent),
            ),
            StandardStaticV1::PlusOnePerOilCounter => {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::ThisObject),
                    AggF::Characteristic,
                );
                out.effect(
                    EffectAtom::new(EvF::StatChange)
                        .player(RelF::You)
                        .obj(ObjF::ThisObject)
                        .amount(AmtF::Dynamic)
                        .duration(DurF::WhileOnBattlefield),
                );
            }
            StandardStaticV1::GrantsWardToOtherHumans => {
                // Vocabulary gap: ward is not a keyword bit and ObjF has no
                // subtype class; nearest is a keyword grant to creatures.
                out.effect(
                    EffectAtom::new(EvF::GrantKeyword)
                        .player(RelF::You)
                        .obj(ObjF::Typed(CardTypeF::Creature))
                        .duration(DurF::WhileOnBattlefield),
                );
            }
            StandardStaticV1::PowerEqualsControlledCreatures => {
                out.read(
                    RelF::You,
                    Some(ZoneF::Battlefield),
                    Some(ObjF::Typed(CardTypeF::Creature)),
                    AggF::Count,
                );
                out.effect(
                    EffectAtom::new(EvF::SetCharacteristic)
                        .obj(ObjF::ThisObject)
                        .amount(AmtF::Dynamic)
                        .duration(DurF::Permanent),
                );
            }
            StandardStaticV1::PowerEqualsGraveyardInstantsAndSorceries => {
                // Vocabulary gap: no two-type union class; both are read.
                for card_type in [CardTypeF::Instant, CardTypeF::Sorcery] {
                    out.read(
                        RelF::You,
                        Some(ZoneF::Graveyard),
                        Some(ObjF::Typed(card_type)),
                        AggF::Count,
                    );
                }
                out.effect(
                    EffectAtom::new(EvF::SetCharacteristic)
                        .obj(ObjF::ThisObject)
                        .amount(AmtF::Dynamic)
                        .duration(DurF::Permanent),
                );
            }
            StandardStaticV1::DoublesOpponentLifeLossOnYourTurn => {
                // A replacement on each opponent life-loss event during the
                // controller's turn; payments are not life loss events here.
                out.control(ControlF::Conditional);
                out.effect(
                    EffectAtom::new(EvF::LifeLoss)
                        .player(RelF::Opponent)
                        .obj(ObjF::Player)
                        .amount(AmtF::Dynamic)
                        .duration(DurF::WhileOnBattlefield),
                );
            }
            StandardStaticV1::NoncreatureSpellsCostOneMore => out.effect(
                // Vocabulary gap: no noncreature class; nearest is any spell.
                EffectAtom::new(EvF::CostChange)
                    .player(RelF::EachPlayer)
                    .obj(ObjF::Spell)
                    .amount(AmtF::fixed(1))
                    .duration(DurF::WhileOnBattlefield),
            ),
            StandardStaticV1::YourInstantsAndSorceriesCostOneLess => {
                for card_type in [CardTypeF::Instant, CardTypeF::Sorcery] {
                    out.effect(
                        EffectAtom::new(EvF::CostChange)
                            .player(RelF::You)
                            .obj(ObjF::Typed(card_type))
                            .amount(AmtF::fixed(-1))
                            .duration(DurF::WhileOnBattlefield),
                    );
                }
            }
            StandardStaticV1::FirstAbilityNeedsOpponentLifeLossThisTurn => {
                // The activated ability's own facts are walked with the
                // card's activated abilities; this is its extra gate.
                out.control(ControlF::Conditional);
                out.read(
                    RelF::Opponent,
                    None,
                    Some(ObjF::Player),
                    AggF::EventThisTurn,
                );
            }
        });
    }
}

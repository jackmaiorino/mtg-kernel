//! Public native cast occurrences and independent optional copy-target slots.
use crate::{
    card_def::{self, CardType, OptionalAdditionalCostDef},
    effect::{thousand_year_storm_v1 as native, EffectOp, EffectTargetSelectionPurpose, PendingEffectChoice},
    engine, event::CommittedEvent as E, ids::{PlayerId, StackItemId},
    omniscience_public_v1::ManaCostV1, optional_trigger_public_v1 as public,
    rl::{PlayerSeatV1, TargetRefV1},
    state::{AbilitySourceContractV4, CastMethodV4, GameState, StackItem, StackItemKind, StackTargetContractV4, Zone},
    tinybones_public_v72::IncarnationV72,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub const PRINTING: u16 = 485;
pub const NAME: &str = "Thousand-Year Storm";
pub const SCHEMA: &str = "mtg-kernel/thousand-year-storm-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/thousand-year-storm/v1";

pub(crate) fn selected_id() -> Result<u16, String> {
    let [trigger] = crate::trigger::triggers_for(PRINTING) else { return Err("Storm trigger roster changed".into()); };
    if card_def::card_id_by_name(NAME) != Some(PRINTING) || !native::definition(PRINTING)
        || trigger.condition != crate::trigger::TriggerCondition::CastInstantOrSorcery
        || trigger.home_zone != Zone::Battlefield || trigger.face_index != 0
        || trigger.intervening_if_kicked || trigger.intervening_if_controls_another_source_card
        || (trigger.effect)() != EffectOp::BindThousandYearStormV1 {
        return Err("complete Storm printing or trigger changed".into());
    }
    Ok(PRINTING)
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut hash = Sha256::new();
        for source in [SCHEMA, MEANING_SCHEMA, include_str!("thousand_year_storm_public_v1.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/thousand_year_storm_v1/casts.rs"),
            include_str!("effect/thousand_year_storm_v1/targets.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"), include_str!("effect.rs"), include_str!("engine.rs"),
            include_str!("state.rs"), include_str!("trigger.rs"), include_str!("../build.rs")] {
            hash.update((source.len() as u64).to_le_bytes()); hash.update(source.as_bytes());
        }
        format!("{:x}", hash.finalize())
    }).clone()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String, pub producer_sha256: String, pub card_db_hash: u64, pub card_id: u16,
    pub mana_cost: ManaCostV1, pub mana_value: u16, pub card_type: CardType,
    pub own_instant_or_sorcery_cast_triggers: bool,
    pub count_prior_own_instant_or_sorcery_casts_this_turn: bool,
    pub includes_casts_before_source_entered: bool, pub copies_are_not_cast: bool,
    pub copy_original_modes_x_kicker_and_targets: bool, pub each_copy_may_choose_new_targets: bool,
    pub each_slot_can_keep_original_target: bool, pub copies_do_not_repay_cast_costs: bool,
}
pub fn printed_meaning() -> Result<MeaningV1, String> {
    let id=selected_id()?;
    Ok(MeaningV1 { schema:MEANING_SCHEMA.into(), producer_sha256:producer_sha256(),
        card_db_hash:card_def::KERNEL_CARDDB_HASH,card_id:id,mana_cost:card_def::CARD_DEFS[id as usize].cost.into(),
        mana_value:6,card_type:CardType::Enchantment,own_instant_or_sorcery_cast_triggers:true,
        count_prior_own_instant_or_sorcery_casts_this_turn:true,includes_casts_before_source_entered:true,
        copies_are_not_cast:true,copy_original_modes_x_kicker_and_targets:true,
        each_copy_may_choose_new_targets:true,each_slot_can_keep_original_target:true,copies_do_not_repay_cast_costs:true })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TargetV1 {
    Player { player: PlayerSeatV1 },
    Object { original: IncarnationV72, current_original: Option<public::ReferenceV1> },
    Stack { relation: u32, original_source: IncarnationV72, kind: StackItemKind, current_position: Option<u32> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotV1 {
    pub original_spell: public::ReferenceV1, pub is_copy: bool,
    pub cast_method: CastMethodV4, pub mode: u8, pub x_value: u16, pub kicked: bool,
    pub optional_additional_cost_paid: Option<OptionalAdditionalCostDef>,
    pub authorized_paid_costs: Vec<PaidCostV1>, pub mana_spent: u16,
    pub targets: Vec<TargetV1>, pub current_stack_position: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaidCostV1 { pub original: IncarnationV72, pub power_lki: Option<i32> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 {
    pub spell: SnapshotV1, pub current_turn: bool, pub prior_own_casts: u16,
    pub observers: Vec<public::RootV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 { Held, Ordered, Published, Resolving }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyV1 { pub ordinal: u16, pub spell: SnapshotV1 }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub chooser: PlayerSeatV1, pub copy_ordinal: u16, pub target_slot: u16,
    pub selected_prefix: Vec<TargetV1>, pub original_target: TargetV1,
    pub legal_new_targets: Vec<TargetRefV1>, pub can_keep_original: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV1 {
    pub cast: u32, pub source: public::RootV1, pub expected_copies: u16,
    pub stage: Option<StageV1>, pub position: Option<u32>,
    pub created_copies: Vec<CopyV1>, pub choice: Option<ChoiceV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 {
    pub schema: String, pub viewer: PlayerSeatV1, pub meaning: MeaningV1,
    pub sources: Vec<public::CurrentV1>, pub current_turn_casts: [u16; 2],
    pub casts: Vec<CastV1>, pub triggers: Vec<TriggerV1>,
}
fn width(n: usize) -> Result<u32,String> { u32::try_from(n).map_err(|_| "Storm public width".into()) }
fn current_original(state:&GameState, viewer:PlayerId, original:&IncarnationV72)->Option<public::ReferenceV1> {
    let id=crate::ids::ObjectId(original.object);
    state.objects.try_get(id).filter(|o|o.zone_change_count==original.generation && o.zone==original.zone)
        .and_then(|_|public::reference(state,id,Some(viewer)))
}
fn target(state:&GameState,viewer:PlayerId,contract:StackTargetContractV4,relations:&mut BTreeMap<StackItemId,u32>)->Result<TargetV1,String> {
    Ok(match contract {
        StackTargetContractV4::Player(player)=>TargetV1::Player{player:player.into()},
        StackTargetContractV4::Object{object,owner,controller,zone,zone_change_count,..}=>{
            let original=IncarnationV72{object:object.0,generation:zone_change_count,owner:owner.into(),controller:controller.into(),zone};
            let current_original=current_original(state,viewer,&original);
            TargetV1::Object{original,current_original}
        }
        StackTargetContractV4::StackItem{stack_item_id,source,owner,controller,source_zone,source_zone_change_count,kind,..}=>{
            let next=width(relations.len())?;
            let relation=*relations.entry(stack_item_id).or_insert(next);
            TargetV1::Stack{relation,original_source:IncarnationV72{object:source.0,generation:source_zone_change_count,owner:owner.into(),controller:controller.into(),zone:source_zone},kind,
                current_position:state.stack.iter().position(|row|row.v4.stack_item_id==stack_item_id).map(width).transpose()?}
        }
    })
}
fn snapshot(state:&GameState,viewer:PlayerId,item:&StackItem,relations:&mut BTreeMap<StackItemId,u32>)->Result<SnapshotV1,String> {
    engine::validate_copy_snapshot(state,item)?;
    let source=item.v4.source_contract.ok_or("Storm snapshot source absent")?;
    Ok(SnapshotV1 {
        original_spell:public::ReferenceV1{object:source.source.0,generation:source.zone_change_count,owner:source.owner.into(),controller:source.controller.into(),zone:source.zone,identity:public::IdentityV1::Known{card_id:source.card_def}},
        is_copy:item.is_copy,cast_method:source.cast_method,mode:item.mode_chosen,x_value:item.v4.x_value,kicked:item.kicked,
        optional_additional_cost_paid:item.v4.optional_additional_cost_paid,
        authorized_paid_costs:item.v4.paid_cost_refs.iter().filter(|p|p.visible_to(viewer)).map(|p|PaidCostV1{
            original:IncarnationV72{object:p.object.0,generation:p.zone_change_count,owner:p.owner.into(),controller:p.controller.into(),zone:p.zone},power_lki:p.power_lki,
        }).collect(),mana_spent:item.v4.mana_spent.0,
        targets:item.v4.target_contracts.iter().map(|&c|target(state,viewer,c,relations)).collect::<Result<_,_>>()?,
        current_stack_position:state.stack.iter().position(|r|r.v4.stack_item_id==item.v4.stack_item_id).map(width).transpose()?,
    })
}
fn root(state:&GameState,viewer:PlayerId,source:AbilitySourceContractV4)->public::RootV1 {
    let original=IncarnationV72{object:source.source.0,generation:source.zone_change_count,owner:source.owner.into(),controller:source.controller.into(),zone:source.zone};
    public::RootV1{
        original_source:public::ReferenceV1{object:original.object,generation:original.generation,owner:original.owner,controller:original.controller,zone:original.zone,identity:public::IdentityV1::Known{card_id:PRINTING}},
        current_original_source:current_original(state,viewer,&original),captured_controller:source.controller.into(),
    }
}
pub(crate) fn validate_native(state:&GameState)->Result<(),String> {
    selected_id()?;
    if !state.thousand_year_storm_consumer_v1 { return Err("Storm owning profile absent".into()); }
    native::validate_state(state)?;
    for item in &state.stack { if native::selected(state,item) { native::validate_published(state,item)?; } }
    if let Some(p)=&state.engine.pending_effect { native::validate_continuation(state,p)?; }
    Ok(())
}
pub(crate) fn capture(state:&GameState,viewer:PlayerId)->Result<FactsV1,String> {
    validate_native(state)?;
    let history=&state.engine.event_history;
    let turn_start=history.iter().rposition(|e|matches!(e,E::LifeLossTurnBeganV1{..})).map_or(0,|i|i+1);
    let mut relations=BTreeMap::new();let mut positions=BTreeMap::new();let mut casts=vec![];let mut counts=[0u16;2];
    for (at,e) in history.iter().enumerate() {
        if let E::ThousandYearStormV1{journal:native::JournalV1::Cast{..}}=e {
            let (spell,observers)=native::cast_at(state,at as u64)?;
            positions.insert(at as u64,width(casts.len())?);
            let current_turn=at>=turn_start;
            if current_turn {counts[spell.controller.index()]=counts[spell.controller.index()].checked_add(1).ok_or("Storm cast count overflow")?;}
            casts.push(CastV1{spell:snapshot(state,viewer,spell,&mut relations)?,current_turn,prior_own_casts:native::count(state,at as u64)?,
                observers:observers.iter().map(|&s|root(state,viewer,s)).collect()});
        }
    }
    let mut triggers=vec![];
    for (position,pending) in public::held(state).into_iter().enumerate() {
        if let Some(cast)=native::capture(&pending.effect) {
            engine::validate_pending_trigger_identity(state,pending)?;
            if pending.placement_ordered {engine::validate_pending_trigger(state,pending)?;}
            let source=pending.source_contract.ok_or("Storm pending source absent")?;
            if !native::cast_at(state,cast)?.1.contains(&source)||pending.effect!=native::program(state,cast)? {
                return Err("Storm pending original capture changed".into());
            }
            triggers.push(TriggerV1{cast:*positions.get(&cast).ok_or("Storm pending cast absent")?,source:root(state,viewer,source),expected_copies:native::count(state,cast)?,
                stage:Some(if pending.placement_ordered{StageV1::Ordered}else{StageV1::Held}),position:Some(width(position)?),created_copies:vec![],choice:None});
        }
    }
    for e in history {
        let E::ThousandYearStormV1{journal:native::JournalV1::Published{cast,item}}=e else{continue};
        native::validate_published(state,item)?;
        let mut row=TriggerV1{cast:*positions.get(cast).ok_or("Storm published cast absent")?,source:root(state,viewer,item.v4.ability_source_contract.ok_or("Storm source absent")?),
            expected_copies:native::count(state,*cast)?,stage:None,position:None,created_copies:vec![],choice:None};
        if let Some(pos)=state.stack.iter().position(|r|r.v4.stack_item_id==item.v4.stack_item_id) {row.stage=Some(StageV1::Published);row.position=Some(width(pos)?);}
        for e in history {
            if let E::ThousandYearStormV1{journal:native::JournalV1::Copy{trigger,ordinal,item:copy}}=e {
                if *trigger==item.v4.stack_item_id {row.created_copies.push(CopyV1{ordinal:*ordinal,spell:snapshot(state,viewer,copy,&mut relations)?});}
            }
        }
        if let Some(p)=state.engine.pending_effect.as_ref().filter(|p|p.resolving_item.v4.stack_item_id==item.v4.stack_item_id) {
            row.stage=Some(StageV1::Resolving);row.position=None;
            if let Some(PendingEffectChoice::SelectTargets{player,selected,legal,min_targets,max_targets,purpose:EffectTargetSelectionPurpose::CopyTarget{choice},..})=&p.choice {
                if !selected.is_empty()||*max_targets!=1||*min_targets>1 {return Err("Storm copy slot selection shape changed".into());}
                if *player==viewer {
                    row.choice=Some(ChoiceV1{chooser:(*player).into(),copy_ordinal:*choice.path.first().ok_or("Storm copy ordinal absent")?,
                        target_slot:u16::try_from(choice.targets.len()).map_err(|_|"Storm target width")?,
                        selected_prefix:choice.contracts.iter().map(|&c|target(state,viewer,c,&mut relations)).collect::<Result<_,_>>()?,
                        original_target:target(state,viewer,*choice.spell.v4.target_contracts.get(choice.targets.len()).ok_or("Storm original target slot absent")?,&mut relations)?,
                        legal_new_targets:legal.iter().map(|c|crate::rl::target_ref(state,c.target).map_err(|e|e.0)).collect::<Result<_,_>>()?,
                        can_keep_original:*min_targets==0});
                }
            }
        }
        triggers.push(row);
    }
    Ok(FactsV1{schema:SCHEMA.into(),viewer:viewer.into(),meaning:printed_meaning()?,
        sources:public::current_tinybones_compat_v72(state,PRINTING)?,current_turn_casts:counts,casts,triggers})
}

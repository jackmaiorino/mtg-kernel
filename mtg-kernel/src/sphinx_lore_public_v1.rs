//! Exact Sphinx attack targets and incarnation-bound temporary flashback routes.
use crate::{card_def::{self, CardType, CostComponent, Subtype},
    effect::{sphinx_lore_v1 as lore, EffectOp}, engine::{self, sphinx_lore_cast_v1 as casting},
    event::CommittedEvent as E, ids::PlayerId, optional_trigger_public_v1 as public,
    omniscience_public_v1::ManaCostV1, rl::PlayerSeatV1,
    state::{GameState, StackTargetContractV4, StackItem, CastMethodV4, Zone},
    tinybones_public_v72::{CountersV72, IncarnationV72}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const PRINTING: u16 = 477;
pub const NAME: &str = "Sphinx of Forgotten Lore";
pub const SCHEMA: &str = "mtg-kernel/sphinx-lore-public/v1";
const MEANING_SCHEMA: &str = "rules-meaning/sphinx-lore/v1";
pub(crate) fn selected_id() -> Result<u16, String> {
    let [trigger] = crate::trigger::triggers_for(PRINTING) else { return Err("Sphinx trigger roster changed".into()); };
    if card_def::card_id_by_name(NAME) != Some(PRINTING) || !lore::definition(PRINTING)
        || trigger.condition != crate::trigger::TriggerCondition::Attacks || trigger.home_zone != Zone::Battlefield
        || trigger.face_index != 0 || trigger.intervening_if_kicked || trigger.intervening_if_controls_another_source_card
        || (trigger.effect)() != lore::marker() {
        return Err("complete Sphinx printing or attack trigger changed".into());
    }
    Ok(PRINTING)
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| { let mut hash = Sha256::new();
        for source in [SCHEMA, MEANING_SCHEMA, include_str!("sphinx_lore_public_v1.rs"),
            include_str!("effect/sphinx_lore_v1.rs"), include_str!("engine/sphinx_lore_cast_v1.rs"),
            include_str!("trigger.rs"), include_str!("engine.rs"), include_str!("spell_costs_v1.rs"), include_str!("../build.rs")] {
            hash.update((source.len() as u64).to_le_bytes()); hash.update(source.as_bytes());
        } format!("{:x}",hash.finalize()) }).clone()
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String, pub producer_sha256: String, pub card_db_hash: u64, pub card_id: u16,
    pub mana_cost: ManaCostV1, pub mana_value: u16, pub card_type: CardType, pub subtype: Subtype,
    pub power: i16, pub toughness: i16, pub flying: bool, pub flash: bool,
    pub attacks_target_own_graveyard_instant_or_sorcery: bool,
    pub grants_flashback_until_cleanup: bool, pub flashback_cost_equals_printed_mana_cost: bool,
    pub retains_normal_timing_targets_and_additional_costs: bool,
    pub granted_flashback_exiles_on_stack_departure: bool, pub grant_requires_original_graveyard_incarnation: bool,
}
pub fn printed_meaning() -> Result<MeaningV1, String> {
    let id = selected_id()?;
    Ok(MeaningV1 { schema: MEANING_SCHEMA.into(), producer_sha256: producer_sha256(), card_db_hash: card_def::KERNEL_CARDDB_HASH,
        card_id: id, mana_cost: card_def::CARD_DEFS[usize::from(id)].cost.into(), mana_value: 4, card_type: CardType::Creature,
        subtype: Subtype::Sphinx, power: 3, toughness: 3, flying: true, flash: true,
        attacks_target_own_graveyard_instant_or_sorcery: true, grants_flashback_until_cleanup: true,
        flashback_cost_equals_printed_mana_cost: true, retains_normal_timing_targets_and_additional_costs: true,
        granted_flashback_exiles_on_stack_departure: true, grant_requires_original_graveyard_incarnation: true })
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermanentV1 { pub card: public::ReferenceV1, pub form: public::FormV1, pub counters: CountersV72,
    pub tapped: bool, pub summoning_sick: bool }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageV1 { Held, Ordered, Published }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV1 { pub original: public::ReferenceV1, pub current_original: Option<public::ReferenceV1>, pub currently_legal: bool }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackV1 { pub source: public::RootV1, pub turn: u32, pub pending: Option<(StageV1,u32)>,
    pub original_target: Option<TargetV1>, pub installed_grant: Option<u32> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantV1 { pub attack: u32, pub holder: PlayerSeatV1, pub original_card: public::ReferenceV1,
    pub current_original_graveyard: Option<public::ReferenceV1>, pub until_cleanup_active: bool,
    pub original_incarnation_has_active_grant: bool, pub mana_cost: ManaCostV1 }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteV1 { Normal, PrintedFlashback, Escape, Granted { grant: u32 } }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostV1 { Mana(ManaCostV1), DiscardCards(u8), SacrificeLands(u8), PayLife(u8), ExileOtherOwnGraveyardCards(u8) }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionV1 { pub route: RouteV1, pub ordered_base_cost: Vec<CostV1> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastStageV1 { Announcing, Published, Resolving }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastV1 { pub spell: public::ReferenceV1, pub original_graveyard: IncarnationV72,
    pub stage: CastStageV1, pub native_stack_position: Option<u32>, pub routes: Vec<OptionV1>,
    pub selected: Option<u16>, pub finalized_method: Option<CastMethodV4> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV1 { pub schema: String, pub viewer: PlayerSeatV1, pub meaning: MeaningV1,
    pub battlefield: Vec<PermanentV1>, pub attacks: Vec<AttackV1>, pub grants: Vec<GrantV1>, pub casts: Vec<CastV1> }
fn width(n: usize) -> Result<u32, String> { u32::try_from(n).map_err(|_| "Sphinx public width".into()) }
fn original_target(contract: StackTargetContractV4) -> Result<public::ReferenceV1, String> {
    let StackTargetContractV4::Object { object, card_def, owner, controller, zone, zone_change_count, .. } = contract
        else { return Err("Sphinx target is not a card".into()); };
    if zone != Zone::Graveyard || !card_def::CARD_DEFS.get(usize::from(card_def)).is_some_and(|d|
        !d.is_token && d.types.iter().any(|t| matches!(t,CardType::Instant|CardType::Sorcery))) {
        return Err("Sphinx original graveyard printing changed".into());
    }
    Ok(public::ReferenceV1 { object: object.0, generation: zone_change_count, owner: owner.into(), controller: controller.into(),
        zone, identity: public::IdentityV1::Known { card_id: card_def } })
}
fn current_original(state: &GameState, viewer: PlayerId, reference: &public::ReferenceV1) -> Option<public::ReferenceV1> {
    let id = crate::ids::ObjectId(reference.object);
    state.objects.try_get(id).filter(|o| o.zone == reference.zone && o.zone_change_count == reference.generation
        && state.players[o.owner.index()].graveyard.contains(&id)).and_then(|_| public::reference(state,id,Some(viewer)))
}
fn target(state: &GameState, viewer: PlayerId, item: &StackItem) -> Result<Option<TargetV1>, String> {
    let Some(contract) = item.v4.target_contracts.first() else { return Ok(None); };
    let original = original_target(*contract)?;
    let current_original = current_original(state,viewer,&original);
    let source = item.v4.ability_source_contract.ok_or("Sphinx original source absent")?;
    Ok(Some(TargetV1 { currently_legal: current_original.is_some()
        && engine::target_contract_matches_live(state,item.targets[0],*contract)
        && engine::effect_target_is_legal_from_ability_source(state,source,item.controller,lore::target_spec(),&item.targets,0),
        original,current_original }))
}
pub(crate) fn costs(values: &[CostComponent]) -> Result<Vec<CostV1>,String> {
    values.iter().map(|cost| Ok(match cost {
        CostComponent::Mana(m) => CostV1::Mana((*m).into()), CostComponent::DiscardCards(n) => CostV1::DiscardCards(*n),
        CostComponent::SacrificeLands(n) => CostV1::SacrificeLands(*n), CostComponent::PayLife(n) => CostV1::PayLife(*n),
        CostComponent::ExileOtherCardsFromOwnGraveyard(n) => CostV1::ExileOtherOwnGraveyardCards(*n),
        _ => return Err("Sphinx route has an unrepresented printed cost component".into()),
    })).collect()
}
pub(crate) fn options(id: u16, routes: &[casting::RouteV1], grants: &std::collections::BTreeMap<u64,u32>) -> Result<Vec<OptionV1>,String> {
    let d=card_def::CARD_DEFS.get(usize::from(id)).ok_or("Sphinx route printing absent")?;
    routes.iter().map(|route| {
        let (route,ordered_base_cost)=match route {
            casting::RouteV1::Normal => (RouteV1::Normal,vec![CostV1::Mana(d.cost.into())]),
            casting::RouteV1::PrintedFlashback => (RouteV1::PrintedFlashback,costs(d.flashback.ok_or("Sphinx printed flashback absent")?.cost)?),
            casting::RouteV1::Escape => (RouteV1::Escape,costs(d.escape.ok_or("Sphinx printed escape absent")?.cost)?),
            casting::RouteV1::Granted { installation } => (RouteV1::Granted {grant:*grants.get(installation).ok_or("Sphinx route original grant absent")?},vec![CostV1::Mana(d.cost.into())]),
        }; Ok(OptionV1 {route,ordered_base_cost})
    }).collect()
}
fn cast(state:&GameState,viewer:PlayerId,item:&StackItem,stage:CastStageV1,position:Option<usize>,
    choice:casting::ChoiceV1,grants:&std::collections::BTreeMap<u64,u32>)->Result<CastV1,String> {
    let options=casting::options_record(state,choice.options_history_index)?;
    let spell=public::reference(state,item.source,Some(viewer)).ok_or("Sphinx current cast absent")?;
    if options.spell!=item.source || options.controller!=item.controller {return Err("Sphinx cast original options changed".into());}
    Ok(CastV1 {spell,original_graveyard:IncarnationV72 {object:options.spell.0,generation:options.graveyard_generation,
        owner:options.controller.into(),controller:options.controller.into(),zone:Zone::Graveyard},stage,
        native_stack_position:position.map(width).transpose()?,routes:self::options(options.card_def,&options.routes,grants)?,
        selected:choice.selected,finalized_method:if stage==CastStageV1::Announcing {None}else{item.v4.cast_method}})
}
pub(crate) fn capture(state:&GameState,viewer:PlayerId)->Result<FactsV1,String> {
    let meaning=printed_meaning()?; lore::validate_state(state)?;
    let mut battlefield=vec![];
    for(id,o)in state.objects.iter() {if o.zone==Zone::Battlefield&&public::visible_known(state,id,Some(viewer),|id|id==PRINTING){
        battlefield.push(PermanentV1{card:public::reference(state,id,Some(viewer)).ok_or("Sphinx permanent absent")?,form:public::checked_form(state,id)?,
            counters:crate::tinybones_public_v72::counters(state,id)?,tapped:o.tapped,summoning_sick:o.summoning_sick});}}
    let history=state.engine.event_history.scan_for_v1(&[crate::event_history_v1::EventKindV1::SphinxLoreV1]);
    let mut attacks=vec![];let mut attack_positions=std::collections::BTreeMap::new();
    for(index,entry)in history.iter().enumerate(){if let E::SphinxLoreV1{journal:lore::JournalV1::Attack{turn,..}}=entry{
        let source = lore::source_at(state,index as u64)?;
        let original_source = public::ReferenceV1 {object:source.source.0,generation:source.zone_change_count,
            owner:source.owner.into(),controller:source.controller.into(),zone:source.zone,identity:public::IdentityV1::Known {card_id:source.card_def}};
        let current_original_source = state.objects.try_get(source.source).filter(|o|o.zone_change_count==source.zone_change_count)
            .and_then(|_|public::reference(state,source.source,Some(viewer)));
        let source = public::RootV1 {original_source,current_original_source,captured_controller:source.controller.into()};
        attack_positions.insert(index as u64,width(attacks.len())?); attacks.push(AttackV1{source,turn:*turn,
            pending:None,original_target:None,installed_grant:None});}}
    let active=lore::active_at(state,state.engine.event_history.len())?;
    let mut grants=vec![];let mut grant_positions=std::collections::BTreeMap::new();
    for(index,entry)in history.iter().enumerate(){if let E::SphinxLoreV1{journal:lore::JournalV1::Installed{attack,target,..}}=entry{
        let(binding,holder,_)=lore::installation(state,index as u64)?;
        let attack=*attack_positions.get(attack).ok_or("Sphinx grant attack absent")?;
        let original_card=original_target(*target)?;let current_original_graveyard=current_original(state,viewer,&original_card);
        let public::IdentityV1::Known{card_id}=original_card.identity else{unreachable!()};
        let ordinal=width(grants.len())?;grant_positions.insert(index as u64,ordinal);attacks[attack as usize].installed_grant=Some(ordinal);
        grants.push(GrantV1{attack,holder:holder.into(),original_card,current_original_graveyard,
            until_cleanup_active:active.contains(&(index as u64)),original_incarnation_has_active_grant:lore::active_grants(state,binding.object,holder).contains(&(index as u64)),
            mana_cost:card_def::CARD_DEFS[usize::from(card_id)].cost.into()});}}
    for entry in history {if let E::SphinxLoreV1{journal:lore::JournalV1::Published{attack,item}}=entry{
        attacks[*attack_positions.get(attack).ok_or("Sphinx published attack absent")? as usize].original_target=target(state,viewer,item)?;}}
    for(position,pending)in public::held(state).into_iter().enumerate(){
        if let EffectOp::SphinxLoreV1{attack_history_index}=pending.effect {
            engine::validate_pending_trigger_identity(state,pending)?;if pending.placement_ordered {engine::validate_pending_trigger(state,pending)?;}
            lore::validate_root(state,&pending.effect,pending.source_contract)?;
            attacks[*attack_positions.get(&attack_history_index).ok_or("Sphinx pending attack absent")? as usize].pending=Some((if pending.placement_ordered{StageV1::Ordered}else{StageV1::Held},width(position)?));
        }
    }
    let mut casts=vec![];
    for(position,item)in state.stack.iter().enumerate(){
        if let Some(EffectOp::SphinxLoreV1{attack_history_index})=item.inline_effect{
            lore::validate_published(state,item)?;
            attacks[*attack_positions.get(&attack_history_index).ok_or("Sphinx live attack absent")? as usize].pending=Some((StageV1::Published,width(position)?));
        }
        if let Some(choice)=casting::published_choice(state,item)? {casts.push(cast(state,viewer,item,CastStageV1::Published,Some(position),choice,&grant_positions)?);}
    }
    if let Some(pending)=&state.engine.pending_cast {if let Some(choice)=pending.sphinx_lore_v1 {
        engine::validate_pending_cast(state,pending)?;
        let item=state.stack.last().ok_or("Sphinx cast placeholder absent")?;
        // The placeholder is not yet a paid published cast.
        casts.retain(|row|row.spell.object!=pending.spell.0);
        casts.push(cast(state,viewer,item,CastStageV1::Announcing,Some(state.stack.len()-1),choice,&grant_positions)?);
    }}
    if let Some(pending)=&state.engine.pending_effect {if let Some(choice)=casting::published_choice(state,&pending.resolving_item)? {
        crate::effect::validate_pending_effect_choice(state)?;
        casts.push(cast(state,viewer,&pending.resolving_item,CastStageV1::Resolving,None,choice,&grant_positions)?);
    }}
    Ok(FactsV1{schema:SCHEMA.into(),viewer:viewer.into(),meaning,battlefield,attacks,grants,casts})
}

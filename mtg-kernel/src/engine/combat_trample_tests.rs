use super::*;
use crate::state::{Counters, GameObject};

fn put(s:&mut GameState,p:PlayerId,name:&str)->ObjectId {
    let card_def=card_def::card_id_by_name(name).unwrap();
    let id=s.objects.push(GameObject{card_def,name:name.into(),owner:p,controller:p,zone:Zone::Battlefield,
        tapped:false,summoning_sick:false,damage:0,counters:Counters::default(),attachments:vec![],
        v4:ObjectStateV4::from_card_def(card_def),spell_copy_origin:None,plotted_turn:None,zone_change_count:0});
    s.players[p.index()].battlefield.push(id);id
}
fn setup(seat:u8,name:&str,blockers:&[&str])->(GameState,ObjectId,Vec<ObjectId>) {
    let actor=PlayerId(seat);let mut s=GameState::new_from_libraries(&[],&[],crate::rl::card_name,57);
    s.active_player=actor;s.priority_player=actor;s.step=Step::DeclareBlockers;
    let a=put(&mut s,actor,name);
    let bs=blockers.iter().map(|b|put(&mut s,actor.opponent(),b)).collect::<Vec<_>>();
    s.engine.combat.attackers_declared=true;s.engine.combat.blockers_declared=true;
    s.engine.combat.attackers=vec![a];s.engine.combat.blocked_by=vec![(a,bs.clone())];(s,a,bs)
}
fn grant(s:&mut GameState,a:ObjectId,k:Keywords) {
    s.engine.until_end_of_turn.push(UntilEndOfTurnEffect::ResolvedObjectKeywordEffect {
        object_id:a,object_zone_change_count:s.objects.get(a).zone_change_count,
        layer:Layers::ABILITY_ADDING,timestamp:1,duration:EffectDuration::EndOfTurn,keywords:k});
}
#[test]
fn trample_single_and_multiple_blockers_both_seats() {
    for seat in 0..2 { for (bs,damage) in [(vec!["Myr Enforcer"],1),(vec!["Voldaren Epicure","Voldaren Epicure"],3),(vec!["Myr Enforcer","Voldaren Epicure"],0)] {
        let (mut s,_,_)=setup(seat,"Avenging Hunter",&bs);deal_combat_damage(&mut s);
        assert_eq!(s.players[1-seat as usize].life,20-damage);
    }}
}
#[test]
fn trample_marked_damage_reduces_assignment() {
    let (mut s,_,bs)=setup(0,"Avenging Hunter",&["Myr Enforcer"]);
    s.objects.get_mut(bs[0]).damage=2;deal_combat_damage(&mut s);assert_eq!(s.players[1].life,17);
}
#[test]
fn trample_removed_blocker_and_nontrample_control() {
    for (name,expected) in [("Avenging Hunter",15),("Myr Enforcer",20)] {
        let (mut s,_,bs)=setup(0,name,&["Voldaren Epicure"]);
        event::propose_and_commit(&mut s,ProposedEvent::zone_change(bs[0],Zone::Graveyard));
        deal_combat_damage(&mut s);assert_eq!(s.players[1].life,expected);
    }
}
#[test]
fn trample_deathtouch_assigns_one_to_each_live_blocker() {
    let (mut s,a,_)=setup(0,"Avenging Hunter",&["Myr Enforcer","Myr Enforcer"]);
    grant(&mut s,a,Keywords::DEATHTOUCH);deal_combat_damage(&mut s);assert_eq!(s.players[1].life,17);
}
#[test]
fn trample_double_strike_rechecks_removed_blockers() {
    let (mut s,a,_)=setup(0,"Avenging Hunter",&["Voldaren Epicure"]);
    grant(&mut s,a,Keywords::DOUBLE_STRIKE);deal_combat_damage(&mut s);assert_eq!(s.players[1].life,11);
}
#[test]
fn trample_protection_still_requires_lethal_assignment() {
    let (mut s,_,bs)=setup(0,"Avenging Hunter",&["Guardian of the Guildpact"]);
    deal_combat_damage(&mut s);assert_eq!(s.players[1].life,18);
    assert_eq!(s.objects.get(bs[0]).damage,0);
}
#[test]
fn trample_lifelink_is_simultaneous_with_player_damage() {
    let (mut s,_,bs)=setup(0,"Avenging Hunter",&["Sacred Cat"]);
    s.players[1].life=2;s.objects.get_mut(bs[0]).counters.plus1_plus1=3;s.objects.get_mut(bs[0]).damage=1;
    deal_combat_damage(&mut s);assert_eq!(s.players[1].life,4);assert!(!s.players[1].has_lost);
}

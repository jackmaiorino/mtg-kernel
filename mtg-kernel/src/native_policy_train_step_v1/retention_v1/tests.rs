use super::*;
use crate::native_flat_tensorizer_v4::{encoded_decision_view_v4,NativeFlatDecisionTensorV4,NativeFlatTensorizerV4};
use crate::native_policy_train_step_v1::tests::perturbed_model;

fn tensor() -> NativeFlatDecisionTensorV4 {
    use crate::flat_policy_v2::{FlatScoringDecisionViewV2,FlatScoringOwnedBuffersV2};
    use crate::flat_policy_v4::{FlatDecisionEncoderV4,FlatScoringDecisionViewV4};
    use crate::rl_session::{FastActorResponseV1,FastActorSessionV1};
    let session=FastActorSessionV1::from_v3_fixture_state(crate::policy_observation_v6::tests::map_choice_state().0);
    let FastActorResponseV1::Decision(d)=session.current_response() else {panic!("missing Map choice")};
    let (mut objects,mut relations,mut subtypes,mut uses,mut goads,mut dungeons,mut effects,mut paths,mut actions,mut refs)=
        (Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new(),Vec::new());
    let e=session.encode_current_flat_scoring_decision_owned_v4(d,&mut FlatDecisionEncoderV4::default(),
        &mut FlatScoringOwnedBuffersV2{objects:&mut objects,relations:&mut relations,object_subtypes:&mut subtypes,
            ability_uses:&mut uses,goads:&mut goads,completed_dungeons:&mut dungeons,effect_subtype_changes:&mut effects,
            context_path_elements:&mut paths,actions:&mut actions,action_refs:&mut refs}).unwrap();
    let common=FlatScoringDecisionViewV2::new(&e.globals,&objects,&relations,&subtypes,&uses,&goads,&dungeons,&effects,&paths,&actions,&refs);
    let mut t=NativeFlatDecisionTensorV4::default();NativeFlatTensorizerV4::default().fill(FlatScoringDecisionViewV4::new(common,&e.extensions),&mut t).unwrap();t
}

struct Fixture { tensor:NativeFlatDecisionTensorV4, logits:Vec<u32>, value:u32, parent:Vec<f32> }
impl Fixture {
    fn new(model:&NativePolicyValueNetV1)->Self {
        let tensor=tensor();let s=model.forward_feature_transfer_v4(encoded_decision_view_v4(&tensor)).unwrap();
        assert!(s.logits.len()>=2);
        let parent=s.logits.iter().enumerate().map(|(i,v)|v+0.7*i as f32).collect();
        Self{tensor,logits:s.logits.iter().map(|v|v.to_bits()).collect(),value:s.value.to_bits(),parent}
    }
    fn teaching(&self)->NativePolicySubstepV1<'_> {NativePolicySubstepV1{
        forward:NativePolicyForwardInputV1::Encoded(Box::new(encoded_decision_view_v4(&self.tensor))),selected_action_index:0,
        expected_raw_action_logit_bits:&self.logits,expected_value_bits:self.value}}
    fn retention(&self)->RetentionRowV1<'_> {RetentionRowV1{encoded:encoded_decision_view_v4(&self.tensor),
        expected_current_logits:&self.logits,expected_current_value:self.value,parent_logits:&self.parent}}
}
fn state()->NativePolicyValueTrainStateV1 {
    let model=NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1()).unwrap();
    let s=NativePolicyValueTrainStateV1::new_v1(model).unwrap();
    let mut snapshot=s.snapshot_v1().unwrap();snapshot.adam_step=7;
    snapshot.first_moments.iter_mut().find(|p|p.name=="value_head.2.bias").unwrap().values[0]=0.015;
    snapshot.second_moments.iter_mut().find(|p|p.name=="value_head.2.bias").unwrap().values[0]=0.023;
    NativePolicyValueTrainStateV1::from_snapshot_v1(s.model_v1().clone(),&snapshot).unwrap()
}
fn group<'a>(steps:&'a [NativePolicySubstepV1<'a>])->NativePolicyPhysicalDecisionV1<'a> {
    NativePolicyPhysicalDecisionV1{substeps:steps,terminal_return:1,baseline_bits:0}
}
fn logp(v:&[f32])->Vec<f64> {
    let max=v.iter().copied().map(f64::from).fold(f64::NEG_INFINITY,f64::max);
    let z=v.iter().map(|x|(f64::from(*x)-max).exp()).sum::<f64>().ln();v.iter().map(|x|f64::from(*x)-max-z).collect()
}
fn oracle(model:&NativePolicyValueNetV1,f:&Fixture,beta:f64)->f64 {
    let output=model.forward_feature_transfer_v4(encoded_decision_view_v4(&f.tensor)).unwrap();
    let p=logp(&f.parent);let q=logp(&output.logits);
    // Two retention groups containing2 and1 identical rows: denominator2,
    // rather than three rows or legal-action count.
    -q[0]+beta*1.5*p.iter().zip(q).map(|(p,q)|p.exp()*(p-q)).sum::<f64>()
}

#[test]
fn retention_zero_beta_preserves_legacy_state_and_moments_exactly() {
    let mut old=state();let mut new=old.clone();let f=Fixture::new(old.model_v1());let steps=[f.teaching()];let teaching=[group(&steps)];
    let a=old.train_step_terminal_winner_imitation_v4(&teaching,0.0001,4).unwrap();
    // Empty retention is deliberately ignored at exact zero.
    let b=new.train_step_retained_imitation_v4(&teaching,&[],0.0,0.0001,4).unwrap();
    assert_eq!(a.loss.to_bits(),b.loss.to_bits());assert_eq!(a.gradients,b.gradients);
    assert_eq!(old.snapshot_v1().unwrap(),new.snapshot_v1().unwrap());assert_eq!(b.adam_step,8);
}

#[test]
fn retention_positive_beta_has_direct_loss_gradient_and_worker_parity() {
    let original=state();let f=Fixture::new(original.model_v1());let steps=[f.teaching()];let teaching=[group(&steps)];
    let two=[f.retention(),f.retention()];let one=[f.retention()];let retained=[RetentionGroupV1{rows:&two},RetentionGroupV1{rows:&one}];
    let mut reference=None;
    for workers in [1,2,4] {
        let mut s=original.clone();let r=s.train_step_retained_imitation_v4(&teaching,&retained,0.75,0.0001,workers).unwrap();
        assert!((r.loss as f64-oracle(original.model_v1(),&f,0.75)).abs()<2e-5);
        assert_eq!(s.adam_step_v1(),8);
        if let Some((old,result))=&reference {assert_eq!(old,&s.snapshot_v1().unwrap());assert_eq!(result,&r);}
        else {reference=Some((s.snapshot_v1().unwrap(),r));}
    }
    let (_,r)=reference.unwrap();let mut checked=0;
    for gradient in &r.gradients {
        let (index,&largest)=gradient.values.iter().enumerate().max_by(|a,b|a.1.abs().total_cmp(&b.1.abs())).unwrap();
        if largest.abs()<1e-5 {continue;}
        let epsilon=0.001;
        let plus=perturbed_model(original.model_v1(),gradient.name,index,epsilon);
        let minus=perturbed_model(original.model_v1(),gradient.name,index,-epsilon);
        let expected=(oracle(&plus,&f,0.75)-oracle(&minus,&f,0.75))/(2.0*epsilon as f64);
        assert!((expected-largest as f64).abs()<0.0005+0.03*expected.abs(),"{}: {} vs {}",gradient.name,largest,expected);
        checked+=1;
    }
    assert!(checked>=6,"only {checked} nontrivial parameter tensors checked");
    assert!(r.retention_gauge.unwrap().raw_gradient_residual.abs()<1e-4);
}

#[test]
fn retention_invalid_inputs_do_not_mutate_original_state() {
    let original=state();let f=Fixture::new(original.model_v1());let steps=[f.teaching()];let teaching=[group(&steps)];
    let row=[f.retention()];let good=[RetentionGroupV1{rows:&row}];
    for beta in [-0.1,f32::NAN,f32::INFINITY] {
        let mut s=original.clone();assert!(s.train_step_retained_imitation_v4(&teaching,&good,beta,0.0001,1).is_err());
        assert_eq!(s.snapshot_v1().unwrap(),original.snapshot_v1().unwrap());
    }
    let mut broken=f.logits.clone();broken[0]^=1;let mut wrong=f.retention();wrong.expected_current_logits=&broken;
    let wrong_rows=[wrong];let wrong_group=[RetentionGroupV1{rows:&wrong_rows}];
    let mut s=original.clone();assert!(s.train_step_retained_imitation_v4(&teaching,&wrong_group,0.5,0.0001,1).unwrap_err().contains("replay differs"));
    assert_eq!(s.snapshot_v1().unwrap(),original.snapshot_v1().unwrap());
    for retained in [&[][..],&good[..]] {
        let mut s=original.clone();let workers=if retained.is_empty(){1}else{0};
        assert!(s.train_step_retained_imitation_v4(&teaching,retained,0.5,0.0001,workers).is_err());
        assert_eq!(s.snapshot_v1().unwrap(),original.snapshot_v1().unwrap());
    }
}

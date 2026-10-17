//! Bound nonland copying and untargeted Aura host choice. Existing native
//! definition/source contracts own the operation; no new historical journal.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyTemplateV1 {
    pub card_def: u16,
    pub face_index: u8,
    #[serde(default, skip_serializing_if = "copy_abilities_zero_v1")]
    pub copied_end_step_sacrifices_v1: u32,
    #[serde(default, skip_serializing_if = "copy_nightmare_false_v1")]
    pub copied_nightmare_v1: bool,
}
fn copy_nightmare_false_v1(value: &bool) -> bool {
    !*value
}
fn copy_abilities_zero_v1(value: &u32) -> bool {
    *value == 0
}
impl std::hash::Hash for CopyTemplateV1 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        std::hash::Hash::hash(&self.card_def, h);
        std::hash::Hash::hash(&self.face_index, h);
        if self.copied_end_step_sacrifices_v1 != 0 {
            std::hash::Hash::hash("copied-end-step-sacrifices/v1", h);
            std::hash::Hash::hash(&self.copied_end_step_sacrifices_v1, h);
        }
        if self.copied_nightmare_v1 {
            std::hash::Hash::hash("copied-nightmare/v1", h);
            std::hash::Hash::hash(&self.copied_nightmare_v1, h);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyChoiceV1 {
    pub template: CopyTemplateV1,
    pub candidates: Vec<EffectObjectBinding>,
    pub replacement: crate::doubling_season_v48::ReplacementInputV48,
    pub selected_hosts: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}
pub(super) fn requires_stage(p: &EffectContinuation) -> bool {
    p.resolving_item.inline_effect == Some(EffectOp::CreateAnotherControlledNonlandTargetCopyV1)
        || p.resolving_item
            .v4
            .ability_source_contract
            .is_some_and(|source| {
                crate::card_def::CARD_DEFS
                    .get(usize::from(source.card_def))
                    .is_some_and(|d| d.name == "Extravagant Replication")
            })
}
fn validate_context(state: &GameState, pending: &EffectContinuation) -> Result<(), String> {
    if pending.ctx.stack_item_id != Some(pending.resolving_item.v4.stack_item_id)
        || pending.ctx.source != pending.resolving_item.source
        || pending.ctx.controller != pending.resolving_item.controller
        || pending.ctx.targets != pending.resolving_item.targets
        || pending.ctx.target_contracts != pending.resolving_item.v4.target_contracts
        || pending.ctx.discarded != pending.resolving_item.discarded
        || pending.ctx.paid_cost_refs != pending.resolving_item.v4.paid_cost_refs
        || pending.ctx.hidden_ability_source != pending.resolving_item.v4.hidden_ability_source
        || pending.ctx.kicked != pending.resolving_item.kicked
        || pending.ctx.ability_source_contract != pending.resolving_item.v4.ability_source_contract
        || pending.ctx.cauldron_grant != pending.resolving_item.v4.cauldron_grant
        || pending.ctx.optional_additional_cost_paid
            != pending.resolving_item.v4.optional_additional_cost_paid
    {
        return Err(
            "effect continuation context no longer mirrors its resolving stack item".to_string(),
        );
    }
    crate::engine::validated_stack_item_target_spec(&pending.resolving_item, state)?;
    Ok(())
}
pub(super) fn preflight(
    state: &GameState,
    op: &EffectOp,
    ctx: &ExecCtx,
    item: &StackItem,
) -> Result<(), String> {
    if *op != EffectOp::CreateAnotherControlledNonlandTargetCopyV1 {
        return Ok(());
    }
    let pending = EffectContinuation {
        resolving_item: item.clone(),
        ctx: ctx.clone(),
        frames: vec![],
        choice: None,
        answered_choice_guard: None,
    };
    template(state, &pending)?;
    Ok(())
}
fn template(state: &GameState, p: &EffectContinuation) -> Result<Option<CopyTemplateV1>, String> {
    validate_context(state, p)?;
    let root = validated_definition_owned_root_effect(state, p)?;
    if root.as_ref() != &EffectOp::CreateAnotherControlledNonlandTargetCopyV1 {
        return Err("nonland copy lost its original definition-owned root".into());
    }
    let ([target @ Target::Object(id)], [contract]) =
        (p.ctx.targets.as_slice(), p.ctx.target_contracts.as_slice())
    else {
        return Err("nonland copy lost its single original target".into());
    };
    if !crate::self_reflection_v1::target_matches_live(state, *target, *contract) {
        return Ok(None);
    }
    let source = p
        .ctx
        .ability_source_contract
        .ok_or("nonland copy lost original ability source")?;
    if !crate::engine::effect_target_is_legal_from_ability_source(
        state,
        source,
        p.ctx.controller,
        crate::card_def::TargetSpec::AnotherControlledNonlandPermanent,
        &p.ctx.targets,
        0,
    ) {
        return Ok(None);
    }
    let live = state.objects.get(*id);
    if live.v4.face_down_v1.is_some() {
        let anonymous =
            crate::card_def::card_id_by_name(crate::self_reflection_v1::ANONYMOUS_BLUEPRINT)
                .filter(|id| crate::self_reflection_v1::anonymous_blueprint_is_exact(*id))
                .ok_or("anonymous copy blueprint absent")?;
        return Ok(Some(CopyTemplateV1 {
            card_def: anonymous,
            face_index: 0,
            copied_end_step_sacrifices_v1: 0,
            copied_nightmare_v1: false,
        }));
    }
    let d = &crate::card_def::CARD_DEFS[live.card_def as usize];
    if live.v4.face_index > 1 || live.v4.face_index == 1 && d.transform_face.is_none() {
        return Err("nonland copy has unsupported effective face".into());
    }
    Ok(Some(CopyTemplateV1 {
        card_def: live.card_def,
        face_index: live.v4.face_index,
        copied_end_step_sacrifices_v1: event::copiable_token_abilities_v1(state, *id)?,
        copied_nightmare_v1: crate::effect::harvester_v1::copiable_nightmare(state, *id)?,
    }))
}
fn hosts(state: &GameState, t: &CopyTemplateV1) -> Result<Vec<EffectObjectBinding>, String> {
    let d = crate::card_def::CARD_DEFS
        .get(usize::from(t.card_def))
        .ok_or("copy definition absent")?;
    let attachment = d.attachment.ok_or("copy host selection is not an Aura")?;
    let colors = if t.face_index == 1 {
        d.transform_face
            .as_ref()
            .ok_or("copied Aura face absent")?
            .colors
    } else {
        d.colors
    };
    let monocolored = colors.len() == 1;
    Ok(state
        .objects
        .iter()
        .filter_map(|(id, live)| {
            (live.zone == Zone::Battlefield
                && attachment.host_matches(state, id)
                && !crate::engine::has_effective_keyword(
                    state,
                    id,
                    Keywords::PROTECTION_FROM_EVERYTHING,
                )
                && !(monocolored
                    && crate::engine::has_effective_keyword(
                        state,
                        id,
                        Keywords::PROTECTION_FROM_MONOCOLORED,
                    )))
            .then_some(EffectObjectBinding {
                object: id,
                expected_zone: Zone::Battlefield,
                expected_zone_change_count: live.zone_change_count,
            })
        })
        .collect())
}
fn candidates(choice: &CopyChoiceV1) -> Vec<EffectTargetCandidate> {
    choice
        .candidates
        .iter()
        .copied()
        .map(|binding| EffectTargetCandidate {
            target: Target::Object(binding.object),
            expected_object: Some(binding),
        })
        .collect()
}
fn validate_input(
    state: &GameState,
    p: &EffectContinuation,
    c: &CopyChoiceV1,
) -> Result<(), String> {
    if !c.path.is_empty()
        || template(state, p)?.as_ref() != Some(&c.template)
        || hosts(state, &c.template)? != c.candidates
        || c.candidates.len() < 2
        || c.replacement
            != crate::doubling_season_v48::ReplacementInputV48::capture(state, p.ctx.controller)
        || c.selected_hosts.len() >= c.replacement.replaced_token_count(1)?
        || c.selected_hosts
            .iter()
            .any(|host| !c.candidates.contains(host))
        || !c.remaining.is_empty()
    {
        return Err("Aura-copy input or complete original continuation changed".into());
    }
    Ok(())
}
pub(super) fn validate_prompt(
    state: &GameState,
    p: &EffectContinuation,
    c: &CopyChoiceV1,
) -> Result<(), String> {
    validate_input(state, p, c)?;
    let expected = PendingEffectChoice::SelectTargets {
        player: p.ctx.controller,
        path: c.path.clone(),
        selected: vec![],
        legal: candidates(c),
        min_targets: 1,
        max_targets: 1,
        ordered: true,
        purpose: EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice: c.clone() },
    };
    if p.choice.as_ref() != Some(&expected)
        || p.frames != c.remaining
        || p.answered_choice_guard.is_some()
    {
        return Err("Aura-copy prompt erased, shifted or replaced its original stage".into());
    }
    Ok(())
}
pub(super) fn answer(
    p: &mut EffectContinuation,
    c: CopyChoiceV1,
    objects: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<(), String> {
    if objects.len() != 1
        || !c.candidates.contains(&objects[0])
        || path != c.path
        || p.frames != c.remaining
    {
        return Err("Aura-copy answer changed host, path or siblings".into());
    }
    let host = objects[0];
    p.answered_choice_guard = Some(EffectAnsweredChoiceGuard::AttachCopiedAuraV1 {
        choice: c.clone(),
        host,
    });
    p.frames.push(EffectFrame::Program {
        op: EffectOp::CreateAttachedAuraCopyV1 { choice: c, host },
        path,
    });
    Ok(())
}
pub(super) fn validate_answer(
    state: &GameState,
    p: &EffectContinuation,
    c: &CopyChoiceV1,
    host: EffectObjectBinding,
) -> Result<(), String> {
    validate_input(state, p, c)?;
    let guard = EffectAnsweredChoiceGuard::AttachCopiedAuraV1 {
        choice: c.clone(),
        host,
    };
    let mut frames = c.remaining.clone();
    frames.push(EffectFrame::Program {
        op: EffectOp::CreateAttachedAuraCopyV1 {
            choice: c.clone(),
            host,
        },
        path: c.path.clone(),
    });
    if p.answered_choice_guard.as_ref() != Some(&guard)
        || p.choice.is_some()
        || p.frames != frames
        || !c.candidates.contains(&host)
    {
        return Err("Aura-copy answered stage lost its exact original input".into());
    }
    Ok(())
}
pub(super) fn validate_boundary(state: &GameState, p: &EffectContinuation) -> Result<(), String> {
    if !requires_stage(p) || p.resolving_item.v4.granted_by.is_some() {
        return Ok(());
    }
    match (&p.choice, &p.answered_choice_guard) {
        (
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice },
                ..
            }),
            None,
        ) => validate_prompt(state, p, choice),
        (None, Some(EffectAnsweredChoiceGuard::AttachCopiedAuraV1 { choice, host })) => {
            validate_answer(state, p, choice, *host)
        }
        _ => Err("nonland-copy public continuation lost its required original Aura stage".into()),
    }
}
fn proposal(t: &CopyTemplateV1, controller: PlayerId) -> event::ProposedEvent {
    let mut proposal = event::ProposedEvent::create_token(t.card_def, controller);
    if let event::ProposedEvent::CreateToken(token) = &mut proposal {
        token.entry_face_index = (t.face_index != 0).then_some(t.face_index);
        token.initialization_v48 = Some(crate::doubling_season_v48::TokenEntryV48 {
            copied: true,
            copied_end_step_sacrifices_v1: t.copied_end_step_sacrifices_v1,
            copied_nightmare_v1: t.copied_nightmare_v1,
            ..Default::default()
        });
    }
    proposal
}
fn create(
    state: &mut GameState,
    t: &CopyTemplateV1,
    controller: PlayerId,
    hosts: Option<Vec<EffectObjectBinding>>,
) -> Result<(), String> {
    if let Some(hosts) = hosts {
        let legal = self::hosts(state, t)?;
        let count = crate::doubling_season_v48::ReplacementInputV48::capture(state, controller)
            .replaced_token_count(1)?;
        if hosts.len() != count || hosts.iter().any(|host| !legal.contains(host)) {
            return Err("copied Aura expanded roster lost a legal host".into());
        }
        event::batch_with_copied_aura_hosts_v1(
            state,
            proposal(t, controller),
            hosts
                .into_iter()
                .map(|b| ObjectLinkV4 {
                    object: b.object,
                    zone_change_count: b.expected_zone_change_count,
                })
                .collect(),
        );
    } else {
        event::propose_and_commit_batch(state, vec![proposal(t, controller)]);
    }
    Ok(())
}
fn install_prompt(p: &mut EffectContinuation, choice: CopyChoiceV1) {
    p.choice = Some(PendingEffectChoice::SelectTargets {
        player: p.ctx.controller,
        path: choice.path.clone(),
        selected: vec![],
        legal: candidates(&choice),
        min_targets: 1,
        max_targets: 1,
        ordered: true,
        purpose: EffectTargetSelectionPurpose::AttachCopiedAuraV1 { choice },
    });
}
pub(super) fn stage(
    state: &mut GameState,
    p: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let Some(t) = template(state, p)? else {
        return Ok(false);
    };
    let d = &crate::card_def::CARD_DEFS[t.card_def as usize];
    if d.attachment.is_none() {
        create(state, &t, p.ctx.controller, None)?;
        return Ok(false);
    }
    let list = hosts(state, &t)?;
    match list.as_slice() {
        [] => Ok(false),
        [host] => {
            let count =
                crate::doubling_season_v48::ReplacementInputV48::capture(state, p.ctx.controller)
                    .replaced_token_count(1)?;
            let mut hosts = Vec::new();
            hosts
                .try_reserve(count)
                .map_err(|_| "Aura roster allocation failed")?;
            hosts.resize(count, *host);
            create(state, &t, p.ctx.controller, Some(hosts))?;
            Ok(false)
        }
        _ => {
            let choice = CopyChoiceV1 {
                template: t,
                candidates: list,
                replacement: crate::doubling_season_v48::ReplacementInputV48::capture(
                    state,
                    p.ctx.controller,
                ),
                selected_hosts: vec![],
                path: path.clone(),
                remaining: p.frames.clone(),
            };
            install_prompt(p, choice);
            Ok(true)
        }
    }
}

pub(super) fn apply_answer(
    state: &mut GameState,
    p: &mut EffectContinuation,
    mut c: CopyChoiceV1,
    host: EffectObjectBinding,
    path: Vec<u16>,
) -> Result<bool, String> {
    validate_input(state, p, &c)?;
    if path != c.path
        || p.frames != c.remaining
        || p.answered_choice_guard.as_ref()
            != Some(&EffectAnsweredChoiceGuard::AttachCopiedAuraV1 {
                choice: c.clone(),
                host,
            })
        || !c.candidates.contains(&host)
        || p.choice.is_some()
    {
        return Err("copied Aura answer changed after canonical frame removal".into());
    }
    p.answered_choice_guard = None;
    c.selected_hosts.push(host);
    if c.selected_hosts.len() < c.replacement.replaced_token_count(1)? {
        install_prompt(p, c);
        Ok(true)
    } else {
        create(state, &c.template, p.ctx.controller, Some(c.selected_hosts))?;
        Ok(false)
    }
}

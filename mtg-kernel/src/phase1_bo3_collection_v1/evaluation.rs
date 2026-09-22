//! Evaluation-only V4 search route. These results are never training trajectories.
use super::*;
use crate::model_guided_search_core_v4 as search;

pub const BO3_EVALUATION_RESULT_SCHEMA_V1: &str = "mtg-kernel-bo3-search-evaluation/v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bo3EvaluationOptionsV1 {
    /// At most 16 complete search trees per match; their bytes count toward
    /// the same record budget as all other committed decisions.
    pub max_retained_search_outcomes: u8,
}

#[derive(Debug, Serialize)]
pub struct Bo3EvaluationResultV1 {
    schema: String,
    config: Bo3CollectionConfigV1,
    options: Bo3EvaluationOptionsV1,
    packages: [CompleteAgentPackageV1; 2],
    current_runtimes: [CurrentAgentRuntimeV1; 2],
    evaluated: EvaluatedMatch,
    /// Excludes wall-clock observations, includes the complete semantic result.
    semantic_sha256: String,
    timings: Vec<SearchTiming>,
}

#[derive(Debug, Serialize)]
struct EvaluatedMatch {
    schema: String,
    match_id: String,
    initial_chooser: PlayerSeatV1,
    behavior_packages_by_seat: [String; 2],
    registrations_by_seat: [OwnDeckConfigurationV1; 2],
    games: Vec<EvaluationGame>,
    ending: Bo3TrajectoryEndingV1,
    abort: Option<Abort>,
    committed_decision_records: u64,
    committed_decision_json_bytes: u64,
}

#[derive(Debug, Serialize)]
struct EvaluationGame {
    game_index: u8,
    start: Option<crate::bo3_match::GameStartV1>,
    environment_seed: Option<u64>,
    decisions: Vec<EvaluationDecision>,
    terminal: Option<Bo3GameTerminalV1>,
    discarded_pending_selections: u64,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum EvaluationDecision {
    Ordinary { record: Bo3DecisionRecordV1 },
    Search { record: Bo3DecisionRecordV1, search: SearchDiagnostics },
}
impl EvaluationDecision {
    fn record(&self) -> &Bo3DecisionRecordV1 {
        match self { Self::Ordinary{record} | Self::Search{record,..} => record }
    }
}

#[derive(Debug, Serialize)]
struct SearchDiagnostics {
    descriptor: V4InformationSetSearchDescriptorV1,
    selected_by_mean: u32,
    simulations: u32,
    transitions: u32,
    nodes: usize,
    headroom: [u64; 2],
    root_visits: Vec<u32>,
    root_value_sums: Vec<i64>,
    root_priors: Vec<u32>,
    root_work: Vec<search::RootWork>,
    census: search::Census,
    outcome_sha256: String,
    full_outcome: Option<search::Outcome>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum AbortCause {
    Search { error: search::Error },
    RecordCap,
    PhysicalGameCap,
    Truncated,
    NonNaturalTerminal,
    Engine { message: String },
}
#[derive(Debug, Serialize)]
struct Abort {
    game_index: u8,
    actor: Option<PlayerSeatV1>,
    step: Option<u64>,
    cause: AbortCause,
}
#[derive(Debug, Serialize)]
struct SearchTiming {
    game_index: u8,
    actor: PlayerSeatV1,
    step: u64,
    elapsed_ns: u64,
}

struct EvaluationStop {
    reason: IncompleteMatchReasonV1,
    actor: Option<PlayerSeatV1>,
    step: Option<u64>,
    cause: AbortCause,
}
impl From<String> for EvaluationStop {
    fn from(message: String) -> Self {
        Self { reason: IncompleteMatchReasonV1::EngineError, actor: None, step: None,
            cause: AbortCause::Engine { message } }
    }
}
impl EvaluationStop {
    fn at(mut self, actor: PlayerSeatV1, step: u64) -> Self {
        self.actor = Some(actor); self.step = Some(step); self
    }
}

struct EvaluationBudget {
    count: u64,
    bytes: u64,
    retained: u8,
    max_count: u64,
    max_bytes: u64,
    max_retained: u8,
}
impl EvaluationBudget {
    fn check(&self, record: &EvaluationDecision) -> Result<u64, EvaluationStop> {
        let size = serde_json::to_vec(record).map_err(|e| e.to_string())?.len() as u64;
        if self.count >= self.max_count || self.bytes.saturating_add(size) > self.max_bytes {
            return Err(EvaluationStop { reason: IncompleteMatchReasonV1::DecisionCap,
                actor: Some(record.record().actor), step: None, cause: AbortCause::RecordCap });
        }
        Ok(size)
    }
    fn append_checked(&mut self, game: &mut EvaluationGame, record: EvaluationDecision, size: u64) {
        debug_assert_eq!(record.record().decision_index, self.count);
        if matches!(&record, EvaluationDecision::Search { search: SearchDiagnostics { full_outcome: Some(_), .. }, .. }) {
            self.retained += 1;
        }
        self.count += 1; self.bytes += size; game.decisions.push(record);
    }
}

/// Explicit route; descriptor validation is not authority for a formal run.
pub fn evaluate_bo3_v4(
    config: Bo3CollectionConfigV1,
    packages: [CompleteAgentPackageV1; 2],
    options: Bo3EvaluationOptionsV1,
) -> Result<Bo3EvaluationResultV1, String> {
    validate_evaluation(&config, packages.each_ref(), &options)?;
    let [p0,p1] = packages.each_ref().map(CompleteAgentPackageV1::load_evaluation_components_v1);
    let p0=p0?; let p1=p1?;
    let mut policies=[p0.gameplay,p1.gameplay];
    let heads=[p0.sideboard,p1.sideboard];
    let (evaluated,timings)=evaluate_loaded(&config, packages.each_ref(), &mut policies,
        heads.each_ref().map(Option::as_ref), &options)?;
    let semantic_sha256=semantic_hash(&(&config,&options,&evaluated))?;
    Ok(Bo3EvaluationResultV1 { schema: BO3_EVALUATION_RESULT_SCHEMA_V1.into(), config,options,packages,
        current_runtimes:[p0.current_runtime,p1.current_runtime],evaluated,semantic_sha256,timings })
}

fn semantic_hash(value: &impl Serialize) -> Result<String,String> {
    Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(value).map_err(|e|e.to_string())?)))
}

fn validate_evaluation(config:&Bo3CollectionConfigV1, packages:[&CompleteAgentPackageV1;2], options:&Bo3EvaluationOptionsV1)->Result<(),String> {
    ensure(options.max_retained_search_outcomes<=16,"evaluation retains at most 16 full search outcomes")?;
    ensure(config.schema==BO3_COLLECTION_CONFIG_SCHEMA_V1,"BO3 evaluation config schema differs")?;
    ensure(!config.match_id.is_empty() && config.match_id.len()<=128 && config.match_id.is_ascii(),"invalid BO3 evaluation match identity")?;
    ensure(packages[0].runtime==packages[1].runtime,"physical seats use different runtime/feature contracts")?;
    ensure((3..=254).contains(&config.max_physical_games) && config.max_physical_decisions>0 && config.max_policy_steps>0,
        "BO3 evaluation requires positive episode limits and 3..=254 physical games")?;
    ensure((1..=MAX_RECORDS).contains(&config.max_decision_records) && (1..=MAX_RECORD_BYTES).contains(&config.max_decision_json_bytes),
        "BO3 evaluation recording limits exceed the bounded driver")?;
    for id in &config.summary_tags.requires_target | &config.summary_tags.is_counterspell {
        ensure((id as usize)<crate::card_def::CARD_DEFS.len(),"summary tag is outside the compiled card registry")?;
    }
    registrations(config)?;
    for package in packages {
        package.validate_metadata_v1()?;
        ensure(matches!(package.opening,AgentOpeningPolicyV1::Existing{protocol:Bo3OpeningProtocolV1::KeepSevenV2})
            && matches!(package.play_draw,AgentPlayDrawPolicyV1::Fixed{..})
            && matches!(package.search,AgentSearchPolicyV1::Disabled|AgentSearchPolicyV1::V4InformationSetV1{..}),
            "evaluation supports KeepSevenV2, fixed play/draw and Disabled or V4 search")?;
        ensure(package.gameplay_sampler_identity==crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1,
            "evaluation requires the wide categorical sampler")?;
        ensure(package.gameplay.identity.model.feature_contract_digest==crate::native_flat_tensorizer_v4::FEATURE_CONTRACT_DIGEST_V4
            && package.gameplay.identity.model.feature_encoding_digest==crate::native_flat_tensorizer_v4::FEATURE_ENCODING_DIGEST_V4,
            "evaluation requires both V4 feature generations")?;
    }
    Ok(())
}

fn evaluate_loaded(config:&Bo3CollectionConfigV1,packages:[&CompleteAgentPackageV1;2],
    policies:&mut [FrozenPlayPolicyV1;2],heads:[Option<&LearnedSideboardModelV1>;2],options:&Bo3EvaluationOptionsV1)
    ->Result<(EvaluatedMatch,Vec<SearchTiming>),String> {
    validate_evaluation(config,packages,options)?;
    crate::deterministic_math_v1::ensure_thread_mxcsr_normalized_v1().map_err(|e|format!("{e:?}"))?;
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|e|format!("{e:?}"))?;
    ensure(policies.iter().all(|p|p.feature_generation_v1()==PlayPolicyGenerationV1::V4),"evaluation requires actual V4 policies")?;
    for i in 0..2 {
        ensure(
            policies[i].uses_observation_successor_v3()
                && policies[i].actual_model_identity_v1() == packages[i].gameplay.identity.model
                && policies[i].identity_v1() == &packages[i].gameplay.identity.source_import
                && policies[i].runtime_sampler_identity_v1()
                    == packages[i].gameplay_sampler_identity,
            "installed BO3 gameplay differs from the behavior package",
        )?;
        match (&packages[i].sideboard, heads[i]) {
            (AgentSideboardPolicyV1::Keep, None) => {}
            (
                AgentSideboardPolicyV1::LearnedGreedyV1 {
                    play_identity,
                    embedding_table_sha256,
                    ..
                },
                Some(head),
            ) => {
                let embeddings = FrozenSideboardEmbeddingsV1::new_v1(
                    policies[i].embedding_rows_v1(),
                    play_identity.clone(),
                )
                .map_err(|e| e.to_string())?;
                head.validate_frozen_embeddings_v1(&embeddings)
                    .map_err(|e| e.to_string())?;
                // The public package loader verified the original file bytes.
                // Canonical reserialization is not necessarily the file's hash.
                ensure(
                    head.play_identity_v1() == play_identity
                        && embeddings.table_sha256_v1() == embedding_table_sha256,
                    "installed BO3 sideboard head differs from the behavior package",
                )?;
            }
            _ => return Err("BO3 sideboard descriptor/component mismatch".into()),
        }
    }
    let mut match_session =
        BestOfThreeDeckMatchV1::new_live_v1(registrations(config)?, player(config.initial_chooser))
            .map_err(|e| e.to_string())?;
    let registered = [PlayerId::P0, PlayerId::P1].map(|p| {
        match_session
            .registered_deck(p)
            .unwrap()
            .registered_configuration()
            .clone()
    });
    let mut current = registered.clone();
    let mut summaries: [Vec<GameSummaryV1>; 2] = [Vec::new(), Vec::new()];
    let mut timings=Vec::new();
    let mut seed_stream = SplitMix64::seed(config.seed);
    let mut budget = EvaluationBudget {
        retained:0, max_retained:options.max_retained_search_outcomes,
        count: 0,
        bytes: 0,
        max_count: config.max_decision_records,
        max_bytes: config.max_decision_json_bytes,
    };
    let tags = RemovalCounterspellTagsV1 {
        requires_target: config.summary_tags.requires_target.clone(),
        is_counterspell: config.summary_tags.is_counterspell.clone(),
    };

    let mut evaluated=EvaluatedMatch {schema:BO3_EVALUATION_RESULT_SCHEMA_V1.into(),match_id:config.match_id.clone(),
        initial_chooser:config.initial_chooser,behavior_packages_by_seat:[packages[0].package_sha256_v1()?,packages[1].package_sha256_v1()?],
        registrations_by_seat:config.registrations.clone(),games:Vec::new(),ending:Bo3TrajectoryEndingV1::Incomplete{reason:IncompleteMatchReasonV1::Interrupted},
        abort:None,committed_decision_records:0,committed_decision_json_bytes:0};
    loop {
        let (game_index,chooser)=match match_session.match_state().phase() {
            MatchPhaseV1::Complete{outcome}=>{evaluated.ending=Bo3TrajectoryEndingV1::Complete{outcome};break;},
            MatchPhaseV1::AwaitingPlayDrawChoice{game_index,chooser}=>(game_index,chooser),
            _=>return Err("evaluation coordinator unexpectedly awaits a game result".into()),
        };
        if game_index>config.max_physical_games {
            evaluated.ending=Bo3TrajectoryEndingV1::Incomplete{reason:IncompleteMatchReasonV1::PhysicalGameCap};
            evaluated.abort=Some(Abort{game_index,actor:None,step:None,cause:AbortCause::PhysicalGameCap});break;
        }
        let mut game=EvaluationGame{game_index,start:None,environment_seed:None,decisions:Vec::new(),terminal:None,discarded_pending_selections:0};
        let played=play_evaluation_game(config,packages,&evaluated.behavior_packages_by_seat,policies,heads,&mut match_session,
            &registered,&mut current,&summaries,&tags,&mut seed_stream,chooser,&mut game,&mut budget,&mut timings);
        evaluated.games.push(game);
        match played {
            Ok(summary)=>{
                let outcome=summary.winner.map_or(GameOutcomeV1::Draw,|winner|GameOutcomeV1::Win{winner});
                let mut other=summary.clone();other.checkpoint_weights_hash=packages[1].gameplay.identity.model.weights_sha256.clone();
                summaries[0].push(summary);summaries[1].push(other);
                if let Err(error)=match_session.record_game_result_v1(outcome) {
                    evaluated.ending=Bo3TrajectoryEndingV1::Incomplete{reason:IncompleteMatchReasonV1::EngineError};
                    evaluated.abort=Some(Abort{game_index,actor:None,step:None,cause:AbortCause::Engine{message:error.to_string()}});break;
                }
            },
            Err(stop)=>{
                evaluated.ending=Bo3TrajectoryEndingV1::Incomplete{reason:stop.reason};
                evaluated.abort=Some(Abort{game_index,actor:stop.actor,step:stop.step,cause:stop.cause});break;
            },
        }
    }
    evaluated.committed_decision_records=budget.count;evaluated.committed_decision_json_bytes=budget.bytes;
    Ok((evaluated,timings))
}
#[allow(clippy::too_many_arguments)]
fn play_evaluation_game(
    config: &Bo3CollectionConfigV1,
    packages: [&CompleteAgentPackageV1; 2],
    hashes: &[String; 2],
    policies: &mut [FrozenPlayPolicyV1; 2],
    heads: [Option<&LearnedSideboardModelV1>; 2],
    match_session: &mut BestOfThreeDeckMatchV1,
    registered: &[DeckConfigurationV1; 2],
    current: &mut [DeckConfigurationV1; 2],
    summaries: &[Vec<GameSummaryV1>; 2],
    tags: &RemovalCounterspellTagsV1,
    seed_stream: &mut SplitMix64,
    chooser: PlayerId,
    game: &mut EvaluationGame,
    budget: &mut EvaluationBudget,
    timings: &mut Vec<SearchTiming>,
) -> Result<GameSummaryV1, EvaluationStop> {
    let wins = [PlayerId::P0, PlayerId::P1].map(|p| match_session.match_state().wins(p).unwrap());
    if game.game_index > 1 {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let i = actor.index();
            let input = project_sideboard_input_v1(
                &registered[i],
                actor,
                &summaries[i],
                game.game_index,
                wins,
            )?;
            let mut deliberation = SideboardDeliberationStateV1::new_v1(&current[i]);
            let embeddings = heads[i]
                .map(|head| {
                    FrozenSideboardEmbeddingsV1::new_v1(
                        policies[i].embedding_rows_v1(),
                        head.play_identity_v1().clone(),
                    )
                })
                .transpose()
                .map_err(|e| e.to_string())?;
            while !deliberation.is_done_v1() {
                let actions = deliberation.legal_actions_v1();
                let selected = if let Some(head) = heads[i] {
                    head.score_v1(&input, &deliberation, embeddings.as_ref().unwrap())
                        .map_err(|e| e.to_string())?
                        .selected_action
                } else {
                    SideboardActionV1::Done
                };
                let selected_index = actions
                    .iter()
                    .position(|a| *a == selected)
                    .ok_or_else(|| "sideboard head selected an unavailable action".to_owned())?
                    as u32;
                let record = evaluation_deterministic(
                    budget,
                    hashes,
                    actor.into(),
                    selected_index,
                    ActorVisibleDecisionV1::Sideboard {
                        input: input.clone(),
                        ordered_actions: actions,
                    },
                );
                let size = budget.check(&record)?;
                deliberation.apply_v1(selected).map_err(|e| e.to_string())?;
                budget.append_checked(game, record, size);
            }
            current[i] = deliberation.configuration_v1().map_err(|e| e.to_string())?;
        }
    }
    let AgentPlayDrawPolicyV1::Fixed { choice } = packages[chooser.index()].play_draw else {
        unreachable!("validated fixed policy")
    };
    let record = evaluation_deterministic(
        budget,
        hashes,
        chooser.into(),
        if choice == PlayDrawChoiceV1::Play {
            0
        } else {
            1
        },
        ActorVisibleDecisionV1::PlayDraw {
            own_configuration: own(&current[chooser.index()]),
            own_games_won: wins[chooser.index()],
            opponent_games_won: wins[chooser.opponent().index()],
            ordered_choices: vec![PlayDrawChoiceV1::Play, PlayDrawChoiceV1::Draw],
        },
    );
    let size = budget.check(&record)?;
    let prepared = match_session
        .prepare_game_with_configurations_v1(chooser, choice, current.clone())
        .map_err(|e| e.to_string())?;
    game.start = Some(prepared.start());
    budget.append_checked(game, record, size);
    let environment_seed = seed_stream.next_u64();
    game.environment_seed = Some(environment_seed);
    let mut opening = HumanOpeningV1::new(
        u64::from(game.game_index),
        environment_seed,
        config.max_physical_decisions,
        config.max_policy_steps,
        config.deck_ids.clone(),
        current.each_ref().map(|c| c.mainboard().to_vec()),
        prepared.start().starting_player,
        PlayerId::P0,
    )?;
    // The existing protocol automatically keeps P1, then explicitly keeps P0.
    let automatic = opening.automatic_keep_seven_view_v1()?;
    let record = evaluation_opening_record(budget, hashes, &current[1], automatic)?;
    let size = budget.check(&record)?;
    budget.append_checked(game, record, size);
    let record = evaluation_opening_record(budget, hashes, &current[0], opening.view())?;
    let size = budget.check(&record)?;
    opening.keep()?;
    budget.append_checked(game, record, size);
    let mut episode = opening.into_session()?;
    let mut recorder=EvaluationPolicy {policies,packages,hashes,game,budget,timings,pending:None,
        failure:None,attempted:None};
    recorder
        .reset_for_game_v1(paired_policy_seeds_v1(environment_seed))
        .map_err(|e| e.to_string())?;
    let played = try_run_fast_episode_with_summary_v1(
        &mut episode,
        &packages[0].gameplay.identity.model.weights_sha256,
        tags,
        &mut recorder,
    );
    let terminal=match episode.current_response() {FastActorResponseV1::Terminal(t)=>Some(t),_=>None};
    recorder.finish_game(played,terminal)
}
fn evaluation_opening_record(
    budget: &EvaluationBudget,
    hashes: &[String; 2],
    current: &DeckConfigurationV1,
    view: HumanOpeningViewV1,
) -> Result<EvaluationDecision, String> {
    Ok(evaluation_deterministic(
        budget,
        hashes,
        view.human_seat,
        0,
        ActorVisibleDecisionV1::Mulligan {
            input: ActorOpeningInputV1 {
                own_configuration: own(current),
                own_hand: view.hand.iter().map(|c| c.card_id).collect(),
                mulligans_taken: view.mulligans_taken,
                remaining_bottom: view.required_bottom,
                starting_player: view.starting_player,
                opponent_hand_count: u8::try_from(view.opponent_hand_count)
                    .map_err(|e| e.to_string())?,
                opponent_has_kept: view.opponent_has_kept,
            },
            ordered_choices: vec![MulliganChoiceV1::Keep, MulliganChoiceV1::Mulligan],
        },
    ))
}
fn evaluation_deterministic(
    budget: &EvaluationBudget,
    packages: &[String; 2],
    actor: PlayerSeatV1,
    selected_index: u32,
    visible: ActorVisibleDecisionV1,
) -> EvaluationDecision {
    EvaluationDecision::Ordinary { record: Bo3DecisionRecordV1 {
        decision_index: budget.count,
        actor,
        behavior_package_sha256: packages[seat(actor)].clone(),
        behavior: BehaviorDistributionV1::Deterministic { selected_index },
        visible,
    }}
}

struct EvaluationPending {
    step:u64,
    record:EvaluationDecision,
    size:u64,
}
struct EvaluationPolicy<'a> {
    policies:&'a mut [FrozenPlayPolicyV1;2],
    packages:[&'a CompleteAgentPackageV1;2],
    hashes:&'a [String;2],
    game:&'a mut EvaluationGame,
    budget:&'a mut EvaluationBudget,
    timings:&'a mut Vec<SearchTiming>,
    pending:Option<EvaluationPending>,
    failure:Option<EvaluationStop>,
    attempted:Option<(PlayerSeatV1,u64)>,
}
impl EvaluationPolicy<'_> {
    fn commit_pending(&mut self,next_step:u64)->Result<(),String> {
        if let Some(pending)=&self.pending {
            ensure(pending.step.checked_add(1)==Some(next_step),"evaluation callback did not confirm previous gameplay step")?;
        } else {ensure(next_step==0,"evaluation callback skipped an unrecorded gameplay step")?;}
        if let Some(pending)=self.pending.take() {
            self.budget.append_checked(self.game,pending.record,pending.size);
        }
        Ok(())
    }
    fn finish_game(&mut self,played:Result<GameSummaryV1,String>,terminal:Option<RlSessionTerminalV1>)->Result<GameSummaryV1,EvaluationStop> {
        if let Some(t)=&terminal {
            self.game.terminal=Some(Bo3GameTerminalV1{classification:t.terminal_classification,outcome:t.terminal_outcome,gameplay_decision_count:t.policy_step_count});
        }
        let result=match played {
            Ok(summary)=>(|| {
                let t=terminal.as_ref().ok_or_else(||"successful evaluation lacks terminal".to_owned())?;
                if t.terminal_classification!=TerminalClassificationV1::Natural {
                    return Err(EvaluationStop{reason:IncompleteMatchReasonV1::EngineError,actor:None,step:Some(t.policy_step_count),cause:AbortCause::NonNaturalTerminal});
                }
                ensure(summary.winner.map(PlayerSeatV1::from)==t.winner,"evaluation summary and terminal winner differ")?;
                self.commit_pending(t.policy_step_count)?;
                Ok(summary)
            })(),
            Err(message)=>{
                Err(self.failure.take().unwrap_or_else(|| {
                    let mut stop=EvaluationStop::from(message);
                    if terminal.as_ref().is_some_and(|t|t.terminal_classification==TerminalClassificationV1::Truncated) {
                        stop.reason=IncompleteMatchReasonV1::DecisionCap;
                        stop.cause=AbortCause::Truncated;
                    }
                    if let Some((actor,step))=self.attempted.or_else(||self.pending.as_ref().map(|p|(p.record.record().actor,p.step))) {stop=stop.at(actor,step);}
                    stop
                }))
            },
        };
        if result.is_err() {
            self.game.discarded_pending_selections=u64::from(self.pending.take().is_some())+u64::from(self.attempted.take().is_some());
        }
        result
    }
    fn select(&mut self,input:&PairedBo1PolicyInputV1<'_>)->Result<u32,EvaluationStop> {
        let decision=input.decision();let i=seat(decision.acting_player);
        self.commit_pending(decision.step)?;
        self.attempted=Some((decision.acting_player,decision.step));
        let (selected,record)=match &self.packages[i].search {
            AgentSearchPolicyV1::Disabled=>{
                let (selected,scores)=self.policies[i].select_paired_with_scores_v1(input).map_err(|e|e.to_string())?;
                let behavior=BehaviorDistributionV1::hamilton_from_logits_v1(&scores.logits,selected)?;
                let record=input.capture_bo3_gameplay_v4(self.budget.count,self.hashes[i].clone(),behavior)?;
                (selected,EvaluationDecision::Ordinary{record})
            },
            AgentSearchPolicyV1::V4InformationSetV1{descriptor}=>{
                let limits=search::Limits{simulations:descriptor.simulations,transitions:descriptor.transitions,depth:descriptor.depth,seed:descriptor.experiment_seed};
                let allocation=match descriptor.root_allocation {V4SearchRootAllocationV1::Puct=>search::RootAllocation::Puct,V4SearchRootAllocationV1::RoundRobin=>search::RootAllocation::RoundRobin};
                let interior=match descriptor.interior_bonus {V4SearchInteriorBonusV1::PriorWeighted=>search::InteriorBonus::PriorWeighted,V4SearchInteriorBonusV1::PriorFree=>search::InteriorBonus::PriorFree};
                let started=std::time::Instant::now();
                let result=input.evaluation_search_v4(&self.policies[i],limits,allocation,interior);
                self.timings.push(SearchTiming{game_index:self.game.game_index,actor:decision.acting_player,step:decision.step,
                    elapsed_ns:u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)});
                let outcome=result.map_err(|error|EvaluationStop{reason:IncompleteMatchReasonV1::EngineError,
                    actor:Some(decision.acting_player),step:Some(decision.step),cause:AbortCause::Search{error}})?;
                let selected=outcome.selected;
                let record=input.capture_bo3_gameplay_v4(self.budget.count,self.hashes[i].clone(),BehaviorDistributionV1::Deterministic{selected_index:selected})?;
                let diagnostics=SearchDiagnostics{descriptor:descriptor.clone(),selected_by_mean:outcome.selected_by_mean,
                    simulations:outcome.simulations,transitions:outcome.transitions,nodes:outcome.nodes,headroom:outcome.headroom,
                    root_visits:outcome.root_visits.clone(),root_value_sums:outcome.root_value_sums.clone(),root_priors:outcome.root_priors.clone(),
                    root_work:outcome.root_work.clone(),census:outcome.census.clone(),outcome_sha256:semantic_hash(&outcome)?,
                    full_outcome:if self.budget.retained<self.budget.max_retained {Some(outcome)} else {None}};
                (selected,EvaluationDecision::Search{record,search:diagnostics})
            },
            _=>return Err("unsupported evaluation search descriptor".to_owned().into()),
        };
        let size=self.budget.check(&record)?;
        self.pending=Some(EvaluationPending{step:decision.step,record,size});
        self.attempted=None;
        Ok(selected)
    }
}
impl PairedBo1PolicyV1 for EvaluationPolicy<'_> {
    fn uses_observation_successor_v3(&self)->bool {true}
    fn reset_for_game_v1(&mut self,seeds:[u64;2])->Result<(),RlSessionError> {
        for policy in &mut *self.policies {policy.reset_for_game_v1(seeds)?;} Ok(())
    }
    fn select_action_v1(&mut self,input:PairedBo1PolicyInputV1<'_>)->Result<u32,RlSessionError> {
        let decision=input.decision();
        self.select(&input).map_err(|stop| {
            let message=format!("evaluation aborted: {:?}",stop.cause);
            self.failure=Some(stop.at(decision.acting_player,decision.step));
            RlSessionError{code:RlSessionErrorCode::StaleEnvironmentBinding,message}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase1_bo3_collection_v1::tests as fixtures;

    fn search_package(p:&mut CompleteAgentPackageV1,depth:u16) {
        let m=&p.gameplay.identity.model;
        p.search=AgentSearchPolicyV1::V4InformationSetV1{descriptor:V4InformationSetSearchDescriptorV1{
            schema:V4_INFORMATION_SET_SEARCH_SCHEMA_V1.into(),algorithm:V4_INFORMATION_SET_SEARCH_ALGORITHM_V1.into(),
            root_allocation:V4SearchRootAllocationV1::RoundRobin,interior_bonus:V4SearchInteriorBonusV1::PriorFree,
            simulations:32,transitions:128,depth,experiment_seed:29,
            weights_sha256:m.weights_sha256.clone(),model_parameter_sha256:m.model_parameter_sha256.clone(),
            embedding_table_sha256:m.embedding_table_sha256.clone(),feature_contract_digest:m.feature_contract_digest.clone(),
            feature_encoding_digest:m.feature_encoding_digest.clone()}};
        p.validate_metadata_v1().unwrap();
    }

    #[test]
    fn v4_evaluation_disabled_matches_native_bo3_and_repeats_semantically() {
        let config=fixtures::config("evaluation-parity");
        let options=Bo3EvaluationOptionsV1{max_retained_search_outcomes:0};
        let choices=[PlayDrawChoiceV1::Play,PlayDrawChoiceV1::Draw];
        let (mut native_policies,packages)=fixtures::fixtures_v4(choices);
        let native=collect_loaded(&config,packages.each_ref(),&mut native_policies,[None,None]).unwrap();
        let mut policies=fixtures::fixtures_v4(choices).0;
        let (actual,timing)=evaluate_loaded(&config,packages.each_ref(),&mut policies,[None,None],&options).unwrap();
        assert!(matches!(actual.ending,Bo3TrajectoryEndingV1::Complete{..}));
        assert_eq!(actual.ending,native.trajectory.ending);assert!(actual.abort.is_none());assert!(timing.is_empty());
        assert_eq!(actual.games.len(),native.trajectory.games.len());
        assert_eq!(actual.committed_decision_records,native.committed_decision_records);
        for (a,b) in actual.games.iter().zip(&native.trajectory.games) {
            assert_eq!(a.start,b.start);assert_eq!(a.terminal,b.terminal);assert_eq!(a.discarded_pending_selections,0);
            assert_eq!(a.decisions.len(),b.decisions.len());
            for (a,b) in a.decisions.iter().zip(&b.decisions) {assert_eq!(a.record(),b);assert!(matches!(a,EvaluationDecision::Ordinary{..}));}
        }
        let mut repeat_policies=fixtures::fixtures_v4(choices).0;
        let (repeat,_)=evaluate_loaded(&config,packages.each_ref(),&mut repeat_policies,[None,None],&options).unwrap();
        assert_eq!(semantic_hash(&(&config,&options,&actual)).unwrap(),semantic_hash(&(&config,&options,&repeat)).unwrap());
        let encoded=serde_json::to_value(&actual).unwrap();
        assert!(serde_json::from_value::<Bo3TrainingTrajectoryV1>(encoded).is_err());
    }

    #[test]
    fn v4_evaluation_search_headroom_abort_is_typed_and_has_no_fallback() {
        let mut config=fixtures::config("evaluation-headroom");config.max_policy_steps=1;
        let options=Bo3EvaluationOptionsV1{max_retained_search_outcomes:1};
        let (mut policies,mut packages)=fixtures::fixtures_v4([PlayDrawChoiceV1::Play;2]);
        for p in &mut packages {search_package(p,4);}
        assert!(validate_configuration(&config,packages.each_ref()).is_err());
        assert!(packages[0].load_supported_components_v1().is_err());
        let (result,timing)=evaluate_loaded(&config,packages.each_ref(),&mut policies,[None,None],&options).unwrap();
        let abort=result.abort.as_ref().unwrap();
        assert!(matches!(abort.cause,AbortCause::Search{error:search::Error::InsufficientHeadroom}));
        assert_eq!(abort.game_index,1);assert_eq!(abort.step,Some(0));assert!(abort.actor.is_some());
        assert_eq!(timing.len(),1);
        assert_eq!(result.games[0].discarded_pending_selections,1);
        assert!(result.games[0].decisions.iter().all(|d|!matches!(d.record().visible,ActorVisibleDecisionV1::Gameplay{..})));
    }

    #[test]
    fn v4_evaluation_caps_and_runtime_identity_are_distinct_from_engine_faults() {
        let mut config=fixtures::config("evaluation-truncation");config.max_policy_steps=1;
        let options=Bo3EvaluationOptionsV1{max_retained_search_outcomes:0};
        let (mut policies,mut packages)=fixtures::fixtures_v4([PlayDrawChoiceV1::Play;2]);
        let (result,_)=evaluate_loaded(&config,packages.each_ref(),&mut policies,[None,None],&options).unwrap();
        assert!(matches!(result.ending,Bo3TrajectoryEndingV1::Incomplete{reason:IncompleteMatchReasonV1::DecisionCap}));
        assert!(matches!(result.abort.unwrap().cause,AbortCause::Truncated));
        let old=packages[1].runtime.tracked_tree_sha256.clone();packages[1].runtime.tracked_tree_sha256="e".repeat(64);
        assert_eq!(validate_evaluation(&config,packages.each_ref(),&options).unwrap_err(),"physical seats use different runtime/feature contracts");
        packages[1].runtime.tracked_tree_sha256=old;
        for id in ["".to_owned(),"x".repeat(129),"nonascii-\u{e9}".to_owned()] {
            config.match_id=id;
            assert_eq!(validate_evaluation(&config,packages.each_ref(),&options).unwrap_err(),"invalid BO3 evaluation match identity");
        }
    }

    #[test]
    fn v4_evaluation_records_search_truthfully_and_never_commits_an_unconfirmed_step() {
        use crate::policy_observation_v6::tests::{ready_state,put};
        use crate::state::Zone;
        use crate::rl_session::FastActorSessionV1;
        let mut state=ready_state();
        put(&mut state,PlayerId::P0,"Lightning Bolt",Zone::Hand);
        state.players[0].mana_pool[crate::mana::ManaColor::R.pool_index()]=3;
        for actor in [PlayerId::P0,PlayerId::P1] {for name in ["Forest","Mountain","Island"] {put(&mut state,actor,name,Zone::Library);}}
        let mut session=FastActorSessionV1::from_v3_fixture_state(state);
        let (mut policies,mut packages)=fixtures::fixtures_v4([PlayDrawChoiceV1::Play;2]);
        for p in &mut packages {search_package(p,4);}
        let hashes=[packages[0].package_sha256_v1().unwrap(),packages[1].package_sha256_v1().unwrap()];
        let mut game=EvaluationGame{game_index:1,start:None,environment_seed:None,decisions:Vec::new(),terminal:None,discarded_pending_selections:0};
        let mut budget=EvaluationBudget{count:0,bytes:0,retained:0,max_count:100,max_bytes:MAX_RECORD_BYTES,max_retained:1};
        let mut timings=Vec::new();
        let mut recorder=EvaluationPolicy{policies:&mut policies,packages:packages.each_ref(),hashes:&hashes,game:&mut game,budget:&mut budget,
            timings:&mut timings,pending:None,failure:None,attempted:None};
        let FastActorResponseV1::Decision(d)=session.current_response() else {panic!("fixture")};
        let action=recorder.select_action_v1(PairedBo1PolicyInputV1::new(&session,d)).unwrap();
        assert!(recorder.game.decisions.is_empty());
        session.step(d.episode_id,d.step,action).unwrap();
        let FastActorResponseV1::Decision(next)=session.current_response() else {panic!("fixture needs next decision")};
        recorder.select_action_v1(PairedBo1PolicyInputV1::new(&session,next)).unwrap();
        let EvaluationDecision::Search{record,search}= &recorder.game.decisions[0] else {panic!("search record missing")};
        assert_eq!(record.behavior,BehaviorDistributionV1::Deterministic{selected_index:action});
        assert_eq!(search.outcome_sha256,semantic_hash(search.full_outcome.as_ref().unwrap()).unwrap());
        assert_eq!(search.simulations,32);assert_eq!(recorder.budget.retained,1);
        // A failed engine receipt never admits the second pending selection.
        assert!(matches!(&recorder.pending.as_ref().unwrap().record,EvaluationDecision::Search{search:SearchDiagnostics{full_outcome:None,..},..}));
        assert!(recorder.finish_game(Err("fixed engine failure receipt".into()),None).is_err());
        assert_eq!(recorder.game.discarded_pending_selections,1);
        assert_eq!(recorder.game.decisions.len(),1);
    }
}


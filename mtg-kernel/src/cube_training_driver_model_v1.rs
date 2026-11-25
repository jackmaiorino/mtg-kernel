use super::{CubeModelState, HeadUpdates, OptimizerOutput, ParameterSetState, Segment};
use crate::cube_draft_build_v1::model::{
    training::{self, Build, Chair, Optimizer, Phase, Role, Row},
    CardHiddenTableV1, ModelCardRowsV1, ModelV1, ParameterSetV1, PickStateV1, TrunkV1,
};
use crate::cube_draft_v1::PACK_SIZE;
use crate::cube_learned_seat_v1::{BuildInputs, PickInputs, SeatForward};
use crate::cube_pod_runner_v1::{BuilderPin, ChairRole, PodRecord};
use crate::cube_seat_v1::SeatError;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub(super) struct TrainingModel {
    model: ModelV1,
    rows: Arc<ModelCardRowsV1>,
    build_hidden: CardHiddenTableV1,
    pick_hidden: CardHiddenTableV1,
}

impl TrainingModel {
    pub(super) fn new(model: ModelV1, rows: Arc<ModelCardRowsV1>) -> Result<Self, String> {
        let hidden = |trunk| {
            model
                .card_hidden_table(&rows, trunk, 1)
                .map_err(|e| e.to_string())
        };
        Ok(Self {
            build_hidden: hidden(TrunkV1::Build)?,
            pick_hidden: hidden(TrunkV1::Pick)?,
            model,
            rows,
        })
    }

    pub(super) fn parameters(&self) -> &ModelV1 {
        &self.model
    }

    pub(super) fn update(
        &self,
        mut optimizer: Optimizer,
        segment: Segment,
        pods: &[PodRecord],
        snapshots: &BTreeMap<String, Arc<Self>>,
    ) -> Result<(Self, Optimizer, OptimizerOutput), String> {
        let phase = match segment {
            Segment::BuilderOnly => Phase::Builder,
            Segment::PickPhase => Phase::Joint,
        };
        let prepared = pods
            .iter()
            .map(prepare_pod)
            .collect::<Result<Vec<_>, _>>()?;
        let chairs: Vec<_> = prepared
            .iter()
            .flatten()
            .map(|chair| Chair {
                role: match &chair.role {
                    Role::Learning { scores } => Role::Learning { scores: *scores },
                    Role::Older {
                        picker_pin,
                        builder_pin,
                    } => Role::Older {
                        picker_pin: *picker_pin,
                        builder_pin,
                    },
                },
                picks: &chair.picks,
                pool: chair.pool,
                builds: &chair.builds,
            })
            .collect();
        let pinned = snapshots
            .iter()
            .map(|(pin, snapshot)| (pin.clone(), &snapshot.model))
            .collect();
        let mut model = self.model.clone();
        let losses = training::update(
            &mut model,
            &mut optimizer,
            &self.rows,
            phase,
            &chairs,
            &pinned,
        )?;
        let loss_values = [
            ("build", losses.build),
            ("pick", losses.pick),
            ("value", losses.value),
        ]
        .into_iter()
        .map(|(name, loss)| (name.to_owned(), loss))
        .collect();
        Ok((
            Self::new(model, self.rows.clone())?,
            optimizer,
            OptimizerOutput {
                losses: loss_values,
                gradient_norms: losses.gradient_norms,
                statistics: losses.statistics,
            },
        ))
    }
}

fn pick_state(input: &PickInputs<'_>) -> PickStateV1 {
    PickStateV1 {
        pack: input.pack.to_vec(),
        pool: input.pool.to_vec(),
        history: input.history.to_vec(),
        step: input
            .step
            .iter()
            .position(|&step| step == 1.0)
            .expect("D3 pick step") as u8,
        direction: input.direction as i8,
    }
}

fn seat_error(error: impl ToString) -> SeatError {
    SeatError::Seat(error.to_string())
}

impl SeatForward for TrainingModel {
    fn pick_and_value(&self, input: &PickInputs<'_>) -> Result<(Vec<f32>, f32), SeatError> {
        let state = pick_state(input);
        let forward = self
            .model
            .pick_forward_with_hidden(&self.rows, &self.pick_hidden);
        let logits = forward.pick_logits_v1(&state).map_err(seat_error)?;
        let value = forward.value_v1(&state).map_err(seat_error)?;
        Ok((logits.logits, value))
    }

    fn count(&self, pool: &[u16]) -> Result<[f32; 41], SeatError> {
        let forward = self
            .model
            .build_forward_with_hidden(&self.rows, &self.build_hidden);
        forward
            .build_context_v1(pool)
            .map_err(seat_error)?
            .count_logits_v1()
            .try_into()
            .map_err(|_| seat_error("D2 count width differs"))
    }

    fn pointer(&self, input: &BuildInputs<'_>, remaining: &[u16]) -> Result<Vec<f32>, SeatError> {
        let forward = self
            .model
            .build_forward_with_hidden(&self.rows, &self.build_hidden);
        let context = forward.build_context_v1(input.pool).map_err(seat_error)?;
        Ok(context
            .pointer_logits_v1(input.basics.into(), input.chosen, remaining)
            .map_err(seat_error)?
            .logits)
    }

    fn type_set(&self, input: &BuildInputs<'_>, masks: &[u16]) -> Result<Vec<f32>, SeatError> {
        let forward = self
            .model
            .build_forward_with_hidden(&self.rows, &self.build_hidden);
        let context = forward.build_context_v1(input.pool).map_err(seat_error)?;
        let logits = context
            .type_context_v1(input.basics.into(), input.chosen)
            .map_err(seat_error)?
            .typeset_logits_v1();
        Ok(masks
            .iter()
            .map(|&mask| logits[usize::from(mask) - 1])
            .collect())
    }

    fn split(&self, input: &BuildInputs<'_>, bits: &[u16]) -> Result<Vec<f32>, SeatError> {
        let forward = self
            .model
            .build_forward_with_hidden(&self.rows, &self.build_hidden);
        let context = forward.build_context_v1(input.pool).map_err(seat_error)?;
        let mask = bits.iter().fold(0, |mask, &bit| mask | bit as u8);
        Ok(context
            .type_context_v1(input.basics.into(), input.chosen)
            .map_err(seat_error)?
            .split_logits_v1(mask)
            .map_err(seat_error)?
            .logits)
    }
}

fn set_index(set: ParameterSetV1) -> usize {
    match set {
        ParameterSetV1::Build => 0,
        ParameterSetV1::Pick => 1,
        ParameterSetV1::Value => 2,
    }
}

impl CubeModelState {
    pub(super) fn from_model(model: &ModelV1, optimizer: &Optimizer) -> Self {
        let mut sets = optimizer.steps.map(|adam_step| ParameterSetState {
            adam_step,
            ..ParameterSetState::default()
        });
        for (i, spec) in model.layout().iter().enumerate() {
            let set = &mut sets[set_index(spec.set)];
            set.parameters.extend(
                model
                    .tensor_v1(&spec.name)
                    .unwrap()
                    .iter()
                    .map(|f| f.to_bits()),
            );
            set.first_moment
                .extend(optimizer.first[i].iter().map(|f| f.to_bits()));
            set.second_moment
                .extend(optimizer.second[i].iter().map(|f| f.to_bits()));
        }
        let [build, pick, value] = sets;
        Self { build, pick, value }
    }

    pub(super) fn restore(
        &self,
        mut model: ModelV1,
        head_updates: HeadUpdates,
    ) -> Result<(ModelV1, Optimizer), String> {
        let sets = [&self.build, &self.pick, &self.value];
        for (set, kind) in sets.into_iter().zip([
            ParameterSetV1::Build,
            ParameterSetV1::Pick,
            ParameterSetV1::Value,
        ]) {
            let length = model.parameter_count_in_set(kind);
            if [&set.parameters, &set.first_moment, &set.second_moment]
                .into_iter()
                .any(|v| {
                    v.len() != length || v.iter().any(|&bits| !f32::from_bits(bits).is_finite())
                })
                || set
                    .second_moment
                    .iter()
                    .any(|&bits| f32::from_bits(bits) < 0.0)
            {
                return Err(
                    "checkpoint parameter or moment values differ from the model layout".into(),
                );
            }
        }
        let mut optimizer = Optimizer::new(&model);
        optimizer.steps = sets.map(|set| set.adam_step);
        optimizer.head_updates = head_updates;
        let mut offsets = [0; 3];
        let mut index = 0;
        model.visit_tensors_mut_v1(|_, kind, _, weights| {
            let set_index = set_index(kind);
            let set = sets[set_index];
            let start = offsets[set_index];
            let end = start + weights.len();
            for (out, bits) in [
                (weights, &set.parameters[start..end]),
                (&mut optimizer.first[index], &set.first_moment[start..end]),
                (&mut optimizer.second[index], &set.second_moment[start..end]),
            ] {
                for (value, &bits) in out.iter_mut().zip(bits) {
                    *value = f32::from_bits(bits);
                }
            }
            offsets[set_index] = end;
            index += 1;
        });
        Ok((model, optimizer))
    }
}

struct PreparedChair<'a> {
    role: Role<'a>,
    picks: Vec<(PickStateV1, Row)>,
    pool: &'a [u16],
    builds: Vec<Build>,
}

fn prepare_pod(pod: &PodRecord) -> Result<Vec<PreparedChair<'_>>, String> {
    let draft = pod.draft.as_ref().ok_or("training pod has no draft")?;
    pod.chairs
        .iter()
        .map(|chair| {
            let mut picks = Vec::new();
            let mut pool = Vec::new();
            let mut history = BTreeSet::new();
            let mut rows = chair.pick_rows.iter();
            for pick in draft
                .picks
                .iter()
                .filter(|pick| pick.seat == usize::from(chair.chair))
            {
                if chair.picker.pin.is_some() && pick.pack.len() > 1 {
                    let row = rows.next().ok_or("missing learned pick row")?;
                    if row.chosen != pick.chosen {
                        return Err("pick row disagrees with draft choice".into());
                    }
                    picks.push((
                        PickStateV1 {
                            pack: pick.pack.clone(),
                            pool: pool.clone(),
                            history: history.iter().copied().collect(),
                            step: (pick.round * PACK_SIZE + pick.pick) as u8,
                            direction: if pick.round == 1 { -1 } else { 1 },
                        },
                        serde_json::from_value(
                            serde_json::to_value(row).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?,
                    ));
                }
                history.extend(pick.pack.iter().copied());
                history.remove(&pick.chosen);
                pool.push(pick.chosen);
            }
            if rows.next().is_some() || pool != draft.pools[usize::from(chair.chair)] {
                return Err("training pick rows or pool differ from draft".into());
            }
            let decks: Vec<_> = pod
                .decks
                .iter()
                .filter(|deck| deck.chair == Some(chair.chair))
                .collect();
            let builds = decks
                .iter()
                .map(|deck| {
                    let rows = deck
                        .build_rows
                        .as_ref()
                        .ok_or("training deck has no learned build rows")?;
                    serde_json::from_value(serde_json::to_value(rows).map_err(|e| e.to_string())?)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let role = match chair.role {
                ChairRole::Learning => {
                    let pair = pod
                        .paired_builds
                        .iter()
                        .find(|pair| pair.chair == chair.chair)
                        .ok_or("learning chair has no matched games")?;
                    let scores = |games: &[u32], deck| {
                        let mut scores = [0; 7];
                        if games.len() != scores.len() {
                            return Err("learning chair needs seven matched game scores");
                        }
                        for (score, &id) in scores.iter_mut().zip(games) {
                            let game = pod.games.get(id as usize).ok_or("missing matched game")?;
                            let seat = game
                                .decks
                                .iter()
                                .position(|&d| d == deck)
                                .ok_or("matched game does not contain focal deck")?;
                            *score = game.scores.ok_or("matched game did not complete")?[seat];
                        }
                        Ok(scores)
                    };
                    Role::Learning {
                        scores: [
                            scores(&pair.games_x, pair.deck_x)?,
                            scores(&pair.games_y, pair.deck_y)?,
                        ],
                    }
                }
                ChairRole::Older => {
                    let [deck] = decks.as_slice() else {
                        return Err("older chair needs one build".into());
                    };
                    let Some(BuilderPin::Checkpoint(pin)) = &deck.builder.pin else {
                        return Err("older chair needs a builder pin".into());
                    };
                    Role::Older {
                        picker_pin: chair.picker.pin.as_deref(),
                        builder_pin: pin,
                    }
                }
                ChairRole::Fixture => return Err("fixture chair in training update".into()),
            };
            Ok(PreparedChair {
                role,
                picks,
                pool: &draft.pools[usize::from(chair.chair)],
                builds,
            })
        })
        .collect()
}

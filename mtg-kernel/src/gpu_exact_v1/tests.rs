//! Device tests run only with a GPU:
//!   MTG_KERNEL_GPU_EXACT_TEST_ORDINAL=1 cargo test --release --features gpu-exact-v1
//!     --lib gpu_exact_v1 -- --ignored --test-threads 1
//! Host-only tests run everywhere.

use super::device::{KernelsV1, TileShapeV1};
use super::net8::{
    forward_batch_v1, DeviceBuffersV1, Net8DeviceModelV1, Net8HostModelV1, PackedBatchV1,
};
use super::service::{BatchPolicyV1, InferenceServiceV1};
use super::ExactDeviceV1;
use crate::inference_cache_v1::ForwardPathV1;
use crate::native_policy_value_net_v1::{
    NativeEncodedDecisionSchemaV1, NativeEncodedDecisionViewV1, NativePolicyValueModelConfigV1,
    NativePolicyValueNetV1,
};
use crate::state::SplitMix64;
use std::sync::Arc;
use std::time::Duration;

fn ordinal() -> usize {
    std::env::var("MTG_KERNEL_GPU_EXACT_TEST_ORDINAL")
        .expect("set MTG_KERNEL_GPU_EXACT_TEST_ORDINAL to the test GPU")
        .parse()
        .expect("ordinal")
}

fn unit(rng: &mut SplitMix64) -> f32 {
    // 24 random bits in [-1, 1), with exact zeros of both signs mixed in, as
    // one-hot features and tanh outputs give.
    match rng.next_u64() % 16 {
        0 => 0.0,
        1 => -0.0,
        _ => ((rng.next_u64() >> 40) as f32 / (1u64 << 23) as f32) - 1.0,
    }
}

/// A net-8 with every parameter random, so no structure hides an order
/// difference.
fn random_net(seed: u64) -> NativePolicyValueNetV1 {
    let mut net =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .expect("net-8");
    let mut rng = SplitMix64::seed(seed);
    let mut snapshot = net.parameter_snapshot_v1();
    for parameter in &mut snapshot {
        for value in &mut parameter.values {
            *value = unit(&mut rng) * 0.3;
        }
        // Card 0 is padding: its embedding row stays zero (a parameter invariant).
        if parameter.name == "card_embedding.weight" {
            parameter.values[..16].fill(0.0);
        }
    }
    net.replace_parameter_snapshot_v1(&snapshot)
        .expect("replace");
    net
}

/// Owned inputs for one random decision.
struct RandomDecision {
    state: Vec<f32>,
    object_features: Vec<f32>,
    object_card_ids: Vec<i64>,
    object_groups: Vec<i64>,
    object_node_ids: Vec<i64>,
    edge_features: Vec<f32>,
    edge_sources: Vec<i64>,
    edge_targets: Vec<i64>,
    action_features: Vec<f32>,
    ref_features: Vec<f32>,
    ref_card_ids: Vec<i64>,
    ref_actions: Vec<i64>,
    ref_nodes: Vec<i64>,
}

fn random_decision(seed: u64) -> RandomDecision {
    random_decision_sized(seed, [40, 60, 24, 30])
}

/// A decision with up to `limits` = [objects, edges, actions, refs] rows.
fn random_decision_sized(seed: u64, limits: [u64; 4]) -> RandomDecision {
    let schema = NativeEncodedDecisionSchemaV1::contract_v1();
    let mut rng = SplitMix64::seed(seed);
    let below = |n: u64, rng: &mut SplitMix64| (rng.next_u64() % n) as i64;
    let objects = 1 + below(limits[0], &mut rng) as usize;
    let edges = below(limits[1], &mut rng) as usize;
    let actions = 1 + below(limits[2], &mut rng) as usize;
    let refs = below(limits[3], &mut rng) as usize;
    let floats = |n: usize, rng: &mut SplitMix64| (0..n).map(|_| unit(rng)).collect::<Vec<f32>>();
    RandomDecision {
        state: floats(schema.state_dim, &mut rng),
        object_features: floats(objects * schema.object_feature_dim, &mut rng),
        // Card 0 is padding; real ids span the vocabulary.
        object_card_ids: (0..objects).map(|_| below(65_537, &mut rng)).collect(),
        object_groups: (0..objects)
            .map(|_| below(schema.object_group_count as u64, &mut rng))
            .collect(),
        object_node_ids: (0..objects as i64).collect(),
        edge_features: floats(edges * schema.edge_feature_dim, &mut rng),
        edge_sources: (0..edges)
            .map(|_| below(objects as u64, &mut rng))
            .collect(),
        edge_targets: (0..edges)
            .map(|_| below(objects as u64, &mut rng))
            .collect(),
        action_features: floats(actions * schema.action_feature_dim, &mut rng),
        ref_features: floats(refs * schema.action_ref_feature_dim, &mut rng),
        ref_card_ids: (0..refs).map(|_| below(65_537, &mut rng)).collect(),
        ref_actions: (0..refs).map(|_| below(actions as u64, &mut rng)).collect(),
        ref_nodes: (0..refs).map(|_| below(objects as u64, &mut rng)).collect(),
    }
}

impl RandomDecision {
    fn view(&self) -> NativeEncodedDecisionViewV1<'_> {
        NativeEncodedDecisionViewV1::from_slices_unvalidated(
            NativeEncodedDecisionSchemaV1::contract_v1(),
            &self.state,
            &self.object_features,
            &self.object_card_ids,
            &self.object_groups,
            &self.object_node_ids,
            &self.edge_features,
            &self.edge_sources,
            &self.edge_targets,
            &self.action_features,
            &self.ref_features,
            &self.ref_card_ids,
            &self.ref_actions,
            &self.ref_nodes,
        )
    }
}

fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

#[test]
fn random_decisions_validate_on_the_cpu_forward() {
    let net = random_net(1);
    for seed in 0..50 {
        let decision = random_decision(seed);
        net.forward_v1(decision.view()).expect("cpu forward");
    }
}

#[test]
fn appended_requests_keep_their_own_offsets() {
    let net = random_net(2);
    let mut single = PackedBatchV1::default();
    let mut merged = PackedBatchV1::default();
    for seed in 0..5 {
        single.clear();
        let decision = random_decision(seed);
        single
            .push_v1(0, &net, ForwardPathV1::Flat, decision.view())
            .expect("pack");
        merged
            .append_v1(&single, (seed % 2) as usize)
            .expect("append");
    }
    assert_eq!(merged.len(), 5);
}

#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn gpu_tanh_equals_host_tanhf_on_all_inputs() {
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    assert_eq!(device.tanh_mismatch_v1().expect("proof"), None);
}

/// Every reply bit equals the CPU forward's, for random nets and inputs.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn gpu_forward_equals_cpu_forward_bit_for_bit() {
    // v1; v2 with the default tile; v2 with another tile shape, which moves
    // every output to another thread and every chain across other tile
    // boundaries without changing its order.
    let alternate = TileShapeV1 {
        k_tile: 32,
        rows: 16,
    };
    let variants = [
        (KernelsV1::V1, TileShapeV1::DEFAULT),
        (KernelsV1::V2, TileShapeV1::DEFAULT),
        (KernelsV1::V2, alternate),
    ];
    for (kernels, tile) in variants {
        let device = ExactDeviceV1::open_with_tile_v1(ordinal(), tile).expect("device");
        let mut buffers = DeviceBuffersV1::default();
        let mut batch = PackedBatchV1::default();
        for net_seed in 0..4u64 {
            let net = random_net(100 + net_seed);
            let host = Net8HostModelV1::from_net_v1(&net).expect("host model");
            let model = Net8DeviceModelV1::upload_v1(&device, &host).expect("upload");
            // Typical decisions, then large ones: row chunks past the tile's
            // rows and index-adds with more destinations than shared memory
            // holds.
            let decisions: Vec<RandomDecision> = (0..500)
                .map(|i| random_decision(net_seed * 10_000 + i))
                .chain((0..12).map(|i| {
                    random_decision_sized(net_seed * 10_000 + 5_000 + i, [260, 400, 230, 300])
                }))
                .collect();
            batch.clear();
            for decision in &decisions {
                batch
                    .push_v1(0, &net, ForwardPathV1::Flat, decision.view())
                    .expect("pack");
            }
            let outputs = forward_batch_v1(&device, kernels, &[&model], &batch, &mut buffers)
                .expect("forward");
            for (index, (decision, gpu)) in decisions.iter().zip(&outputs).enumerate() {
                let cpu = net.forward_v1(decision.view()).expect("cpu forward");
                assert_eq!(
                    bits(&gpu.logits),
                    bits(&cpu.logits),
                    "{kernels:?} {tile:?} net {net_seed} decision {index} logits"
                );
                assert_eq!(
                    gpu.value.to_bits(),
                    cpu.value.to_bits(),
                    "{kernels:?} {tile:?} net {net_seed} decision {index} value"
                );
            }
        }
    }
}

/// Each request's reply is the same alone and inside batches of any size,
/// position and model mix.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn gpu_replies_do_not_depend_on_batch_composition() {
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    // Ten models, all served by one v2 launch per batch.
    let nets: Vec<NativePolicyValueNetV1> = (7..17).map(random_net).collect();
    let hosts: Vec<Net8HostModelV1> = nets
        .iter()
        .map(|n| Net8HostModelV1::from_net_v1(n).expect("host"))
        .collect();
    let models: Vec<Net8DeviceModelV1> = hosts
        .iter()
        .map(|h| Net8DeviceModelV1::upload_v1(&device, h).expect("upload"))
        .collect();
    let model_refs: Vec<&Net8DeviceModelV1> = models.iter().collect();
    let pool: Vec<RandomDecision> = (0..400).map(|i| random_decision(50_000 + i)).collect();
    let mut buffers = DeviceBuffersV1::default();
    let mut batch = PackedBatchV1::default();
    // Batch of one for every (decision, model).
    let mut alone = Vec::new();
    for (i, decision) in pool.iter().enumerate() {
        let slot = i % nets.len();
        batch.clear();
        batch
            .push_v1(slot, &nets[slot], ForwardPathV1::Flat, decision.view())
            .expect("pack");
        alone.push(
            forward_batch_v1(&device, KernelsV1::V2, &model_refs, &batch, &mut buffers)
                .expect("forward")
                .remove(0),
        );
    }
    let mut rng = SplitMix64::seed(99);
    for &size in &[2usize, 7, 64, 256, 400] {
        for _trial in 0..4 {
            let picks: Vec<usize> = (0..size)
                .map(|_| (rng.next_u64() % pool.len() as u64) as usize)
                .collect();
            batch.clear();
            for &i in &picks {
                let slot = i % nets.len();
                batch
                    .push_v1(slot, &nets[slot], ForwardPathV1::Flat, pool[i].view())
                    .expect("pack");
            }
            let outputs =
                forward_batch_v1(&device, KernelsV1::V2, &model_refs, &batch, &mut buffers)
                    .expect("forward");
            for (&i, output) in picks.iter().zip(&outputs) {
                assert_eq!(
                    bits(&output.logits),
                    bits(&alone[i].logits),
                    "size {size} decision {i}"
                );
                assert_eq!(
                    output.value.to_bits(),
                    alone[i].value.to_bits(),
                    "size {size} decision {i}"
                );
            }
        }
    }
}

/// Through the service: many client threads, three latency windows; every
/// reply equals the CPU forward.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn service_replies_equal_cpu_under_any_timing() {
    let nets = Arc::new([random_net(11), random_net(12)]);
    let hosts: Arc<Vec<Arc<Net8HostModelV1>>> = Arc::new(
        nets.iter()
            .map(|n| Arc::new(Net8HostModelV1::from_net_v1(n).expect("host")))
            .collect(),
    );
    for latency in [0u64, 200, 5_000] {
        let service = InferenceServiceV1::start_v1(
            ordinal(),
            BatchPolicyV1 {
                max_latency: Duration::from_micros(latency),
                ..BatchPolicyV1::default()
            },
        )
        .expect("service");
        std::thread::scope(|s| {
            for worker in 0..8u64 {
                let mut client = service.client_v1();
                let nets = Arc::clone(&nets);
                let hosts = Arc::clone(&hosts);
                s.spawn(move || {
                    for i in 0..60u64 {
                        let decision = random_decision(worker * 1_000 + i);
                        let slot = ((worker + i) % 2) as usize;
                        let gpu = client
                            .forward_v1(
                                &hosts[slot],
                                &nets[slot],
                                ForwardPathV1::Flat,
                                decision.view(),
                            )
                            .expect("service forward");
                        let cpu = nets[slot].forward_v1(decision.view()).expect("cpu forward");
                        assert_eq!(bits(&gpu.logits), bits(&cpu.logits));
                        assert_eq!(gpu.value.to_bits(), cpu.value.to_bits());
                    }
                });
            }
        });
        let stats = service.stats_v1().expect("stats");
        assert_eq!(stats.requests, 8 * 60);
        eprintln!("latency {latency} us: {stats:?}");
    }
}

/// Timing receipt for the design note (not a pass/fail check): the fused
/// net-8 forward per batch (pack, upload, launch, download) at several batch
/// sizes, and the CPU forward per decision on this thread.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL); timing only"]
fn gpu_forward_timing_receipt() {
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let net = random_net(21);
    let host = Net8HostModelV1::from_net_v1(&net).expect("host");
    let model = Net8DeviceModelV1::upload_v1(&device, &host).expect("upload");
    let pool: Vec<RandomDecision> = (0..512).map(|i| random_decision(70_000 + i)).collect();
    let mut buffers = DeviceBuffersV1::default();
    let mut batch = PackedBatchV1::default();
    let started = std::time::Instant::now();
    for decision in &pool {
        net.forward_v1(decision.view()).expect("cpu");
    }
    let cpu_us = started.elapsed().as_secs_f64() * 1e6 / pool.len() as f64;
    println!("{{\"probe\":\"net8_cpu_forward\",\"us_per_decision\":{cpu_us:.1}}}");
    for kernels in [KernelsV1::V1, KernelsV1::V2] {
        for &size in &[1usize, 2, 4, 10, 20, 64, 256, 512] {
            let mut samples = Vec::new();
            for trial in 0..60 {
                let t = std::time::Instant::now();
                batch.clear();
                for decision in pool.iter().cycle().skip(trial * 7).take(size) {
                    batch
                        .push_v1(0, &net, ForwardPathV1::Flat, decision.view())
                        .expect("pack");
                }
                forward_batch_v1(&device, kernels, &[&model], &batch, &mut buffers)
                    .expect("forward");
                if trial >= 10 {
                    samples.push(t.elapsed().as_secs_f64() * 1e6);
                }
            }
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let p50 = samples[samples.len() / 2];
            println!(
                "{{\"probe\":\"net8_gpu_forward\",\"kernels\":\"{kernels:?}\",\"batch\":{size},\"p50_us\":{p50:.1},\"us_per_decision\":{:.2}}}",
                p50 / size as f64
            );
        }
    }
}

/// Twelve models pending at once (above `max_models` = 10): the service runs
/// them in groups and every reply still equals the CPU forward.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn service_serves_more_models_than_resident_slots() {
    let nets: Arc<Vec<NativePolicyValueNetV1>> =
        Arc::new((0..12).map(|i| random_net(300 + i)).collect());
    let hosts: Arc<Vec<Arc<Net8HostModelV1>>> = Arc::new(
        nets.iter()
            .map(|n| Arc::new(Net8HostModelV1::from_net_v1(n).expect("host")))
            .collect(),
    );
    let service = InferenceServiceV1::start_v1(
        ordinal(),
        BatchPolicyV1 {
            max_latency: Duration::from_millis(20),
            ..BatchPolicyV1::default()
        },
    )
    .expect("service");
    let barrier = std::sync::Barrier::new(12);
    std::thread::scope(|s| {
        for worker in 0..12usize {
            let mut client = service.client_v1();
            let (nets, hosts, barrier) = (Arc::clone(&nets), Arc::clone(&hosts), &barrier);
            s.spawn(move || {
                for i in 0..6u64 {
                    let decision = random_decision(9_000 + worker as u64 * 100 + i);
                    barrier.wait();
                    let gpu = client
                        .forward_v1(
                            &hosts[worker],
                            &nets[worker],
                            ForwardPathV1::Flat,
                            decision.view(),
                        )
                        .expect("service forward");
                    let cpu = nets[worker]
                        .forward_v1(decision.view())
                        .expect("cpu forward");
                    assert_eq!(bits(&gpu.logits), bits(&cpu.logits));
                    assert_eq!(gpu.value.to_bits(), cpu.value.to_bits());
                }
            });
        }
    });
    let stats = service.stats_v1().expect("stats");
    assert_eq!(stats.requests, 72);
    assert!(
        stats.model_uploads > 12,
        "eviction and re-upload expected: {stats:?}"
    );
    eprintln!("{stats:?}");
}

/// A service that dies with a request pending fails the waiting client at
/// once instead of leaving it blocked.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn a_service_fault_fails_the_waiting_client() {
    let net = random_net(41);
    let host = Arc::new(Net8HostModelV1::from_net_v1(&net).expect("host"));
    let service =
        InferenceServiceV1::start_v1(ordinal(), BatchPolicyV1::default()).expect("service");
    let mut client = service.client_v1();
    service.panic_on_next_request_for_test_v1();
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(move || {
            let decision = random_decision(1);
            let _ = done.send(client.forward_v1(&host, &net, ForwardPathV1::Flat, decision.view()));
        });
        let reply = wait
            .recv_timeout(Duration::from_secs(30))
            .expect("the client must not hang");
        assert!(reply.is_err(), "a dead service cannot answer");
    });
    // The receipt of a dead service still names its device and says why it
    // has no counters.
    let receipt = super::service::service_receipt_v1(&service);
    assert!(receipt["device"]["program_sha256"].is_string(), "{receipt}");
    assert!(receipt["stats_error"].is_string(), "{receipt}");
    // Its thread panicked by design; dropping would only join it.
    std::mem::forget(service);
}

/// After a real device fault the service fails the batch and every later
/// request, without retrying on the poisoned context. The fault leaves this
/// process's CUDA context unusable, so the test runs only when asked for
/// alone: `MTG_KERNEL_GPU_EXACT_FAULT_TEST=1 ... -- --exact --ignored
/// gpu_exact_v1::tests::a_device_fault_poisons_the_service`.
#[test]
#[ignore = "needs a CUDA device and its own process (MTG_KERNEL_GPU_EXACT_FAULT_TEST=1)"]
fn a_device_fault_poisons_the_service() {
    if std::env::var_os("MTG_KERNEL_GPU_EXACT_FAULT_TEST").is_none() {
        eprintln!("skipped: set MTG_KERNEL_GPU_EXACT_FAULT_TEST=1 and run this test alone");
        return;
    }
    let net = random_net(43);
    let host = Arc::new(Net8HostModelV1::from_net_v1(&net).expect("host"));
    let service =
        InferenceServiceV1::start_v1(ordinal(), BatchPolicyV1::default()).expect("service");
    let mut client = service.client_v1();
    let decision = random_decision(2);
    let before = client
        .forward_v1(&host, &net, ForwardPathV1::Flat, decision.view())
        .expect("a healthy service answers");
    let cpu = net.forward_v1(decision.view()).expect("cpu forward");
    assert_eq!(bits(&before.logits), bits(&cpu.logits));
    service.fault_before_next_batch_for_test_v1();
    for attempt in 0..3 {
        let reply = client.forward_v1(&host, &net, ForwardPathV1::Flat, decision.view());
        let error = reply.expect_err("a faulted device must not answer");
        eprintln!("attempt {attempt}: {error}");
        if attempt > 0 {
            assert!(error.contains("stopped after a device error"), "{error}");
        }
    }
    let stats = service
        .stats_v1()
        .expect("a poisoned service still reports");
    assert!(stats.poisoned.is_some(), "{stats:?}");
    std::mem::forget(service);
}

/// A net whose outputs overflow fails closed on both paths.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn non_finite_outputs_fail_closed_like_the_cpu() {
    let mut net = random_net(42);
    let mut snapshot = net.parameter_snapshot_v1();
    for parameter in &mut snapshot {
        if parameter.name == "scorer.2.weight" || parameter.name == "value_head.2.weight" {
            parameter.values.fill(f32::MAX);
        }
    }
    net.replace_parameter_snapshot_v1(&snapshot)
        .expect("replace");
    let host = Arc::new(Net8HostModelV1::from_net_v1(&net).expect("host"));
    let service =
        InferenceServiceV1::start_v1(ordinal(), BatchPolicyV1::default()).expect("service");
    let mut client = service.client_v1();
    let mut refused = 0;
    for seed in 0..20 {
        let decision = random_decision(500 + seed);
        let cpu = net.forward_v1(decision.view());
        let gpu = client.forward_v1(&host, &net, ForwardPathV1::Flat, decision.view());
        assert_eq!(cpu.is_err(), gpu.is_err(), "seed {seed}");
        refused += usize::from(gpu.is_err());
    }
    assert!(
        refused > 0,
        "the overflowing net must produce non-finite outputs"
    );
}

/// The policy hook: a collector thread's forwards go to the installed
/// service and equal the CPU forward, also after each site that replaces the
/// policy's parameters (which must clear its cached host copy); a V8 policy
/// keeps the CPU forward.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn policy_hook_follows_parameter_replacement_and_falls_back_exactly() {
    use crate::rl_session::FastActorSessionV1;
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;
    if super::inference_receipt_v1().is_none() {
        super::install_inference_service_v1(ordinal(), 200).expect("install");
    }
    let session = || {
        let (mut state, hunter, _, _) =
            crate::rl_session::avenging_hunter_undercity_arena_choose_targets_state_v1(false);
        crate::rl_session::shuffle_trigger_source_into_library_v1(
            &mut state,
            hunter,
            crate::ids::PlayerId::P0,
        );
        FastActorSessionV1::from_v3_fixture_state(state)
    };
    let requests = || {
        super::inference_receipt_v1().unwrap()["requests"]
            .as_u64()
            .unwrap()
    };
    let perturb = |policy: &FrozenPlayPolicyV1| {
        let mut parameters = policy.training_parameters_v3();
        for parameter in &mut parameters {
            if parameter.name == "object_encoder.0.weight" {
                for value in parameter.values.iter_mut().step_by(5) {
                    *value *= 1.25;
                }
            }
        }
        parameters
    };
    let score_bits = |policy: &mut FrozenPlayPolicyV1| {
        let scores = policy.score_fast_session_v1(&session()).expect("score");
        (bits(&scores.logits), scores.value.to_bits())
    };
    // CPU references run on threads without a client.
    let on_cpu = |policy: &mut FrozenPlayPolicyV1| {
        std::thread::scope(|t| t.spawn(|| score_bits(policy)).join().unwrap())
    };
    std::thread::scope(|s| {
        s.spawn(|| {
            let _guard = super::service::enter_collector_v1();
            let mut cpu = FrozenPlayPolicyV1::training_fixture_v3();
            let mut gpu = FrozenPlayPolicyV1::training_fixture_v3();
            let original = on_cpu(&mut cpu);
            let before = requests();
            assert_eq!(score_bits(&mut gpu), original);
            assert!(
                requests() > before,
                "the collector thread must use the service"
            );
            // replace_training_parameters_v3 clears the cached host copy.
            let parameters = perturb(&gpu);
            cpu.replace_training_parameters_v3(&parameters)
                .expect("cpu replace");
            gpu.replace_training_parameters_v3(&parameters)
                .expect("gpu replace");
            let replaced = on_cpu(&mut cpu);
            assert_ne!(
                replaced, original,
                "the perturbation must change the forward"
            );
            assert_eq!(
                score_bits(&mut gpu),
                replaced,
                "after replace_training_parameters_v3"
            );
            // apply_no_state_digest_v1 converts the parameters and clears it too.
            cpu.apply_no_state_digest_v1("converted".into(), 123..219, 1_499, true)
                .expect("cpu convert");
            gpu.apply_no_state_digest_v1("converted".into(), 123..219, 1_499, true)
                .expect("gpu convert");
            let converted = on_cpu(&mut cpu);
            assert_eq!(
                score_bits(&mut gpu),
                converted,
                "after apply_no_state_digest_v1"
            );
            // into_flat_v8_v1: the V8 net never reaches the service.
            let mut cpu = FrozenPlayPolicyV1::training_fixture_v4()
                .into_flat_v8_v1()
                .expect("v8");
            let mut gpu = FrozenPlayPolicyV1::training_fixture_v4()
                .into_flat_v8_v1()
                .expect("v8");
            let v8 = on_cpu(&mut cpu);
            let before = requests();
            assert_eq!(score_bits(&mut gpu), v8, "V8 falls back to the CPU forward");
            assert_eq!(
                requests(),
                before,
                "a V8 forward must not reach the service"
            );
        });
    });
}

/// Concurrent invocation receipts never overwrite each other.
#[test]
fn invocation_receipts_reserve_distinct_files() {
    let root = std::env::temp_dir().join(format!("gpu-exact-receipts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| super::invocation_receipt_v1(&root, Err("no run")).expect("receipt"));
        }
    });
    let written = std::fs::read_dir(root.join("gpu-exact-receipts"))
        .expect("dir")
        .count();
    assert_eq!(written, 8);
    let _ = std::fs::remove_dir_all(&root);
}

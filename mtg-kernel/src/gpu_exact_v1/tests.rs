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

/// Below sm_80 the program leaves out the v2 source, whose `cp.async` such a
/// device cannot load; v1 and the learner kernels compile for compute_75
/// without it. With v2 the source is the one the program hash always named.
#[test]
#[ignore = "needs NVRTC (the CUDA runtime DLLs), not a device"]
fn the_program_below_sm_80_has_no_v2_and_compiles_for_compute_75() {
    let tile = TileShapeV1::DEFAULT;
    let (with, _) = super::device::source_and_options(tile, true);
    let (without, options) = super::device::source_and_options(tile, false);
    assert!(with.contains("mtg_net8_forward_v2") && with.contains("cp.async"));
    assert!(!without.contains("mtg_net8_forward_v2") && !without.contains("cp.async"));
    assert!(without.contains("mtg_fault_trap"));
    let ptx = cudarc::nvrtc::compile_ptx_with_opts(
        &without,
        cudarc::nvrtc::CompileOptions {
            arch: Some("compute_75"),
            options,
            ..Default::default()
        },
    )
    .expect("v1 program for compute_75")
    .to_src();
    for kernel in [
        "mtg_net8_forward",
        "mtg_net8_reverse",
        "mtg_net8_param_grad",
        "mtg_net8_embedding_grad",
        "mtg_tanh_proof",
        "mtg_fault_trap",
    ] {
        assert!(ptx.contains(&format!(".entry {kernel}(")), "{kernel}");
    }
    assert!(!ptx.contains("cp.async") && !ptx.contains("mtg_net8_forward_v2"));
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

/// One request per client, each for its own model, released together.
fn one_round(
    nets: &[NativePolicyValueNetV1],
    hosts: &[Arc<Net8HostModelV1>],
    clients: &mut [super::service::InferenceClientV1],
    seed: u64,
) -> Vec<Result<(), String>> {
    let barrier = std::sync::Barrier::new(clients.len());
    std::thread::scope(|s| {
        let handles: Vec<_> = clients
            .iter_mut()
            .enumerate()
            .map(|(worker, client)| {
                let (net, host, barrier) = (&nets[worker], &hosts[worker], &barrier);
                s.spawn(move || {
                    let decision = random_decision(seed + worker as u64);
                    barrier.wait();
                    client
                        .forward_v1(host, net, ForwardPathV1::Flat, decision.view())
                        .map(|_| ())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("client thread"))
            .collect()
    })
}

/// A device fault in a batch of several model groups (`max_models` 1, three
/// models) fails every request of the batch and starts no later group, so no
/// device work follows the error. Same isolation as the test above:
/// `MTG_KERNEL_GPU_EXACT_FAULT_TEST=1 ... -- --exact --ignored
/// gpu_exact_v1::tests::a_device_fault_stops_a_multi_group_batch`.
#[test]
#[ignore = "needs a CUDA device and its own process (MTG_KERNEL_GPU_EXACT_FAULT_TEST=1)"]
fn a_device_fault_stops_a_multi_group_batch() {
    if std::env::var_os("MTG_KERNEL_GPU_EXACT_FAULT_TEST").is_none() {
        eprintln!("skipped: set MTG_KERNEL_GPU_EXACT_FAULT_TEST=1 and run this test alone");
        return;
    }
    let nets: Vec<NativePolicyValueNetV1> = (0..3).map(|i| random_net(500 + i)).collect();
    let hosts: Vec<Arc<Net8HostModelV1>> = nets
        .iter()
        .map(|n| Arc::new(Net8HostModelV1::from_net_v1(n).expect("host")))
        .collect();
    // The window waits for every client, so each round is one batch.
    let service = InferenceServiceV1::start_v1(
        ordinal(),
        BatchPolicyV1 {
            max_latency: Duration::from_millis(500),
            max_models: 1,
            ..BatchPolicyV1::default()
        },
    )
    .expect("service");
    let mut clients: Vec<_> = (0..3).map(|_| service.client_v1()).collect();
    let healthy = one_round(&nets, &hosts, &mut clients, 700);
    assert!(healthy.iter().all(Result::is_ok), "{healthy:?}");
    let before = service.stats_v1().expect("stats");
    assert_eq!(before.batches, 1, "one batch: {before:?}");
    assert_eq!(before.groups, 3, "one group per model: {before:?}");
    service.fault_before_next_batch_for_test_v1();
    let faulted = one_round(&nets, &hosts, &mut clients, 800);
    let stats = service
        .stats_v1()
        .expect("a poisoned service still reports");
    eprintln!("{faulted:?}\n{stats:?}");
    assert_eq!(stats.batches, 2, "one faulted batch: {stats:?}");
    for reply in &faulted {
        let error = reply
            .as_ref()
            .expect_err("no reply survives a faulted batch");
        assert!(error.contains("gpu-exact device"), "{error}");
    }
    assert_eq!(
        stats.groups - before.groups,
        1,
        "no group after the device error: {stats:?}"
    );
    assert!(stats.poisoned.is_some(), "{stats:?}");
    std::mem::forget(clients);
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

fn random_s_decision_v1(
    rng: &mut crate::s_transformer_v1::SplitMixV1,
    card_vocab: usize,
    limits: [usize; 4],
) -> crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2 {
    use crate::s_transformer_v1::{
        ACTION_FEATURE_DIM_V1, EDGE_FEATURE_DIM_V1, OBJECT_FEATURE_DIM_V1, OBJECT_GROUP_COUNT_V1,
        REF_FEATURE_DIM_V1, STATE_DIM_V1,
    };
    let n = 1 + (rng.below(limits[0] as u64) as usize);
    let e = rng.below((limits[1] + 1) as u64) as usize;
    let a = 1 + (rng.below(limits[2] as u64) as usize);
    let r = rng.below((limits[3] + 1) as u64) as usize;
    let f = |rng: &mut crate::s_transformer_v1::SplitMixV1| match rng.below(12) {
        0..=4 => 0.0f32,
        5 => -0.0f32,
        6 => 1.0f32,
        7 => -1.0f32,
        _ => rng.uniform(-1.5, 1.5),
    };
    let card = |rng: &mut crate::s_transformer_v1::SplitMixV1| {
        if rng.below(5) == 0 {
            0i64
        } else {
            1 + rng.below((card_vocab - 1) as u64) as i64
        }
    };
    crate::native_flat_tensorizer_v2::NativeFlatDecisionTensorV2 {
        state: (0..STATE_DIM_V1).map(|_| f(rng)).collect(),
        object_features: (0..n * OBJECT_FEATURE_DIM_V1).map(|_| f(rng)).collect(),
        object_card_ids: (0..n).map(|_| card(rng)).collect(),
        object_groups: (0..n)
            .map(|_| rng.below(OBJECT_GROUP_COUNT_V1 as u64) as i64)
            .collect(),
        object_node_ids: (0..n as i64).collect(),
        edge_features: (0..e * EDGE_FEATURE_DIM_V1).map(|_| f(rng)).collect(),
        edge_source_indices: (0..e).map(|_| rng.below(n as u64) as i64).collect(),
        edge_target_indices: (0..e).map(|_| rng.below(n as u64) as i64).collect(),
        action_features: (0..a * ACTION_FEATURE_DIM_V1).map(|_| f(rng)).collect(),
        action_ref_features: (0..r * REF_FEATURE_DIM_V1).map(|_| f(rng)).collect(),
        action_ref_card_ids: (0..r).map(|_| card(rng)).collect(),
        action_ref_action_indices: (0..r).map(|_| rng.below(a as u64) as i64).collect(),
        action_ref_node_indices: (0..r).map(|_| rng.below(n as u64) as i64).collect(),
    }
}

#[test]
fn s_packed_batch_append_rebases_all_offsets_and_preserves_counts() {
    use super::s_transformer::SPackedBatchV1;
    use crate::s_transformer_v1::{SConfigV1, SInputV1, SplitMixV1};
    let (src, _) = super::device::s_source_and_options();
    assert!(
        !src.contains("cp.async"),
        "S CUDA source must avoid cp.async"
    );
    let config = SConfigV1::s_v1();
    let mut rng = SplitMixV1::new(404);
    let mut single = SPackedBatchV1::default();
    let mut merged = SPackedBatchV1::default();
    let mut total_actions = 0usize;
    for idx in 0..6usize {
        let t = random_s_decision_v1(&mut rng, config.card_vocab, [24, 12, 8, 10]);
        single.clear();
        let counts = single
            .push_infer_v1(0, &config, SInputV1::of_tensor_v1(&t))
            .expect("pack infer");
        total_actions += counts.actions;
        merged.append_v1(&single, idx % 3).expect("append");
    }
    assert_eq!(merged.len(), 6);
    assert_eq!(merged.logits_len, total_actions);
}

#[test]
#[ignore = "needs NVRTC (the CUDA runtime DLLs), not a device"]
fn s_program_compiles_for_compute_75_without_cp_async() {
    let (src, options) = super::device::s_source_and_options();
    assert!(!src.contains("cp.async"));
    let ptx = cudarc::nvrtc::compile_ptx_with_opts(
        src,
        cudarc::nvrtc::CompileOptions {
            arch: Some("compute_75"),
            options,
            ..Default::default()
        },
    )
    .expect("S program for compute_75")
    .to_src();
    for kernel in [
        "mtg_s_forward",
        "mtg_s_reverse",
        "mtg_s_linear_grad",
        "mtg_s_ln_grad",
        "mtg_s_group_refbias_grad",
        "mtg_s_card_grad",
    ] {
        assert!(ptx.contains(&format!(".entry {kernel}(")), "{kernel}");
    }
    assert!(!ptx.contains("cp.async"));
}

/// C1: `LearnerBuffersV1` reuses persistent host + device allocations across
/// updates of varying batch sizes without changing a single gradient bit.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn net8_learner_persistent_buffers_match_fresh_buffers_across_resized_updates() {
    use super::net8_learner::{net8_gradients_v1, LearnerBuffersV1, LearnerSubstepV1};
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let mut persistent = LearnerBuffersV1::default();

    for (round, count) in [6usize, 48, 12, 64].into_iter().enumerate() {
        let net = random_net(900 + round as u64);
        let snapshot = net.parameter_snapshot_v1();
        let tensor_refs: Vec<&[f32]> = snapshot.iter().map(|p| p.values.as_slice()).collect();
        let decisions: Vec<RandomDecision> = (0..count)
            .map(|i| random_decision(20_000 + round as u64 * 500 + i as u64))
            .collect();
        let mut outs = Vec::with_capacity(count);
        let mut d_logits_vec = Vec::with_capacity(count);
        let mut rng = SplitMix64::seed(77 + round as u64);
        for d in &decisions {
            let out = net.forward_v1(d.view()).expect("cpu forward");
            let dl: Vec<f32> = (0..out.logits.len())
                .map(|_| unit(&mut rng) * 0.25)
                .collect();
            outs.push(out);
            d_logits_vec.push(dl);
        }
        let substeps: Vec<LearnerSubstepV1<'_>> = decisions
            .iter()
            .zip(&outs)
            .zip(&d_logits_vec)
            .map(|((d, out), dl)| LearnerSubstepV1 {
                view: d.view(),
                logits: &out.logits,
                value: out.value,
                d_logits: dl,
                d_value: unit(&mut rng) * 0.5,
            })
            .collect();

        let reused = net8_gradients_v1(
            &device,
            &mut persistent,
            net.config_v1(),
            &tensor_refs,
            &substeps,
        )
        .expect("reused net8 learner");
        let fresh = net8_gradients_v1(
            &device,
            &mut LearnerBuffersV1::default(),
            net.config_v1(),
            &tensor_refs,
            &substeps,
        )
        .expect("fresh net8 learner");
        assert_eq!(reused.len(), fresh.len());
        for (ti, (a, b)) in reused.iter().zip(&fresh).enumerate() {
            assert_eq!(bits(a), bits(b), "round {round} tensor {ti}");
        }
    }
}

/// C2: `forward_s_batch_v1` matches CPU `SModelV1::forward_into_v1` bit for bit
/// across standard S (`layers = 4`) and odd-layer (`layers = 3`) configs, edge
/// cases, multi-model batches, and non-finite fail-closed checks.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn s_gpu_forward_equals_cpu_forward_bit_for_bit() {
    use super::s_transformer::{
        forward_s_batch_v1, SDeviceBuffersV1, SDeviceModelV1, SHostModelV1, SPackedBatchV1,
    };
    use crate::s_transformer_v1::{SConfigV1, SInputV1, SModelV1, STapeV1, SplitMixV1};

    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let mut buffers = SDeviceBuffersV1::default();
    let mut batch = SPackedBatchV1::default();
    let mut tape = STapeV1::default();

    let odd_cfg = SConfigV1 {
        d_model: 64,
        layers: 3,
        heads: 4,
        ffn: 192,
        card_dim: 16,
        card_vocab: 193,
    };
    for (cfg_idx, cfg) in [SConfigV1::s_v1(), odd_cfg].into_iter().enumerate() {
        let mut rng = SplitMixV1::new(1_000 + cfg_idx as u64);
        let models: Vec<SModelV1> = (0..10)
            .map(|m| SModelV1::init_pilot_scheme_v1(cfg, 500 + m + cfg_idx as u64 * 20).unwrap())
            .collect();
        let hosts: Vec<SHostModelV1> = models.iter().map(SHostModelV1::from_model_v1).collect();
        let devs: Vec<SDeviceModelV1> = hosts
            .iter()
            .map(|h| SDeviceModelV1::upload_v1(&device, h).expect("upload S"))
            .collect();
        let dev_refs: Vec<&SDeviceModelV1> = devs.iter().collect();

        // Explicit boundary shapes: 0 edges / 0 refs, 1 action, dense edges/refs, large token count.
        let mut decisions = vec![
            random_s_decision_v1(&mut rng, cfg.card_vocab, [1, 0, 1, 0]),
            random_s_decision_v1(&mut rng, cfg.card_vocab, [12, 0, 6, 8]),
            random_s_decision_v1(&mut rng, cfg.card_vocab, [12, 10, 6, 0]),
            random_s_decision_v1(&mut rng, cfg.card_vocab, [90, 40, 24, 30]),
        ];
        for _ in 0..200 {
            decisions.push(random_s_decision_v1(
                &mut rng,
                cfg.card_vocab,
                [48, 20, 14, 16],
            ));
        }

        batch.clear();
        for (i, d) in decisions.iter().enumerate() {
            batch
                .push_infer_v1(i % models.len(), &cfg, SInputV1::of_tensor_v1(d))
                .expect("pack S");
        }
        let gpu_outs =
            forward_s_batch_v1(&device, &dev_refs, &batch, &mut buffers).expect("gpu S forward");
        assert_eq!(gpu_outs.len(), decisions.len());
        for (i, (d, gpu)) in decisions.iter().zip(&gpu_outs).enumerate() {
            let model = &models[i % models.len()];
            let cpu = model
                .forward_v1(SInputV1::of_tensor_v1(d), &mut tape)
                .expect("cpu S forward");
            assert_eq!(
                bits(&gpu.logits),
                bits(&cpu.logits),
                "cfg {cfg_idx} decision {i} logits"
            );
            assert_eq!(
                gpu.value.to_bits(),
                cpu.value.to_bits(),
                "cfg {cfg_idx} decision {i} value"
            );
            assert_eq!(
                gpu.value_logit.to_bits(),
                cpu.value_logit.to_bits(),
                "cfg {cfg_idx} decision {i} value_logit"
            );
        }
    }

    // Overflowing S model fails closed on the service just like CPU.
    let cfg = SConfigV1::s_v1();
    let base = SModelV1::init_pilot_scheme_v1(cfg, 99).unwrap();
    let mut bad_vals = base.values_v1().to_vec();
    let pol2_w = base.layout_v1().pol2.w;
    bad_vals[pol2_w..pol2_w + cfg.d_model].fill(f32::MAX);
    let bad_model = SModelV1::from_values_v1(cfg, bad_vals).expect("from_values");
    let bad_host = Arc::new(SHostModelV1::from_model_v1(&bad_model));
    let service =
        InferenceServiceV1::start_v1(ordinal(), BatchPolicyV1::default()).expect("service");
    let mut client = service.client_v1();
    let mut rng = SplitMixV1::new(777);
    let mut refused = 0usize;
    for i in 0..20 {
        let d = random_s_decision_v1(&mut rng, cfg.card_vocab, [20, 6, 6, 6]);
        let cpu = bad_model.forward_v1(SInputV1::of_tensor_v1(&d), &mut tape);
        let gpu = client.forward_s_v1(&bad_host, SInputV1::of_tensor_v1(&d));
        assert_eq!(cpu.is_err(), gpu.is_err(), "overflow decision {i}");
        refused += usize::from(gpu.is_err());
    }
    assert!(refused > 0, "overflowing S model must fail closed");
}

/// C2: Mixed Net-8 + S concurrent batches on one `InferenceServiceV1`, plus the
/// `FrozenPlayPolicyV1` hook (`install_s_model_v1` and `PolicyCacheV1`).
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn s_service_and_policy_hook_equal_cpu_under_mixed_net8_and_s_batches() {
    use super::s_transformer::SHostModelV1;
    use crate::rl_session::FastActorSessionV1;
    use crate::s_transformer_v1::{SConfigV1, SInputV1, SModelV1, STapeV1, SplitMixV1};
    use crate::sideboard_play_policy_v1::FrozenPlayPolicyV1;

    let net8 = Arc::new(random_net(313));
    let net8_host = Arc::new(Net8HostModelV1::from_net_v1(&net8).expect("net8 host"));
    let s_cfg = SConfigV1::s_v1();
    let s_models: Arc<Vec<SModelV1>> = Arc::new(
        (0..3)
            .map(|i| SModelV1::init_pilot_scheme_v1(s_cfg, 800 + i).unwrap())
            .collect(),
    );
    let s_hosts: Arc<Vec<Arc<SHostModelV1>>> = Arc::new(
        s_models
            .iter()
            .map(|m| Arc::new(SHostModelV1::from_model_v1(m)))
            .collect(),
    );

    let service = InferenceServiceV1::start_v1(
        ordinal(),
        BatchPolicyV1 {
            max_latency: Duration::from_micros(500),
            ..BatchPolicyV1::default()
        },
    )
    .expect("service");
    std::thread::scope(|s| {
        for worker in 0..8u64 {
            let mut client = service.client_v1();
            let net8 = Arc::clone(&net8);
            let net8_host = Arc::clone(&net8_host);
            let s_models = Arc::clone(&s_models);
            let s_hosts = Arc::clone(&s_hosts);
            s.spawn(move || {
                let mut rng = SplitMixV1::new(9_000 + worker);
                let mut tape = STapeV1::default();
                for step in 0..30u64 {
                    if (worker + step) % 2 == 0 {
                        let d = random_decision(worker * 1_000 + step);
                        let gpu = client
                            .forward_v1(&net8_host, &net8, ForwardPathV1::Flat, d.view())
                            .expect("gpu net8");
                        let cpu = net8.forward_v1(d.view()).expect("cpu net8");
                        assert_eq!(bits(&gpu.logits), bits(&cpu.logits));
                        assert_eq!(gpu.value.to_bits(), cpu.value.to_bits());
                    } else {
                        let slot = ((worker + step) as usize) % s_models.len();
                        let d = random_s_decision_v1(&mut rng, s_cfg.card_vocab, [32, 10, 8, 8]);
                        let gpu = client
                            .forward_s_v1(&s_hosts[slot], SInputV1::of_tensor_v1(&d))
                            .expect("gpu S");
                        let cpu = s_models[slot]
                            .forward_v1(SInputV1::of_tensor_v1(&d), &mut tape)
                            .expect("cpu S");
                        assert_eq!(bits(&gpu.logits), bits(&cpu.logits));
                        assert_eq!(gpu.value.to_bits(), cpu.value.to_bits());
                    }
                }
            });
        }
    });
    let stats = service.stats_v1().expect("stats");
    assert_eq!(stats.requests, 8 * 30);

    // Policy hook + PolicyCacheV1 integration on the global service.
    if super::inference_receipt_v1().is_none() {
        super::install_inference_service_v1(ordinal(), 200).expect("install global service");
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
    let s_arc = Arc::new(SModelV1::init_pilot_scheme_v1(s_cfg, 2026).unwrap());
    let mut cpu_pol = FrozenPlayPolicyV1::training_fixture_v4();
    cpu_pol
        .install_s_model_v1(Arc::clone(&s_arc))
        .expect("install S on cpu_pol");
    let expected = {
        let sc = cpu_pol
            .score_fast_session_v1(&session())
            .expect("cpu score");
        (bits(&sc.logits), sc.value.to_bits())
    };

    std::thread::scope(|t| {
        t.spawn(|| {
            let _guard = super::service::enter_collector_v1();
            let mut gpu_pol = FrozenPlayPolicyV1::training_fixture_v4();
            gpu_pol
                .install_s_model_v1(Arc::clone(&s_arc))
                .expect("install S on gpu_pol");
            let r0 = requests();
            let sc = gpu_pol
                .score_fast_session_v1(&session())
                .expect("gpu score");
            assert_eq!((bits(&sc.logits), sc.value.to_bits()), expected);
            let r1 = requests();
            assert!(r1 > r0, "uncached S policy must batch onto GPU service");

            // Attach a PolicyCacheV1: miss hits GPU, repeat hit stays in RAM.
            let shared = crate::inference_cache_v1::InferenceCacheV1::shared(4 * 1024 * 1024);
            gpu_pol.attach_inference_cache_v1(&shared).expect("attach");
            let sc_miss = gpu_pol.score_fast_session_v1(&session()).expect("miss");
            assert_eq!((bits(&sc_miss.logits), sc_miss.value.to_bits()), expected);
            let r2 = requests();
            assert!(r2 > r1, "cache miss must batch onto GPU service");
            let sc_hit = gpu_pol.score_fast_session_v1(&session()).expect("hit");
            assert_eq!((bits(&sc_hit.logits), sc_hit.value.to_bits()), expected);
            assert_eq!(requests(), r2, "cache hit must stay in RAM");
        });
    });
}

/// C3: GPU S learner forward+backward matches CPU serial, CPU parallel, W5
/// masked GAE, and EX1 auxiliary KL (`auxiliary_bias_residual`, `cap`,
/// `step_telemetry`, and `after_state_sha256`) bit for bit across all 460,346
/// parameters.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn s_gpu_learner_equals_cpu_gae_and_auxiliary_bit_for_bit() {
    use super::s_learner::{s_learner_gradient_v1, SLearnerBuffersV1};
    use crate::line_b_teacher_target_v1::{line_b_softmax_v1, LineBDivergenceV1};
    use crate::s_transformer_v1::{
        SAuxiliaryInputV1, SAuxiliaryRowV1, SConfigV1, SGroupV1, SInputV1, SModelV1, SSubstepV1,
        STrainStateV1, SWorkV1, SplitMixV1,
    };

    // 1. Direct `s_learner_gradient_v1` check across resized batches & odd/even layers.
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let mut s_buffers = SLearnerBuffersV1::default();
    let odd_cfg = SConfigV1 {
        d_model: 64,
        layers: 3,
        heads: 4,
        ffn: 192,
        card_dim: 16,
        card_vocab: 193,
    };
    for (cfg_idx, cfg) in [odd_cfg, SConfigV1::s_v1()].into_iter().enumerate() {
        let model = SModelV1::init_pilot_scheme_v1(cfg, 4_200 + cfg_idx as u64).unwrap();
        let mut rng = SplitMixV1::new(5_000 + cfg_idx as u64);
        for n_groups in [5usize, 28, 9, 36] {
            let mut tensors = Vec::new();
            let mut groups_meta = Vec::new();
            for _ in 0..n_groups {
                let subs = 1 + (rng.below(3) as usize);
                let start = tensors.len();
                for _ in 0..subs {
                    tensors.push(random_s_decision_v1(
                        &mut rng,
                        cfg.card_vocab,
                        [34, 12, 10, 12],
                    ));
                }
                groups_meta.push((
                    start..tensors.len(),
                    rng.uniform(-2.0, 2.0),
                    rng.uniform(-1.0, 1.0),
                ));
            }
            let groups: Vec<SGroupV1<'_>> = groups_meta
                .iter()
                .map(|(range, adv, vt)| SGroupV1 {
                    substeps: tensors[range.clone()]
                        .iter()
                        .enumerate()
                        .map(|(k, t)| {
                            let a = t.action_features.len()
                                / crate::s_transformer_v1::ACTION_FEATURE_DIM_V1;
                            SSubstepV1 {
                                input: SInputV1::of_tensor_v1(t),
                                selected: k % a,
                            }
                        })
                        .collect(),
                    advantage: *adv,
                    value_target: *vt,
                })
                .collect();

            let mut cpu_grad = vec![0.0f32; model.layout_v1().total];
            let cpu_loss = model
                .gae_loss_gradient_v1(&groups, 0.5, &mut cpu_grad, &mut SWorkV1::default())
                .expect("cpu gae");

            let flat_inputs: Vec<SInputV1<'_>> = groups
                .iter()
                .flat_map(|g| g.substeps.iter().map(|s| s.input))
                .collect();
            let group_count = groups.len() as f32;
            let ((gpu_pol_sum, gpu_val_sum), mut gpu_grad) = s_learner_gradient_v1(
                &device,
                &mut s_buffers,
                &model,
                &flat_inputs,
                |outputs, d_logits_all, d_dz_all| {
                    let mut p_sum = 0.0f32;
                    let mut v_sum = 0.0f32;
                    let mut cur = 0usize;
                    let mut row_dl = Vec::new();
                    for (gi, g) in groups.iter().enumerate() {
                        let d_joint = -g.advantage / group_count;
                        let mut joint = 0.0f32;
                        let mut verr = 0.0f32;
                        for (si, sub) in g.substeps.iter().enumerate() {
                            let out = &outputs[cur];
                            cur += 1;
                            let sel = crate::s_transformer_v1::gae_policy_row_into_v1(
                                &out.logits,
                                sub.selected,
                                d_joint,
                                gi,
                                si,
                                &mut row_dl,
                            )?;
                            d_logits_all.extend_from_slice(&row_dl);
                            joint = if si == 0 { sel } else { joint + sel };
                            let dv = if si == 0 {
                                verr = out.value - g.value_target;
                                (0.5 / group_count) * (2.0 * verr)
                            } else {
                                0.0
                            };
                            d_dz_all.push(dv * 0.5 * (1.0 - out.value * out.value));
                        }
                        p_sum += -joint * g.advantage;
                        v_sum += verr * verr;
                    }
                    Ok((p_sum, v_sum))
                },
            )
            .expect("gpu s_learner_gradient_v1");
            for range in model.layout_v1().gauge_ranges_v1() {
                gpu_grad[range].fill(0.0);
            }
            assert_eq!(gpu_pol_sum.to_bits(), cpu_loss.policy_sum.to_bits());
            assert_eq!(gpu_val_sum.to_bits(), cpu_loss.value_sum.to_bits());
            assert_eq!(
                bits(&gpu_grad),
                bits(&cpu_grad),
                "cfg {cfg_idx} n_groups {n_groups} gradient mismatch"
            );
        }
    }

    // 2. End-to-end `STrainStateV1` GAE + W5/EX1 auxiliary parity through global hook.
    let cfg = SConfigV1::s_v1();
    let mut rng = SplitMixV1::new(8_888);
    let base_model = SModelV1::init_pilot_scheme_v1(cfg, 8_888).unwrap();
    let initial = STrainStateV1::fresh_optimizer_v1(base_model);

    let mut group_tensors = Vec::new();
    let mut group_meta = Vec::new();
    for _ in 0..24 {
        let subs = 1 + (rng.below(3) as usize);
        let start = group_tensors.len();
        for _ in 0..subs {
            group_tensors.push(random_s_decision_v1(
                &mut rng,
                cfg.card_vocab,
                [36, 12, 10, 12],
            ));
        }
        group_meta.push((
            start..group_tensors.len(),
            rng.uniform(-1.5, 1.5),
            rng.uniform(-1.0, 1.0),
        ));
    }
    let groups: Vec<SGroupV1<'_>> = group_meta
        .iter()
        .map(|(range, adv, vt)| SGroupV1 {
            substeps: group_tensors[range.clone()]
                .iter()
                .enumerate()
                .map(|(k, t)| {
                    let a =
                        t.action_features.len() / crate::s_transformer_v1::ACTION_FEATURE_DIM_V1;
                    SSubstepV1 {
                        input: SInputV1::of_tensor_v1(t),
                        selected: k % a,
                    }
                })
                .collect(),
            advantage: *adv,
            value_target: *vt,
        })
        .collect();

    // Build 10 off-batch EX1 auxiliary rows (requiring >= 2 actions each).
    let mut aux_tensors = Vec::new();
    let mut aux_targets = Vec::new();
    while aux_tensors.len() < 10 {
        let t = random_s_decision_v1(&mut rng, cfg.card_vocab, [30, 10, 8, 10]);
        let a = t.action_features.len() / crate::s_transformer_v1::ACTION_FEATURE_DIM_V1;
        if a >= 2 {
            let teacher_logits: Vec<f64> =
                (0..a).map(|_| f64::from(rng.uniform(-2.0, 2.0))).collect();
            aux_targets.push(line_b_softmax_v1(&teacher_logits).expect("softmax"));
            aux_tensors.push(t);
        }
    }
    let make_aux_input = || SAuxiliaryInputV1 {
        direction: LineBDivergenceV1::Forward,
        coefficient: 0.25,
        denominator: 16,
        cap: Some(0.5),
        step_telemetry: true,
        rows: aux_tensors
            .iter()
            .zip(&aux_targets)
            .map(|(t, target)| SAuxiliaryRowV1 {
                input: SInputV1::of_tensor_v1(t),
                target,
            })
            .collect(),
        policy_mask: vec![(0, 0), (3, 0)],
    };

    // Compute CPU reference BEFORE installing the global GPU learner.
    assert!(
        !super::learner_installed_v1(),
        "run CPU reference before installing global GPU learner"
    );
    let mut cpu_gae = initial.clone();
    let cpu_step_1 = cpu_gae
        .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut SWorkV1::default(), 1)
        .expect("cpu gae step 1");
    let cpu_step_2 = cpu_gae
        .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut SWorkV1::default(), 3)
        .expect("cpu gae step 2");

    let mut cpu_aux_state = initial.clone();
    let (cpu_aux_step, cpu_aux_res) = cpu_aux_state
        .train_step_gae_auxiliary_with_workers_v1(
            &groups,
            0.5,
            1.0e-4,
            &make_aux_input(),
            &mut SWorkV1::default(),
            3,
        )
        .expect("cpu aux step");

    // Now install global GPU learner and verify identical hashes & receipts.
    super::install_learner_v1(ordinal()).expect("install GPU learner");
    let updates_before = super::learner_receipt_v1().unwrap()["updates"]
        .as_u64()
        .unwrap();

    let mut gpu_gae = initial.clone();
    let gpu_step_1 = gpu_gae
        .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut SWorkV1::default(), 1)
        .expect("gpu gae step 1");
    let gpu_step_2 = gpu_gae
        .train_step_gae_with_workers_v1(&groups, 0.5, 1.0e-4, &mut SWorkV1::default(), 3)
        .expect("gpu gae step 2");
    assert_eq!(gpu_step_1, cpu_step_1);
    assert_eq!(gpu_step_2, cpu_step_2);

    let mut gpu_aux_state = initial;
    let (gpu_aux_step, gpu_aux_res) = gpu_aux_state
        .train_step_gae_auxiliary_with_workers_v1(
            &groups,
            0.5,
            1.0e-4,
            &make_aux_input(),
            &mut SWorkV1::default(),
            3,
        )
        .expect("gpu aux step");
    assert_eq!(gpu_aux_step, cpu_aux_step);
    assert_eq!(
        gpu_aux_res.auxiliary_bias_residual.to_bits(),
        cpu_aux_res.auxiliary_bias_residual.to_bits()
    );
    assert_eq!(gpu_aux_res, cpu_aux_res);

    let updates_after = super::learner_receipt_v1().unwrap()["updates"]
        .as_u64()
        .unwrap();
    // 2 GAE updates + 1 masked GAE + 1 auxiliary KL pass = 4 GPU learner calls.
    assert_eq!(updates_after - updates_before, 4);
}

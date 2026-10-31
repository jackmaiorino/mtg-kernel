//! Device tests run only with a GPU:
//!   MTG_KERNEL_GPU_EXACT_TEST_ORDINAL=1 cargo test --release --features gpu-exact-v1
//!     --lib gpu_exact_v1 -- --ignored --test-threads 1
//! Host-only tests run everywhere.

use super::net8::{forward_batch_v1, DeviceBuffersV1, Net8DeviceModelV1, Net8HostModelV1, PackedBatchV1};
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
    let mut net = NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
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
    net.replace_parameter_snapshot_v1(&snapshot).expect("replace");
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
    let schema = NativeEncodedDecisionSchemaV1::contract_v1();
    let mut rng = SplitMix64::seed(seed);
    let below = |n: u64, rng: &mut SplitMix64| (rng.next_u64() % n) as i64;
    let objects = 1 + below(40, &mut rng) as usize;
    let edges = below(60, &mut rng) as usize;
    let actions = 1 + below(24, &mut rng) as usize;
    let refs = below(30, &mut rng) as usize;
    let floats = |n: usize, rng: &mut SplitMix64| (0..n).map(|_| unit(rng)).collect::<Vec<f32>>();
    RandomDecision {
        state: floats(schema.state_dim, &mut rng),
        object_features: floats(objects * schema.object_feature_dim, &mut rng),
        // Card 0 is padding; real ids span the vocabulary.
        object_card_ids: (0..objects).map(|_| below(65_537, &mut rng)).collect(),
        object_groups: (0..objects).map(|_| below(schema.object_group_count as u64, &mut rng)).collect(),
        object_node_ids: (0..objects as i64).collect(),
        edge_features: floats(edges * schema.edge_feature_dim, &mut rng),
        edge_sources: (0..edges).map(|_| below(objects as u64, &mut rng)).collect(),
        edge_targets: (0..edges).map(|_| below(objects as u64, &mut rng)).collect(),
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
        single.push_v1(0, &net, ForwardPathV1::Flat, decision.view()).expect("pack");
        merged.append_v1(&single, (seed % 2) as usize).expect("append");
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
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let mut buffers = DeviceBuffersV1::default();
    let mut batch = PackedBatchV1::default();
    for net_seed in 0..4u64 {
        let net = random_net(100 + net_seed);
        let host = Net8HostModelV1::from_net_v1(&net).expect("host model");
        let model = Net8DeviceModelV1::upload_v1(&device, &host).expect("upload");
        let decisions: Vec<RandomDecision> = (0..500).map(|i| random_decision(net_seed * 10_000 + i)).collect();
        batch.clear();
        for decision in &decisions {
            batch.push_v1(0, &net, ForwardPathV1::Flat, decision.view()).expect("pack");
        }
        let outputs = forward_batch_v1(&device, &[&model], &batch, &mut buffers).expect("forward");
        for (index, (decision, gpu)) in decisions.iter().zip(&outputs).enumerate() {
            let cpu = net.forward_v1(decision.view()).expect("cpu forward");
            assert_eq!(bits(&gpu.logits), bits(&cpu.logits), "net {net_seed} decision {index} logits");
            assert_eq!(gpu.value.to_bits(), cpu.value.to_bits(), "net {net_seed} decision {index} value");
        }
    }
}

/// Each request's reply is the same alone and inside batches of any size,
/// position and model mix.
#[test]
#[ignore = "needs a CUDA device (MTG_KERNEL_GPU_EXACT_TEST_ORDINAL)"]
fn gpu_replies_do_not_depend_on_batch_composition() {
    let device = ExactDeviceV1::open_v1(ordinal()).expect("device");
    let nets = [random_net(7), random_net(8), random_net(9)];
    let hosts: Vec<Net8HostModelV1> = nets.iter().map(|n| Net8HostModelV1::from_net_v1(n).expect("host")).collect();
    let models: Vec<Net8DeviceModelV1> =
        hosts.iter().map(|h| Net8DeviceModelV1::upload_v1(&device, h).expect("upload")).collect();
    let model_refs: Vec<&Net8DeviceModelV1> = models.iter().collect();
    let pool: Vec<RandomDecision> = (0..400).map(|i| random_decision(50_000 + i)).collect();
    let mut buffers = DeviceBuffersV1::default();
    let mut batch = PackedBatchV1::default();
    // Batch of one for every (decision, model).
    let mut alone = Vec::new();
    for (i, decision) in pool.iter().enumerate() {
        let slot = i % nets.len();
        batch.clear();
        batch.push_v1(slot, &nets[slot], ForwardPathV1::Flat, decision.view()).expect("pack");
        alone.push(forward_batch_v1(&device, &model_refs, &batch, &mut buffers).expect("forward").remove(0));
    }
    let mut rng = SplitMix64::seed(99);
    for &size in &[2usize, 7, 64, 256, 400] {
        for _trial in 0..4 {
            let picks: Vec<usize> = (0..size).map(|_| (rng.next_u64() % pool.len() as u64) as usize).collect();
            batch.clear();
            for &i in &picks {
                let slot = i % nets.len();
                batch.push_v1(slot, &nets[slot], ForwardPathV1::Flat, pool[i].view()).expect("pack");
            }
            let outputs = forward_batch_v1(&device, &model_refs, &batch, &mut buffers).expect("forward");
            for (&i, output) in picks.iter().zip(&outputs) {
                assert_eq!(bits(&output.logits), bits(&alone[i].logits), "size {size} decision {i}");
                assert_eq!(output.value.to_bits(), alone[i].value.to_bits(), "size {size} decision {i}");
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
    let hosts: Arc<Vec<Arc<Net8HostModelV1>>> =
        Arc::new(nets.iter().map(|n| Arc::new(Net8HostModelV1::from_net_v1(n).expect("host"))).collect());
    for latency in [0u64, 200, 5_000] {
        let service = InferenceServiceV1::start_v1(
            ordinal(),
            BatchPolicyV1 { max_latency: Duration::from_micros(latency), ..BatchPolicyV1::default() },
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
                            .forward_v1(&hosts[slot], &nets[slot], ForwardPathV1::Flat, decision.view())
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
    for &size in &[1usize, 10, 20, 64, 256, 512] {
        let mut samples = Vec::new();
        for trial in 0..60 {
            let t = std::time::Instant::now();
            batch.clear();
            for decision in pool.iter().cycle().skip(trial * 7).take(size) {
                batch.push_v1(0, &net, ForwardPathV1::Flat, decision.view()).expect("pack");
            }
            forward_batch_v1(&device, &[&model], &batch, &mut buffers).expect("forward");
            if trial >= 10 {
                samples.push(t.elapsed().as_secs_f64() * 1e6);
            }
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = samples[samples.len() / 2];
        println!(
            "{{\"probe\":\"net8_gpu_forward\",\"batch\":{size},\"p50_us\":{p50:.1},\"us_per_decision\":{:.2}}}",
            p50 / size as f64
        );
    }
}

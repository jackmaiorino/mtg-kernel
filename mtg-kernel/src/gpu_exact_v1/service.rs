//! The batched inference service: one thread owns the device and the
//! resident models; collector threads hold clients and block on replies.
//!
//! A client validates and packs its own request (in parallel with the other
//! collectors), sends it, and waits. The service gathers requests until every
//! live client is waiting, the batch is full, or the latency window since the
//! oldest request closes, then runs one launch per model and replies. Replies
//! are exact (`net8_forward.cu`), so batching changes only timing, never a
//! bit. Sampling and its randomness stay in the collector.

use super::net8::{
    forward_batch_v1, DeviceBuffersV1, ExactOutputV1, Net8DeviceModelV1, Net8HostModelV1,
    PackedBatchV1,
};
use super::ExactDeviceV1;
use crate::inference_cache_v1::ForwardPathV1;
use crate::native_policy_value_net_v1::{NativeEncodedDecisionViewV1, NativePolicyValueNetV1};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How the service forms batches. Timing only: no setting changes a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BatchPolicyV1 {
    pub(crate) max_requests: usize,
    pub(crate) max_latency: Duration,
    /// Resident models before the least recently used one is dropped.
    pub(crate) max_models: usize,
}

impl Default for BatchPolicyV1 {
    fn default() -> Self {
        Self { max_requests: 256, max_latency: Duration::from_micros(200), max_models: 10 }
    }
}

/// Counters for receipts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ServiceStatsV1 {
    pub(crate) batches: u64,
    pub(crate) requests: u64,
    pub(crate) launches: u64,
    pub(crate) model_uploads: u64,
    pub(crate) busy_micros: u64,
}

type Reply = Result<ExactOutputV1, String>;

struct Request {
    model: Arc<Net8HostModelV1>,
    packed: PackedBatchV1,
    reply: SyncSender<Reply>,
}

enum Message {
    Request(Request),
    Stats(SyncSender<ServiceStatsV1>),
    /// Test only: panic while holding the next request, as a device fault
    /// would end the thread.
    #[cfg(test)]
    PanicOnNextRequest,
}

struct Shared {
    /// Clients alive; when this many requests are pending, nothing else can
    /// arrive before a reply, so the batch flushes at once.
    clients: AtomicUsize,
}

/// Owns the service thread. Dropping it (after every client) stops the
/// thread.
pub(crate) struct InferenceServiceV1 {
    pub(crate) identity: super::DeviceIdentityV1,
    sender: Option<Sender<Message>>,
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// A collector's handle. Cloning registers another client.
pub(crate) struct InferenceClientV1 {
    sender: Sender<Message>,
    shared: Arc<Shared>,
}

impl InferenceServiceV1 {
    /// Opens device `ordinal` on a new thread (which owns the CUDA context)
    /// and returns once the device is ready (kernels loaded, tanh proved).
    pub(crate) fn start_v1(ordinal: usize, policy: BatchPolicyV1) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let shared = Arc::new(Shared { clients: AtomicUsize::new(0) });
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name(format!("gpu-exact-inference-{ordinal}"))
            .spawn(move || match ExactDeviceV1::open_v1(ordinal) {
                Ok(device) => {
                    let _ = ready_sender.send(Ok(device.identity.clone()));
                    serve_v1(device, policy, receiver, thread_shared);
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(error));
                }
            })
            .map_err(|e| format!("spawn inference service: {e}"))?;
        let identity = ready_receiver
            .recv()
            .map_err(|_| "inference service exited during startup".to_owned())??;
        Ok(Self { identity, sender: Some(sender), shared, thread: Some(thread) })
    }

    pub(crate) fn client_v1(&self) -> InferenceClientV1 {
        InferenceClientV1::new(self.sender.clone().expect("service running"), Arc::clone(&self.shared))
    }

    #[cfg(test)]
    pub(crate) fn panic_on_next_request_for_test_v1(&self) {
        let _ = self.sender.as_ref().expect("service running").send(Message::PanicOnNextRequest);
    }

    pub(crate) fn stats_v1(&self) -> Result<ServiceStatsV1, String> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .as_ref()
            .expect("service running")
            .send(Message::Stats(reply))
            .map_err(|_| "inference service stopped".to_owned())?;
        receiver.recv().map_err(|_| "inference service stopped".to_owned())
    }
}

impl Drop for InferenceServiceV1 {
    fn drop(&mut self) {
        // The thread ends when every sender (ours and the clients') is gone.
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl InferenceClientV1 {
    fn new(sender: Sender<Message>, shared: Arc<Shared>) -> Self {
        shared.clients.fetch_add(1, Ordering::SeqCst);
        Self { sender, shared }
    }

    /// The exact forward of `view` under `model`, from the service.
    pub(crate) fn forward_v1(
        &mut self,
        model: &Arc<Net8HostModelV1>,
        net: &NativePolicyValueNetV1,
        path: ForwardPathV1,
        view: NativeEncodedDecisionViewV1<'_>,
    ) -> Result<ExactOutputV1, String> {
        let mut packed = PackedBatchV1::default();
        packed.push_v1(0, net, path, view)?;
        // A channel per request: its only sender travels with the request,
        // so a service that ends with it pending (a panic, a device fault)
        // drops the sender and this wait fails at once instead of hanging.
        let (reply, receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Message::Request(Request { model: Arc::clone(model), packed, reply }))
            .map_err(|_| "inference service stopped".to_owned())?;
        receiver.recv().map_err(|_| "inference service stopped".to_owned())?
    }
}

impl Clone for InferenceClientV1 {
    fn clone(&self) -> Self {
        Self::new(self.sender.clone(), Arc::clone(&self.shared))
    }
}

impl Drop for InferenceClientV1 {
    fn drop(&mut self) {
        self.shared.clients.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Resident {
    model: Net8DeviceModelV1,
    last_used: u64,
}

fn serve_v1(device: ExactDeviceV1, policy: BatchPolicyV1, receiver: Receiver<Message>, shared: Arc<Shared>) {
    let mut resident: HashMap<[u8; 32], Resident> = HashMap::new();
    let mut buffers = DeviceBuffersV1::default();
    let mut batch = PackedBatchV1::default();
    let mut stats = ServiceStatsV1::default();
    let mut tick = 0u64;
    let mut pending: Vec<Request> = Vec::new();
    #[cfg(test)]
    let mut armed = false;
    // Blocks for the first message; returns when every sender is gone.
    while let Ok(first) = receiver.recv() {
        // Mutable only for the test-only fault arm.
        #[cfg_attr(not(test), allow(unused_mut))]
        let mut take = |message: Message, pending: &mut Vec<Request>| match message {
            Message::Request(request) => {
                pending.push(request);
                #[cfg(test)]
                if armed {
                    panic!("injected service fault with a request pending");
                }
            }
            Message::Stats(reply) => {
                let _ = reply.send(stats);
            }
            #[cfg(test)]
            Message::PanicOnNextRequest => armed = true,
        };
        take(first, &mut pending);
        if pending.is_empty() {
            continue;
        }
        let deadline = Instant::now() + policy.max_latency;
        while pending.len() < policy.max_requests
            && pending.len() < shared.clients.load(Ordering::SeqCst)
        {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match receiver.recv_timeout(deadline - now) {
                Ok(message) => take(message, &mut pending),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => break,
            }
        }
        let started = Instant::now();
        tick += 1;
        let result = run_batch_v1(
            &device,
            &policy,
            &mut resident,
            &mut buffers,
            &mut batch,
            &pending,
            tick,
            &mut stats,
        );
        for (request, reply) in pending.drain(..).zip(result) {
            // The CPU forward refuses a non-finite output the same way.
            let reply = reply.and_then(|output| {
                let finite = output.value.is_finite() && output.logits.iter().all(|x| x.is_finite());
                if finite { Ok(output) } else { Err("non-finite GPU forward output".into()) }
            });
            let _ = request.reply.send(reply);
        }
        stats.batches += 1;
        stats.busy_micros += started.elapsed().as_micros() as u64;
    }
}

/// Every pending request's reply, in order. Requests run in groups of at
/// most `max_models` distinct models (first-use order), so any mix of models
/// is served; a group's device error fails only that group's requests.
/// Grouping changes launches, never a reply's bits.
#[allow(clippy::too_many_arguments)]
fn run_batch_v1(
    device: &ExactDeviceV1,
    policy: &BatchPolicyV1,
    resident: &mut HashMap<[u8; 32], Resident>,
    buffers: &mut DeviceBuffersV1,
    batch: &mut PackedBatchV1,
    pending: &[Request],
    tick: u64,
    stats: &mut ServiceStatsV1,
) -> Vec<Reply> {
    let mut digests: Vec<[u8; 32]> = Vec::new();
    for request in pending {
        if !digests.contains(&request.model.digest) {
            digests.push(request.model.digest);
        }
    }
    let mut replies: Vec<Option<Reply>> = (0..pending.len()).map(|_| None).collect();
    for group in digests.chunks(policy.max_models.max(1)) {
        let members: Vec<usize> =
            (0..pending.len()).filter(|&i| group.contains(&pending[i].model.digest)).collect();
        let result = run_group_v1(
            device, policy, resident, buffers, batch, pending, &members, group, tick, stats,
        );
        match result {
            Ok(outputs) => {
                for (&i, output) in members.iter().zip(outputs) {
                    replies[i] = Some(Ok(output));
                }
            }
            Err(error) => {
                for &i in &members {
                    replies[i] = Some(Err(error.clone()));
                }
            }
        }
    }
    replies.into_iter().map(|reply| reply.expect("every request is in one group")).collect()
}

#[allow(clippy::too_many_arguments)]
fn run_group_v1(
    device: &ExactDeviceV1,
    policy: &BatchPolicyV1,
    resident: &mut HashMap<[u8; 32], Resident>,
    buffers: &mut DeviceBuffersV1,
    batch: &mut PackedBatchV1,
    pending: &[Request],
    members: &[usize],
    group: &[[u8; 32]],
    tick: u64,
    stats: &mut ServiceStatsV1,
) -> Result<Vec<ExactOutputV1>, String> {
    for &i in members {
        let digest = pending[i].model.digest;
        if !resident.contains_key(&digest) {
            while resident.len() >= policy.max_models {
                // Drop the least recently used model this group does not use.
                let victim = resident
                    .iter()
                    .filter(|(d, _)| !group.contains(d))
                    .min_by_key(|(_, r)| r.last_used)
                    .map(|(d, _)| *d)
                    .ok_or("every resident model is in use by this group")?;
                resident.remove(&victim);
            }
            let model = Net8DeviceModelV1::upload_v1(device, &pending[i].model)?;
            stats.model_uploads += 1;
            resident.insert(digest, Resident { model, last_used: tick });
        }
        resident.get_mut(&digest).expect("resident").last_used = tick;
    }
    batch.clear();
    for &i in members {
        let slot = group.iter().position(|d| *d == pending[i].model.digest).expect("slot");
        batch.append_v1(&pending[i].packed, slot)?;
    }
    let models: Vec<&Net8DeviceModelV1> = group.iter().map(|d| &resident[d].model).collect();
    stats.requests += members.len() as u64;
    stats.launches += models.len() as u64;
    forward_batch_v1(device, &models, batch, buffers)
}

/// The process's service, installed once by a run that declares the GPU
/// inference backend. Collector threads enter it with
/// [`enter_collector_v1`].
static GLOBAL: std::sync::OnceLock<InferenceServiceV1> = std::sync::OnceLock::new();

std::thread_local! {
    static COLLECTOR: std::cell::RefCell<Option<InferenceClientV1>> = const { std::cell::RefCell::new(None) };
}

/// Installs the process's service. Fails if one is already installed.
pub(crate) fn install_global_v1(service: InferenceServiceV1) -> Result<(), String> {
    GLOBAL.set(service).map_err(|_| "a GPU inference service is already installed".to_owned())
}

/// The installed service's device and counters, for receipts.
pub(crate) fn global_receipt_v1() -> Option<Result<serde_json::Value, String>> {
    GLOBAL.get().map(|service| {
        let stats = service.stats_v1()?;
        let id = &service.identity;
        Ok(serde_json::json!({
            "schema": "mtg-kernel-gpu-exact-inference-receipt/v1",
            "device": {"ordinal": id.ordinal, "name": id.name,
                "compute_capability": [id.compute_capability.0, id.compute_capability.1],
                "driver_version": id.driver_version,
                "nvrtc_version": [id.nvrtc_version.0, id.nvrtc_version.1],
                "program_sha256": id.program_sha256,
                "host_tanhf_identity": id.host_tanhf_identity, "tanh_proof_key": id.tanh_proof_key},
            "batches": stats.batches, "requests": stats.requests, "launches": stats.launches,
            "model_uploads": stats.model_uploads, "busy_seconds": stats.busy_micros as f64 / 1e6,
            "mean_batch": if stats.batches == 0 { 0.0 } else { stats.requests as f64 / stats.batches as f64 },
        }))
    })
}

/// While alive, this thread's policy forwards go to the installed service
/// (one client per thread, so a game's learner and opponent seats count as
/// one waiting client). Without an installed service it does nothing.
pub(crate) struct CollectorGuardV1(bool);

pub(crate) fn enter_collector_v1() -> CollectorGuardV1 {
    let Some(service) = GLOBAL.get() else {
        return CollectorGuardV1(false);
    };
    COLLECTOR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_some() {
            return CollectorGuardV1(false);
        }
        *slot = Some(service.client_v1());
        CollectorGuardV1(true)
    })
}

impl Drop for CollectorGuardV1 {
    fn drop(&mut self) {
        if self.0 {
            COLLECTOR.with(|slot| slot.borrow_mut().take());
        }
    }
}

/// A policy's cached host copy of its net for the service: built on first
/// use, and cleared wherever the policy's parameters change.
#[derive(Clone, Default)]
pub(crate) struct PolicySlotV1 {
    /// Some(None): this net is not one the exact GPU forward runs.
    host: Option<Option<Arc<Net8HostModelV1>>>,
}

impl PolicySlotV1 {
    pub(crate) fn clear(&mut self) {
        self.host = None;
    }
}

/// The service's reply for this collector thread's forward, or None when
/// the thread has no client or the net or path is not one the exact GPU
/// forward runs (the caller then runs the CPU forward, with the same bits).
pub(crate) fn collector_forward_v1(
    slot: &mut PolicySlotV1,
    net: &NativePolicyValueNetV1,
    path: ForwardPathV1,
    view: NativeEncodedDecisionViewV1<'_>,
) -> Option<Result<ExactOutputV1, String>> {
    if matches!(path, ForwardPathV1::TransferV8) || !view.pairing_node_indices.is_empty() {
        return None;
    }
    // The kernel's tanh is proved equal to the platform tanhf in the default
    // floating-point environment only; a thread in another state (MXCSR
    // control bits, UCRT's FMA3 switch) keeps the CPU forward.
    if !crate::vector_tanh_v1::default_tanhf_environment_v1() {
        return None;
    }
    COLLECTOR.with(|client| {
        let mut client = client.borrow_mut();
        let client = client.as_mut()?;
        let host = slot
            .host
            .get_or_insert_with(|| Net8HostModelV1::from_net_v1(net).ok().map(Arc::new))
            .as_ref()?;
        Some(client.forward_v1(host, net, path, view))
    })
}

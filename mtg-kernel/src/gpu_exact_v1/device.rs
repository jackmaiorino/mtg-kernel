//! One CUDA device with the exact kernels loaded: NVRTC compile with an
//! on-disk cache, and the activation proof.

use cudarc::driver::{CudaContext, CudaFunction, CudaStream, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions, Ptx};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

/// Version of the kernel source and its compile options; part of every
/// cache and proof key.
const PROGRAM_V1: &str = "mtg-kernel-gpu-exact-v1";

const TANH_SOURCE: &str = include_str!("tanh.cuh");
const NET8_SOURCE: &str = include_str!("net8_forward.cu");
const NET8_BACKWARD_SOURCE: &str = include_str!("net8_backward.cu");

/// What a result depends on beyond its inputs: the program and options, the
/// NVRTC that compiled it and the device that runs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceIdentityV1 {
    pub(crate) ordinal: usize,
    pub(crate) name: String,
    pub(crate) compute_capability: (i32, i32),
    pub(crate) driver_version: i32,
    pub(crate) nvrtc_version: (i32, i32),
    /// SHA-256 of the program name, source and options.
    pub(crate) program_sha256: String,
    /// `vector_tanh_v1`'s key over the loaded `tanhf` image and CPU features.
    pub(crate) host_tanhf_identity: String,
    /// The proof record's key: the program, this device and the host tanhf.
    pub(crate) tanh_proof_key: String,
}

pub(crate) struct ExactDeviceV1 {
    pub(crate) identity: DeviceIdentityV1,
    pub(crate) stream: Arc<CudaStream>,
    pub(crate) net8_forward: CudaFunction,
    pub(crate) net8_reverse: CudaFunction,
    pub(crate) net8_param_grad: CudaFunction,
    pub(crate) net8_embedding_grad: CudaFunction,
    tanh_proof: CudaFunction,
    /// Kept alive for the functions above.
    _ctx: Arc<CudaContext>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn source_and_options() -> (String, Vec<String>) {
    let source =
        format!("#define MTG_HIDDEN 64\n{TANH_SOURCE}\n{NET8_SOURCE}\n{NET8_BACKWARD_SOURCE}");
    // --fmad=false: no contraction of a separate multiply and add, so every
    // `a * b + c` the source does not spell as an FMA stays two roundings.
    let options = vec![
        "--fmad=false".to_owned(),
        "--ftz=false".to_owned(),
        "--prec-div=true".to_owned(),
        "--prec-sqrt=true".to_owned(),
    ];
    (source, options)
}

/// Where compiled PTX and proof records live: `MTG_KERNEL_GPU_EXACT_DIR`,
/// else `%LOCALAPPDATA%\mtg-kernel\gpu-exact`.
fn record_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MTG_KERNEL_GPU_EXACT_DIR") {
        return Some(dir.into());
    }
    std::env::var_os("LOCALAPPDATA")
        .map(|base| PathBuf::from(base).join("mtg-kernel").join("gpu-exact"))
}

fn write_atomically(path: &std::path::Path, bytes: &[u8]) {
    // Best effort: an unwritable record only means the next process redoes it.
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    if std::fs::write(&temp, bytes).is_ok() && std::fs::rename(&temp, path).is_err() {
        let _ = std::fs::remove_file(&temp);
    }
}

fn nvrtc_version() -> Result<(i32, i32), String> {
    let (mut major, mut minor) = (0, 0);
    // SAFETY: valid out pointers; NVRTC is loaded dynamically by cudarc.
    let status = unsafe { cudarc::nvrtc::sys::nvrtcVersion(&mut major, &mut minor) };
    if status != cudarc::nvrtc::sys::nvrtcResult::NVRTC_SUCCESS {
        return Err(format!("nvrtcVersion: {status:?}"));
    }
    Ok((major, minor))
}

fn driver_version() -> Result<i32, String> {
    let mut version = 0;
    // SAFETY: a valid out pointer; the driver is initialized by the context.
    let status = unsafe { cudarc::driver::sys::cuDriverGetVersion(&mut version) };
    if status != cudarc::driver::sys::CUresult::CUDA_SUCCESS {
        return Err(format!("cuDriverGetVersion: {status:?}"));
    }
    Ok(version)
}

impl ExactDeviceV1 {
    /// Opens device `ordinal`, loads the program (compiling it once per
    /// program, NVRTC and compute capability) and proves the activation
    /// equal to the host `tanhf` (once per device and host libm).
    pub(crate) fn open_v1(ordinal: usize) -> Result<Self, String> {
        let ctx = CudaContext::new(ordinal).map_err(|e| format!("CUDA device {ordinal}: {e:?}"))?;
        let stream = ctx.default_stream();
        let name = ctx.name().map_err(|e| format!("device name: {e:?}"))?;
        let compute_capability = ctx
            .compute_capability()
            .map_err(|e| format!("compute capability: {e:?}"))?;
        let (source, options) = source_and_options();
        let mut h = Sha256::new();
        h.update(PROGRAM_V1.as_bytes());
        h.update(source.as_bytes());
        for option in &options {
            h.update(option.as_bytes());
        }
        let identity = DeviceIdentityV1 {
            ordinal,
            name,
            compute_capability,
            driver_version: driver_version()?,
            nvrtc_version: nvrtc_version()?,
            program_sha256: hex(&h.finalize()),
            host_tanhf_identity: String::new(),
            tanh_proof_key: String::new(),
        };
        let ptx = load_or_compile_v1(&identity, &source, &options)?;
        let module = ctx
            .load_module(ptx)
            .map_err(|e| format!("load module: {e:?}"))?;
        let load = |name: &str| {
            module
                .load_function(name)
                .map_err(|e| format!("{name}: {e:?}"))
        };
        let device = Self {
            net8_forward: load("mtg_net8_forward")?,
            net8_reverse: load("mtg_net8_reverse")?,
            net8_param_grad: load("mtg_net8_param_grad")?,
            net8_embedding_grad: load("mtg_net8_embedding_grad")?,
            tanh_proof: load("mtg_tanh_proof")?,
            identity,
            stream,
            _ctx: ctx,
        };
        let mut device = device;
        device.identity.host_tanhf_identity =
            crate::vector_tanh_v1::host_tanhf_identity_v1().unwrap_or_default();
        device.identity.tanh_proof_key = device.tanh_proof_key_v1()?;
        device.ensure_tanh_proved_v1()?;
        Ok(device)
    }

    /// Free and total device memory in bytes.
    pub(crate) fn memory_v1(&self) -> Result<(usize, usize), String> {
        self._ctx
            .mem_get_info()
            .map_err(|e| format!("memory query: {e:?}"))
    }

    /// The proof record's key: the program, this device and NVRTC, and the
    /// host `tanhf` (`vector_tanh_v1`'s key over the loaded images).
    fn tanh_proof_key_v1(&self) -> Result<String, String> {
        let host = crate::vector_tanh_v1::host_tanhf_identity_v1().ok_or_else(|| {
            "GPU exact kernels require the Windows UCRT FMA3 tanhf in the default MXCSR state"
                .to_owned()
        })?;
        let id = &self.identity;
        let mut h = Sha256::new();
        h.update(b"tanh-proof-v1");
        h.update(id.program_sha256.as_bytes());
        h.update(id.name.as_bytes());
        h.update(format!(
            "{:?}{:?}{}",
            id.compute_capability, id.nvrtc_version, id.driver_version
        ));
        h.update(host.as_bytes());
        Ok(hex(&h.finalize()))
    }

    fn ensure_tanh_proved_v1(&self) -> Result<(), String> {
        let key = self.identity.tanh_proof_key.clone();
        let record = record_dir().map(|dir| dir.join("tanh-proof").join(format!("{key}.proved")));
        if let Some(path) = &record {
            if std::fs::read_to_string(path).is_ok_and(|s| s.trim() == key) {
                return Ok(());
            }
        }
        if let Some(first) = self.tanh_mismatch_v1()? {
            return Err(format!(
                "GPU tanh differs from the host tanhf at input bits {first:#010x}"
            ));
        }
        if let Some(path) = record {
            write_atomically(&path, key.as_bytes());
        }
        Ok(())
    }

    /// Evaluates the kernel's tanh on all 2^32 inputs and compares each with
    /// the host's (`vector_tanh_v1::libm_tanh_in_place_v1`, the production
    /// activation). NaN inputs are skipped: every caller fails closed on a
    /// non-finite output. Returns the first mismatching input, if any.
    pub(crate) fn tanh_mismatch_v1(&self) -> Result<Option<u32>, String> {
        const CHUNK: u32 = 1 << 24;
        let mut device_out = self
            .stream
            .alloc_zeros::<u32>(CHUNK as usize)
            .map_err(|e| format!("alloc: {e:?}"))?;
        let mut gpu = vec![0u32; CHUNK as usize];
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let mut first = 0u32;
        loop {
            let mut launch = self.stream.launch_builder(&self.tanh_proof);
            launch.arg(&first).arg(&CHUNK).arg(&mut device_out);
            let config = LaunchConfig {
                grid_dim: (1024, 1, 1),
                block_dim: (256, 1, 1),
                shared_mem_bytes: 0,
            };
            // SAFETY: the kernel writes CHUNK u32 values into a CHUNK buffer.
            unsafe { launch.launch(config) }.map_err(|e| format!("tanh proof launch: {e:?}"))?;
            self.stream
                .memcpy_dtoh(&device_out, &mut gpu)
                .map_err(|e| format!("tanh proof download: {e:?}"))?;
            let span = (CHUNK as usize).div_ceil(threads);
            let mismatch = std::thread::scope(|s| {
                let workers: Vec<_> = gpu
                    .chunks(span)
                    .enumerate()
                    .map(|(t, part)| {
                        s.spawn(move || {
                            let base = first + (t * span) as u32;
                            let mut host: Vec<f32> = (0..part.len())
                                .map(|i| f32::from_bits(base + i as u32))
                                .collect();
                            crate::vector_tanh_v1::libm_tanh_in_place_v1(&mut host);
                            part.iter()
                                .zip(&host)
                                .enumerate()
                                .find_map(|(i, (&g, &c))| {
                                    let input = f32::from_bits(base + i as u32);
                                    (!input.is_nan() && g != c.to_bits()).then_some(base + i as u32)
                                })
                        })
                    })
                    .collect();
                workers
                    .into_iter()
                    .filter_map(|w| w.join().expect("proof worker"))
                    .min()
            });
            if mismatch.is_some() {
                return Ok(mismatch);
            }
            match first.checked_add(CHUNK) {
                Some(next) => first = next,
                None => return Ok(None),
            }
        }
    }
}

fn load_or_compile_v1(
    identity: &DeviceIdentityV1,
    source: &str,
    options: &[String],
) -> Result<Ptx, String> {
    let (major, minor) = identity.compute_capability;
    let arch: &'static str = Box::leak(format!("compute_{major}{minor}").into_boxed_str());
    let mut h = Sha256::new();
    h.update(identity.program_sha256.as_bytes());
    h.update(format!("{:?}{arch}", identity.nvrtc_version));
    let key = hex(&h.finalize());
    let path = record_dir().map(|dir| dir.join("ptx").join(format!("{key}.ptx")));
    if let Some(path) = &path {
        if let Ok(text) = std::fs::read_to_string(path) {
            return Ok(Ptx::from_src(text));
        }
    }
    let ptx = compile_ptx_with_opts(
        source,
        CompileOptions {
            arch: Some(arch),
            options: options.to_vec(),
            name: Some("mtg_gpu_exact_v1.cu".into()),
            ..Default::default()
        },
    )
    .map_err(|e| format!("NVRTC: {e:?}"))?;
    if let Some(path) = &path {
        write_atomically(path, ptx.to_src().as_bytes());
    }
    Ok(ptx)
}

//! Microbenchmarks for the batched GPU inference design note
//! (`docs/design/gpu_batched_inference_v1.md`). Measurement only.
//!
//! 1. Round trip: upload a request batch, one kernel, download results, sync.
//! 2. Launch chain: N dependent small kernels, then one sync.
//! 3. Exact-order linear layer: every output accumulates its K products in
//!    ascending K with IEEE `__fmul_rn`/`__fadd_rn` (no contraction), so its
//!    bits do not depend on the batch and match a sequential CPU loop.
//!    Naive (thread per output) and register-tiled (64x64 tile, 4x4 per thread).
//! 4. cuBLAS SGEMM on the same shapes (not batch invariant; speed reference).
//!
//! Run: cargo run --release --example gpu_infer_microbench_v1
//!      --features gpu-exact-v1 -- <device ordinal>

use cudarc::cublas::{sys as blas_sys, CudaBlas, Gemm, GemmConfig};
use cudarc::driver::{CudaContext, CudaSlice, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::{compile_ptx_with_opts, CompileOptions};
use std::error::Error;
use std::time::Instant;

const SOURCE: &str = r#"
extern "C" __global__ void noop(float* out) {
    if (threadIdx.x == 0 && blockIdx.x == 0) { out[0] = out[0] + 1.0f; }
}

extern "C" __global__ void linear_naive(const float* x, const float* w, float* y,
                                        int rows, int k, int n) {
    int col = blockIdx.x * blockDim.x + threadIdx.x;
    int row = blockIdx.y;
    if (col >= n || row >= rows) return;
    float acc = 0.0f;
    const float* xr = x + (size_t)row * k;
    for (int i = 0; i < k; ++i) {
        acc = __fadd_rn(acc, __fmul_rn(xr[i], w[(size_t)i * n + col]));
    }
    y[(size_t)row * n + col] = acc;
}

// 64x64 output tile per 256-thread block, 4x4 outputs per thread, K tiles of
// 16 staged in shared memory. Each output still adds its products in
// ascending K, one at a time.
extern "C" __global__ void linear_tiled(const float* x, const float* w, float* y,
                                        int rows, int k, int n) {
    __shared__ float xs[16][64];
    __shared__ float ws[16][64];
    int tx = threadIdx.x % 16;
    int ty = threadIdx.x / 16;
    int row0 = blockIdx.y * 64;
    int col0 = blockIdx.x * 64;
    float acc[4][4];
    for (int a = 0; a < 4; ++a) for (int b = 0; b < 4; ++b) acc[a][b] = 0.0f;
    for (int k0 = 0; k0 < k; k0 += 16) {
        for (int t = threadIdx.x; t < 16 * 64; t += 256) {
            int kk = t / 64, rr = t % 64;
            int gr = row0 + rr, gk = k0 + kk;
            xs[kk][rr] = (gr < rows && gk < k) ? x[(size_t)gr * k + gk] : 0.0f;
            int gc = col0 + rr;
            ws[kk][rr] = (gk < k && gc < n) ? w[(size_t)gk * n + gc] : 0.0f;
        }
        __syncthreads();
        int kmax = min(16, k - k0);
        for (int kk = 0; kk < kmax; ++kk) {
            float xv[4], wv[4];
            for (int a = 0; a < 4; ++a) xv[a] = xs[kk][ty * 4 + a];
            for (int b = 0; b < 4; ++b) wv[b] = ws[kk][tx * 4 + b];
            for (int a = 0; a < 4; ++a)
                for (int b = 0; b < 4; ++b)
                    acc[a][b] = __fadd_rn(acc[a][b], __fmul_rn(xv[a], wv[b]));
        }
        __syncthreads();
    }
    for (int a = 0; a < 4; ++a) {
        int gr = row0 + ty * 4 + a;
        if (gr >= rows) continue;
        for (int b = 0; b < 4; ++b) {
            int gc = col0 + tx * 4 + b;
            if (gc < n) y[(size_t)gr * n + gc] = acc[a][b];
        }
    }
}
"#;

fn percentile(samples: &mut [f64], p: f64) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[((samples.len() - 1) as f64 * p).round() as usize]
}

fn main() -> Result<(), Box<dyn Error>> {
    let ordinal: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(1);
    let ctx = CudaContext::new(ordinal)?;
    let stream = ctx.default_stream();
    let started = Instant::now();
    let ptx = compile_ptx_with_opts(
        SOURCE,
        CompileOptions {
            fmad: Some(false),
            ftz: Some(false),
            prec_div: Some(true),
            prec_sqrt: Some(true),
            use_fast_math: Some(false),
            ..Default::default()
        },
    )?;
    let module = ctx.load_module(ptx)?;
    let compile_ms = started.elapsed().as_secs_f64() * 1e3;
    let noop = module.load_function("noop")?;
    let naive = module.load_function("linear_naive")?;
    let tiled = module.load_function("linear_tiled")?;
    println!("{{\"probe\":\"compile\",\"device\":{ordinal},\"nvrtc_and_load_ms\":{compile_ms:.1}}}");

    // 1. Round trip at several upload sizes (bytes per batch).
    for &upload_floats in &[1_024usize, 16_384, 262_144] {
        let host_in = vec![0.5f32; upload_floats];
        let mut host_out = vec![0f32; 4_096];
        let mut dev_in: CudaSlice<f32> = stream.alloc_zeros(upload_floats)?;
        let dev_out: CudaSlice<f32> = stream.alloc_zeros(4_096)?;
        let mut samples = Vec::new();
        for iteration in 0..600 {
            let t = Instant::now();
            stream.memcpy_htod(&host_in, &mut dev_in)?;
            let mut launch = stream.launch_builder(&noop);
            launch.arg(&mut dev_in);
            unsafe { launch.launch(LaunchConfig::for_num_elems(1))? };
            stream.memcpy_dtoh(&dev_out, &mut host_out)?;
            stream.synchronize()?;
            if iteration >= 100 {
                samples.push(t.elapsed().as_secs_f64() * 1e6);
            }
        }
        let p50 = percentile(&mut samples, 0.5);
        let p99 = percentile(&mut samples, 0.99);
        println!(
            "{{\"probe\":\"roundtrip\",\"upload_bytes\":{},\"p50_us\":{p50:.1},\"p99_us\":{p99:.1}}}",
            upload_floats * 4
        );
    }

    // 2. Launch chain: 24 dependent tiny kernels then one sync.
    {
        let mut dev: CudaSlice<f32> = stream.alloc_zeros(1)?;
        let mut samples = Vec::new();
        for iteration in 0..400 {
            let t = Instant::now();
            for _ in 0..24 {
                let mut launch = stream.launch_builder(&noop);
                launch.arg(&mut dev);
                unsafe { launch.launch(LaunchConfig::for_num_elems(1))? };
            }
            stream.synchronize()?;
            if iteration >= 50 {
                samples.push(t.elapsed().as_secs_f64() * 1e6 / 24.0);
            }
        }
        let p50 = percentile(&mut samples, 0.5);
        println!("{{\"probe\":\"launch_chain\",\"kernels\":24,\"per_kernel_p50_us\":{p50:.2}}}");
    }

    // 3 and 4. Linear layers.
    let blas = CudaBlas::new(stream.clone())?;
    for &(rows, width) in &[
        (64usize, 64usize),
        (1_024, 64),
        (16_384, 64),
        (64, 512),
        (1_024, 512),
        (16_384, 512),
        (64, 1_024),
        (4_096, 1_024),
        (16_384, 1_024),
    ] {
        let k = width;
        let n = width;
        let x_host: Vec<f32> = (0..rows * k).map(|i| ((i * 2_654_435_761usize) % 1_000) as f32 * 1e-3 - 0.5).collect();
        let w_host: Vec<f32> = (0..k * n).map(|i| ((i * 40_503usize) % 997) as f32 * 1e-3 - 0.5).collect();
        let x = stream.clone_htod(&x_host)?;
        let w = stream.clone_htod(&w_host)?;
        let mut y_naive: CudaSlice<f32> = stream.alloc_zeros(rows * n)?;
        let mut y_tiled: CudaSlice<f32> = stream.alloc_zeros(rows * n)?;
        let mut y_blas: CudaSlice<f32> = stream.alloc_zeros(rows * n)?;
        let (rows_i, k_i, n_i) = (rows as i32, k as i32, n as i32);
        let macs = (rows * k * n) as f64;
        let repeats = ((2e9 / macs) as usize).clamp(3, 2_000);

        let time_it = |label: &str, run: &mut dyn FnMut() -> Result<(), Box<dyn Error>>| -> Result<f64, Box<dyn Error>> {
            run()?;
            stream.synchronize()?;
            let t = Instant::now();
            for _ in 0..repeats {
                run()?;
            }
            stream.synchronize()?;
            let seconds = t.elapsed().as_secs_f64() / repeats as f64;
            println!(
                "{{\"probe\":\"linear\",\"kernel\":\"{label}\",\"rows\":{rows},\"k\":{k},\"n\":{n},\"us\":{:.2},\"gmac_per_s\":{:.1}}}",
                seconds * 1e6,
                macs / seconds / 1e9
            );
            Ok(seconds)
        };
        time_it("exact_naive", &mut || {
            let mut launch = stream.launch_builder(&naive);
            launch.arg(&x).arg(&w).arg(&mut y_naive).arg(&rows_i).arg(&k_i).arg(&n_i);
            let config = LaunchConfig {
                grid_dim: (n.div_ceil(128) as u32, rows as u32, 1),
                block_dim: (128, 1, 1),
                shared_mem_bytes: 0,
            };
            unsafe { launch.launch(config)? };
            Ok(())
        })?;
        time_it("exact_tiled", &mut || {
            let mut launch = stream.launch_builder(&tiled);
            launch.arg(&x).arg(&w).arg(&mut y_tiled).arg(&rows_i).arg(&k_i).arg(&n_i);
            let config = LaunchConfig {
                grid_dim: (n.div_ceil(64) as u32, rows.div_ceil(64) as u32, 1),
                block_dim: (256, 1, 1),
                shared_mem_bytes: 0,
            };
            unsafe { launch.launch(config)? };
            Ok(())
        })?;
        time_it("cublas_sgemm", &mut || {
            unsafe {
                blas.gemm(
                    GemmConfig {
                        transa: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                        transb: blas_sys::cublasOperation_t::CUBLAS_OP_N,
                        m: n_i,
                        n: rows_i,
                        k: k_i,
                        alpha: 1.0f32,
                        lda: n_i,
                        ldb: k_i,
                        beta: 0.0f32,
                        ldc: n_i,
                    },
                    &w,
                    &x,
                    &mut y_blas,
                )?;
            }
            Ok(())
        })?;
        let a = stream.clone_dtoh(&y_naive)?;
        let b = stream.clone_dtoh(&y_tiled)?;
        // CPU reference for the first and last rows, same order, no FMA.
        let mut mismatches = 0usize;
        for &row in &[0usize, rows - 1] {
            for col in 0..n {
                let mut acc = 0f32;
                for i in 0..k {
                    acc += x_host[row * k + i] * w_host[i * n + col];
                }
                if acc.to_bits() != a[row * n + col].to_bits() || acc.to_bits() != b[row * n + col].to_bits() {
                    mismatches += 1;
                }
            }
        }
        let naive_vs_tiled = a.iter().zip(&b).filter(|(p, q)| p.to_bits() != q.to_bits()).count();
        println!(
            "{{\"probe\":\"linear_bits\",\"rows\":{rows},\"k\":{k},\"cpu_mismatches_checked_rows\":{mismatches},\"naive_vs_tiled_mismatches\":{naive_vs_tiled}}}"
        );
    }
    Ok(())
}

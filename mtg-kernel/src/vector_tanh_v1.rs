//! The platform `tanhf`, eight values at a time, with the same bits.
//!
//! `f32::tanh` calls the C runtime's `tanhf`. On Windows x86_64 that is
//! UCRT's, which on CPUs with FMA3 evaluates AMD's libm algorithm
//! (`libm_sse2/tanhf.c`): the identity below 2^-13, a [2,1] rational on
//! (0, 0.9) and on [0.9, 1], `1 - 2 / (exp(2|x|) + 1)` on (1, 10] (AMD's
//! `splitexpf` with its 32-entry tables), and 1 above, with fused
//! multiply-adds in the rational's polynomials and in `splitexpf`. Every step
//! is an IEEE float operation, so `tanh8` (the same operations on eight
//! lanes, with AVX2 and FMA) gives the same bits lane by lane.
//!
//! That is a claim about one C runtime on one CPU family, so it is proved
//! before use: the first time a process asks, `tanh8` is compared with the
//! loaded `tanhf` on every one of the 2^32 f32 inputs (about 40 CPU seconds),
//! and a passed comparison is recorded under a key binding the running
//! executable and the loaded `ucrtbase.dll` (SHA-256 of each loaded image's
//! read-only sections, `image_digest`) and the CPU features `tanhf` could
//! dispatch on. Later processes of the same executable on the same machine
//! read the record. The proof runs with
//! the default MXCSR (every exception masked, no flush-to-zero, no
//! denormals-are-zero, round to nearest) and UCRT's FMA3 paths on, so only a
//! caller in that state takes `tanh8`. Anything else (another OS or
//! architecture, no AVX2 or FMA, a `tanhf` not from `ucrtbase.dll`, any
//! mismatch, a failed check, another MXCSR state, FMA3 paths turned off, or
//! `MTG_KERNEL_VECTOR_TANH=0`) keeps calling `tanhf` per value. NaN inputs
//! always take `tanhf`, so NaN payloads are untouched. The returned bits are
//! the same; MXCSR status flags need not be (lanes a branch discards can
//! raise the sticky inexact flag where `tanhf` does not).
//!
//! On Linux x86_64 with glibc, `tanhf` is fdlibm's in the releases this was
//! written against (`sysdeps/ieee754/flt-32/s_tanhf.c` over `s_expm1f.c`):
//! `x` itself below 2^-55, `-t / (t + 2)` with `t = expm1f(-2|x|)` below 1,
//! `1 - 2 / (t + 2)` with `t = expm1f(2|x|)` below 22, and 1 above, all in
//! single-precision operations with no fused multiply-add. `glibc::tanh8`
//! performs the same operations on eight lanes with AVX2 and is proved the
//! same way, against the loaded `tanhf` on every input, in the default MXCSR
//! state. Symbols can be interposed on Linux, so its record's key binds
//! every image loaded in the process (SHA-256 of each one's read-only
//! segments as mapped, which hold no load address), the CPU features and
//! `GLIBC_TUNABLES`. A `tanhf` that differs in any bit (newer glibc releases
//! compute `tanhf` another way) is recorded as differing, so later processes
//! do not repeat the comparison, and `tanhf` is called per value as before.
//! A process with nowhere to keep a record (no `MTG_KERNEL_VECTOR_TANH_DIR`,
//! `XDG_CACHE_HOME` or `HOME`, as under a launcher with a fixed environment,
//! or a folder it cannot write) makes no comparison and calls `tanhf`:
//! otherwise every such process would pay for the comparison again. Builds
//! with debug assertions (unoptimized ones) always call `tanhf`: the
//! comparison would take them minutes for every new binary.

/// A token for the state the platform `tanhf` (and so every activation)
/// depends on beyond its input: UCRT's FMA3 switch on Windows x86_64. Equal
/// tokens mean equal `tanhf` results for equal inputs.
pub(crate) fn libm_environment_token_v1() -> u32 {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        u32::from(windows::fma3_enabled())
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        0
    }
}

/// Settles now, once per process, whether `tanh8` may be used: reads the
/// proof record or runs the proof. Without it the first activation does
/// this, so a caller that times its searches (Stage 4a's seat clock) calls
/// it at startup to keep the proof's 40 CPU seconds out of every search.
/// Results are the same bits either way; only when the cost lands changes.
pub(crate) fn prepare_v1() {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if windows::default_mxcsr() && windows::fma3_enabled() {
        let _ = windows::proved();
    }
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    if glibc::default_mxcsr() {
        let _ = glibc::proved();
    }
}

/// The identity of the `tanhf` every activation computes with, for proofs
/// that compare another implementation with it (`gpu_exact_v1`): the proof
/// record key above, when this process is in the state `tanh8` is proved
/// for. None elsewhere (another OS, MXCSR state or FMA3 setting).
#[cfg_attr(not(feature = "gpu-exact-v1"), allow(dead_code))]
pub(crate) fn host_tanhf_identity_v1() -> Option<String> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if windows::default_mxcsr() && windows::fma3_enabled() {
        return windows::key();
    }
    None
}

/// Whether this thread computes `tanhf` in the state the identity above
/// covers: the default MXCSR and UCRT's FMA3 paths on. Checked per call by
/// callers that rely on a proof made on another thread (`gpu_exact_v1`).
#[cfg_attr(not(feature = "gpu-exact-v1"), allow(dead_code))]
pub(crate) fn default_tanhf_environment_v1() -> bool {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::default_mxcsr() && windows::fma3_enabled()
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        false
    }
}

/// `*v = v.tanh()` for every value, with the same bits.
pub(crate) fn libm_tanh_in_place_v1(values: &mut [f32]) {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if windows::default_mxcsr() && windows::fma3_enabled() && windows::proved() {
        // SAFETY: `proved` holds only where AVX2 and FMA are present.
        unsafe { windows::tanh_in_place(values) };
        return;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    if glibc::default_mxcsr() && glibc::proved() {
        // SAFETY: `proved` holds only where AVX2 is present.
        unsafe { glibc::tanh_in_place(values) };
        return;
    }
    for value in values {
        *value = value.tanh();
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows {
    use sha2::{Digest, Sha256};
    use std::arch::x86_64::*;
    use std::sync::OnceLock;

    /// Version of the algorithm below; part of the proof record's key.
    const ALGORITHM: &str = "mtg-kernel-vector-tanh-v1:amd-libm-tanhf-fma3";

    /// `2^(j/32)` split as a 10-bit lead and a trailing part (AMD's tables).
    static LEAD: [u32; 32] = [
        0x3F800000, 0x3F82C000, 0x3F858000, 0x3F888000, 0x3F8B8000, 0x3F8E8000, 0x3F91C000,
        0x3F94C000, 0x3F980000, 0x3F9B8000, 0x3F9EC000, 0x3FA24000, 0x3FA5C000, 0x3FA98000,
        0x3FAD4000, 0x3FB10000, 0x3FB50000, 0x3FB8C000, 0x3FBD0000, 0x3FC10000, 0x3FC54000,
        0x3FC98000, 0x3FCE0000, 0x3FD28000, 0x3FD74000, 0x3FDBC000, 0x3FE0C000, 0x3FE58000,
        0x3FEAC000, 0x3FEFC000, 0x3FF50000, 0x3FFA8000,
    ];
    static TRAIL: [u32; 32] = [
        0x00000000, 0x39D86988, 0x3AAB0D9F, 0x3A407404, 0x3A2E0F1E, 0x3A90E62D, 0x38F4DCE0,
        0x3AD3BEA3, 0x3ADFC146, 0x39D39B9C, 0x3AD4C982, 0x3AC10C0C, 0x3AFB5AA6, 0x3A856AD3,
        0x3A41F752, 0x3A8FD607, 0x391E6678, 0x3AEEBD1D, 0x398A39F4, 0x3AB13329, 0x3A9CA845,
        0x3AE6F619, 0x3A923054, 0x3AA07647, 0x391F9958, 0x3AEEDE5F, 0x39CDEEC0, 0x3AE41B9D,
        0x37C6E7C0, 0x3A92E66F, 0x3A95F454, 0x38ECB6D0,
    ];

    /// The algorithm on eight lanes. NaN lanes give an unspecified value
    /// (callers replace them).
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn tanh8(x: __m256) -> __m256 {
        let sign_mask = _mm256_castsi256_ps(_mm256_set1_epi32(i32::MIN));
        let y = _mm256_andnot_ps(sign_mask, x);
        let sign = _mm256_and_ps(sign_mask, x);
        let tiny = _mm256_cmpgt_epi32(_mm256_set1_epi32(0x3900_0000), _mm256_castps_si256(y));
        let big = _mm256_cmp_ps::<_CMP_GT_OQ>(y, _mm256_set1_ps(10.0));
        let at_most_one = _mm256_cmp_ps::<_CMP_LE_OQ>(y, _mm256_set1_ps(1.0));
        let below = _mm256_cmp_ps::<_CMP_LT_OQ>(y, _mm256_set1_ps(0.9));
        // Each branch reads y clamped to its own range: lanes that branch
        // returns are unchanged, and the others (discarded) never reach an
        // infinity, an invalid operation or an overflow, so no exception
        // the scalar path would not raise.
        let one = _mm256_set1_ps(1.0);
        let yr = _mm256_min_ps(
            _mm256_max_ps(y, _mm256_castsi256_ps(_mm256_set1_epi32(0x3900_0000))),
            one,
        );
        let ye = _mm256_min_ps(_mm256_max_ps(y, one), _mm256_set1_ps(10.0));
        // A branch no lane takes is skipped (its lanes would be discarded).
        let all = _mm256_castsi256_ps(_mm256_set1_epi32(-1));
        let rational_lanes = _mm256_andnot_ps(_mm256_castsi256_ps(tiny), at_most_one);
        let exp_lanes = _mm256_andnot_ps(_mm256_or_ps(at_most_one, big), all);
        let rational = if _mm256_movemask_ps(rational_lanes) == 0 {
            _mm256_setzero_ps()
        } else {
            // y + y*y2*n/d, coefficients for [0, 0.9) or [0.9, 1] per lane.
            let pick =
                |a: f32, b: f32| _mm256_blendv_ps(_mm256_set1_ps(b), _mm256_set1_ps(a), below);
            let c0 = pick(-0.281_928_06, -0.240_698_59);
            let c1 = pick(-0.001_462_835_6, -0.001_232_564_4);
            let c2 = pick(4.891_631e-5, 3.827_535e-5);
            let d0 = pick(0.845_784_2, 0.722_097_4);
            let d1 = pick(0.342_701_8, 0.292_529_07);
            let y2 = _mm256_mul_ps(yr, yr);
            let n = _mm256_fmadd_ps(_mm256_fmadd_ps(c2, y2, c1), y2, c0);
            let d = _mm256_fmadd_ps(d1, y2, d0);
            _mm256_add_ps(
                yr,
                _mm256_div_ps(_mm256_mul_ps(_mm256_mul_ps(yr, y2), n), d),
            )
        };
        let exp_branch = if _mm256_movemask_ps(exp_lanes) == 0 {
            _mm256_setzero_ps()
        } else {
            // 1 - 2/(exp(2y) + 1), exp as 2^m * (z1 + z2) (`splitexpf`).
            let t = _mm256_mul_ps(_mm256_set1_ps(2.0), ye);
            let r = _mm256_mul_ps(t, _mm256_set1_ps(46.166_24));
            // r > 0 on this branch, so (int)(r + 0.5) truncates the rounded sum.
            let ni = _mm256_cvttps_epi32(_mm256_add_ps(r, _mm256_set1_ps(0.5)));
            let nf = _mm256_cvtepi32_ps(ni);
            let trail = _mm256_set1_ps(9.983_182e-7);
            let r1 = _mm256_sub_ps(t, _mm256_mul_ps(nf, _mm256_set1_ps(0.021_659_851)));
            let r2 = _mm256_sub_ps(_mm256_setzero_ps(), _mm256_mul_ps(nf, trail));
            let j = _mm256_and_si256(ni, _mm256_set1_epi32(0x1f));
            let f1 = _mm256_i32gather_ps::<4>(LEAD.as_ptr().cast(), j);
            let f2 = _mm256_i32gather_ps::<4>(TRAIL.as_ptr().cast(), j);
            let m = _mm256_srai_epi32::<5>(_mm256_sub_epi32(ni, j));
            let rr = _mm256_fnmadd_ps(nf, trail, r1);
            let poly = _mm256_add_ps(
                _mm256_set1_ps(0.5),
                _mm256_mul_ps(rr, _mm256_set1_ps(0.166_666_67)),
            );
            let q = _mm256_add_ps(r1, _mm256_fmadd_ps(_mm256_mul_ps(rr, rr), poly, r2));
            let z2 = _mm256_fmadd_ps(_mm256_add_ps(f1, f2), q, f2);
            // `scaleFloat_2`: two exponent halves (m >= 0 here, so m / 2 is a shift).
            let m1 = _mm256_srai_epi32::<1>(m);
            let bias = _mm256_set1_epi32(127);
            let t1 = _mm256_castsi256_ps(_mm256_slli_epi32::<23>(_mm256_add_epi32(m1, bias)));
            let t2 = _mm256_castsi256_ps(_mm256_slli_epi32::<23>(_mm256_add_epi32(
                _mm256_sub_epi32(m, m1),
                bias,
            )));
            let scaled = _mm256_mul_ps(_mm256_mul_ps(_mm256_add_ps(f1, z2), t1), t2);
            let p = _mm256_add_ps(scaled, _mm256_set1_ps(1.0));
            _mm256_sub_ps(_mm256_set1_ps(1.0), _mm256_div_ps(_mm256_set1_ps(2.0), p))
        };
        let z = _mm256_blendv_ps(exp_branch, rational, at_most_one);
        let z = _mm256_blendv_ps(z, _mm256_set1_ps(1.0), big);
        // z >= 0, so multiplying by the sign of x sets the sign bit.
        _mm256_blendv_ps(_mm256_or_ps(z, sign), x, _mm256_castsi256_ps(tiny))
    }

    /// `tanh8` over a slice; NaN inputs take `tanhf`.
    #[target_feature(enable = "avx2,fma")]
    pub(super) unsafe fn tanh_in_place(values: &mut [f32]) {
        let mut chunks = values.chunks_exact_mut(8);
        for chunk in &mut chunks {
            let x = _mm256_loadu_ps(chunk.as_ptr());
            let nan = _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_UNORD_Q>(x, x));
            if nan == 0 {
                _mm256_storeu_ps(chunk.as_mut_ptr(), tanh8(x));
            } else {
                for value in chunk.iter_mut() {
                    *value = value.tanh();
                }
            }
        }
        let rest = chunks.into_remainder();
        if !rest.is_empty() {
            let mut lanes = [0f32; 8];
            lanes[..rest.len()].copy_from_slice(rest);
            tanh_in_place(&mut lanes);
            rest.copy_from_slice(&lanes[..rest.len()]);
        }
    }

    /// Whether the calling thread's MXCSR control bits are the default (a new
    /// thread's): every exception masked, flush-to-zero and denormals-are-
    /// zero off, round to nearest. The proof runs in that state, so it is
    /// the one where `tanh8` is proved; the status flags are ignored.
    pub(super) fn default_mxcsr() -> bool {
        crate::deterministic_math_v1::read_mxcsr_v1() & 0xffc0 == 0x1f80
    }

    /// Whether UCRT's FMA3 code paths are on (`_get_FMA3_enable`). A process
    /// can turn them off (`_set_FMA3_enable(0)`), which changes `tanhf`'s
    /// bits; `tanh8` is the FMA3 path, so it is used only while they are on.
    pub(super) fn fma3_enabled() -> bool {
        static GET: OnceLock<Option<unsafe extern "C" fn() -> i32>> = OnceLock::new();
        let get = GET.get_or_init(|| {
            let (module, _, _) = linked_tanhf()?;
            // SAFETY: `module` is loaded and the name is NUL-terminated.
            let f = unsafe { GetProcAddress(module, c"_get_FMA3_enable".as_ptr().cast()) };
            // SAFETY: UCRT exports `int __cdecl _get_FMA3_enable(void)`.
            (!f.is_null()).then(|| unsafe {
                std::mem::transmute::<*const c_void, unsafe extern "C" fn() -> i32>(f)
            })
        });
        // SAFETY: the export's documented signature, no arguments.
        get.is_some_and(|f| unsafe { f() } != 0)
    }

    /// Whether `tanh8` is proved equal to this process's `tanhf`.
    pub(super) fn proved() -> bool {
        static PROVED: OnceLock<bool> = OnceLock::new();
        *PROVED.get_or_init(|| {
            if std::env::var("MTG_KERNEL_VECTOR_TANH").is_ok_and(|v| v == "0") {
                return false;
            }
            if !(is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")) {
                return false;
            }
            let Some(key) = key() else {
                return false;
            };
            let record = record_dir().map(|dir| dir.join(format!("{key}.proved")));
            if let Some(path) = &record {
                if std::fs::read_to_string(path).is_ok_and(|s| s.trim() == key) {
                    return true;
                }
            }
            // The proof covers the FMA3 path in the default MXCSR state.
            if !(fma3_enabled() && default_mxcsr() && exhaustive_check() && fma3_enabled()) {
                return false;
            }
            if let Some(path) = record {
                // Best effort: an unwritable record only means the next
                // process checks again.
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let temp = path.with_extension(format!("tmp-{}", std::process::id()));
                if std::fs::write(&temp, &key).is_ok() && std::fs::rename(&temp, &path).is_err() {
                    let _ = std::fs::remove_file(&temp);
                }
            }
            true
        })
    }

    /// Where proof records live: `MTG_KERNEL_VECTOR_TANH_DIR`, else
    /// `%LOCALAPPDATA%\mtg-kernel\vector-tanh`, else (an environment
    /// without `LOCALAPPDATA`, such as an allowlisted launcher's) the same
    /// folder under Windows' default `%USERPROFILE%\AppData\Local`.
    fn record_dir() -> Option<std::path::PathBuf> {
        record_dir_from(|name| std::env::var_os(name))
    }

    /// `record_dir` over an environment lookup (testable).
    pub(super) fn record_dir_from(
        var: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Option<std::path::PathBuf> {
        if let Some(dir) = var("MTG_KERNEL_VECTOR_TANH_DIR") {
            return Some(dir.into());
        }
        let local = var("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                var("USERPROFILE")
                    .map(|home| std::path::Path::new(&home).join("AppData").join("Local"))
            });
        local.map(|base| base.join("mtg-kernel").join("vector-tanh"))
    }

    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *const c_void;
    }
    #[link(name = "ucrt")]
    extern "C" {
        fn tanhf(x: f32) -> f32;
    }

    /// `ucrtbase.dll`, its exported `tanhf`, and the function this crate's
    /// `tanhf` calls reach: the address itself or, when that is the linker's
    /// import thunk (`jmp qword ptr [rip + disp32]`, emitted for a call not
    /// marked `dllimport`), the target in the thunk's import slot.
    pub(super) fn linked_tanhf() -> Option<(*mut c_void, *const c_void, *const c_void)> {
        let name: Vec<u16> = "ucrtbase.dll\0".encode_utf16().collect();
        // SAFETY: a NUL-terminated wide string; a null result is handled.
        let module = unsafe { GetModuleHandleW(name.as_ptr()) };
        if module.is_null() {
            return None;
        }
        // SAFETY: `module` is a loaded module and the name is NUL-terminated.
        let exported = unsafe { GetProcAddress(module, c"tanhf".as_ptr().cast()) };
        let linked = tanhf as unsafe extern "C" fn(f32) -> f32 as *const c_void;
        let code = linked.cast::<u8>();
        // SAFETY: `linked` is a function entry in a loaded image, so its first
        // instruction's bytes are readable; a thunk's slot (rip-relative) is a
        // pointer in the same image's import address table.
        let target = unsafe {
            let at = match std::slice::from_raw_parts(code, 3) {
                [0xff, 0x25, _] => Some(2),
                [0x48, 0xff, 0x25] => Some(3),
                _ => None,
            };
            match at {
                Some(at) => {
                    let disp = code.add(at).cast::<i32>().read_unaligned();
                    let slot = code.add(at + 4).offset(disp as isize);
                    slot.cast::<*const c_void>().read_unaligned()
                }
                None => linked,
            }
        };
        Some((module, exported, target))
    }

    /// The proof's key, if `f32::tanh` is `ucrtbase.dll`'s `tanhf`.
    pub(super) fn key() -> Option<String> {
        // `f32::tanh` calls the `tanhf` this crate links against; it must be
        // the one in the module whose bytes the key covers.
        let (module, exported, target) = linked_tanhf()?;
        if exported.is_null() || exported != target {
            return None;
        }
        // The loaded images themselves (not files that now have their names):
        // the module holding `tanh8` (found by its address, so also when this
        // crate is in a DLL) and `ucrtbase.dll`, holding `tanhf`. Each digest
        // must cover the code and tables the results come from.
        let ours: [*const u8; 5] = [
            tanh8 as unsafe fn(__m256) -> __m256 as *const u8,
            tanh_in_place as unsafe fn(&mut [f32]) as *const u8,
            super::libm_tanh_in_place_v1 as fn(&mut [f32]) as *const u8,
            LEAD.as_ptr().cast(),
            TRAIL.as_ptr().cast(),
        ];
        let mut own: *mut c_void = std::ptr::null_mut();
        // FROM_ADDRESS | UNCHANGED_REFCOUNT: the module containing `tanh8`,
        // which stays loaded while its code runs.
        // SAFETY: `ours[0]` is an address in a loaded module; `own` is a
        // valid out pointer.
        if unsafe { GetModuleHandleExW(0x4 | 0x2, ours[0].cast(), &mut own) } == 0 || own.is_null()
        {
            return None;
        }
        // SAFETY: both are modules loaded in this process while this runs.
        let (exe, ucrt) = unsafe {
            (
                image_digest(own.cast(), &ours)?,
                image_digest(module.cast(), &[target.cast()])?,
            )
        };
        let mut h = Sha256::new();
        h.update(ALGORITHM.as_bytes());
        h.update(exe);
        h.update(ucrt);
        for (feature, on) in [
            ("sse4.1", is_x86_feature_detected!("sse4.1")),
            ("avx", is_x86_feature_detected!("avx")),
            ("avx2", is_x86_feature_detected!("avx2")),
            ("fma", is_x86_feature_detected!("fma")),
            ("avx512f", is_x86_feature_detected!("avx512f")),
        ] {
            h.update(feature.as_bytes());
            h.update([u8::from(on)]);
        }
        Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// SHA-256 of a loaded PE32+ image's read-only sections as mapped, made
    /// independent of where it was loaded: each base-relocation field holds
    /// its load-address-relative value instead (the field minus the load
    /// address: the same for every load, and still naming the same target),
    /// the import address table (the imports' load addresses; which imports
    /// it names is in the import directory, which is hashed) is zeroed, and
    /// `.00cfg` (control-flow-guard pointers the loader fills) is left out.
    /// So it describes the code that is running, not whatever file now has
    /// its name. `None` for anything this does not model (another relocation
    /// type, a header out of bounds), which only means no proof record.
    ///
    /// `None` too if any executable section is not hashed (e.g. made
    /// writable at link time) or any of `covers` (addresses the results
    /// depend on) lies outside the hashed sections.
    ///
    /// # Safety
    /// `base` is the base address of a module loaded in this process.
    pub(super) unsafe fn image_digest(base: *const u8, covers: &[*const u8]) -> Option<[u8; 32]> {
        const EXECUTE: u32 = 0x2000_0000;
        const READ: u32 = 0x4000_0000;
        const WRITE: u32 = 0x8000_0000;
        // A loaded image's first page (its headers) is always mapped.
        const PAGE: usize = 4096;
        let u16_at = |o: usize| base.add(o).cast::<u16>().read_unaligned();
        let u32_at = |o: usize| base.add(o).cast::<u32>().read_unaligned();
        if u16_at(0) != 0x5a4d {
            return None;
        }
        let nt = u32_at(0x3c) as usize;
        let optional = nt.checked_add(24)?;
        if optional.checked_add(112)? > PAGE || u32_at(nt) != 0x4550 || u16_at(optional) != 0x20b {
            return None;
        }
        let sections = usize::from(u16_at(nt + 6));
        let optional_size = usize::from(u16_at(nt + 20));
        let image_size = u32_at(optional + 56) as usize;
        let headers = (u32_at(optional + 60) as usize).min(image_size);
        let directories = (u32_at(optional + 108) as usize).min(16);
        let table = optional + optional_size;
        if 112 + 8 * directories > optional_size
            || table + 40 * sections > headers.max(PAGE).min(image_size)
        {
            return None;
        }
        let directory = |i: usize| {
            (i < directories).then(|| {
                let at = optional + 112 + 8 * i;
                (u32_at(at) as usize, u32_at(at + 4) as usize)
            })
        };
        // Relocated fields: (offset, width).
        let mut relocated: Vec<(usize, usize)> = Vec::new();
        if let Some((rva, len)) = directory(5).filter(|d| d.1 > 0) {
            if rva.checked_add(len)? > image_size {
                return None;
            }
            let mut at = rva;
            while at + 8 <= rva + len {
                let page = u32_at(at) as usize;
                let block = u32_at(at + 4) as usize;
                if block < 8 || at + block > rva + len {
                    return None;
                }
                for k in 0..(block - 8) / 2 {
                    let entry = u16_at(at + 8 + 2 * k);
                    let offset = page + usize::from(entry & 0xfff);
                    match entry >> 12 {
                        0 => {}
                        3 => relocated.push((offset, 4)),
                        10 => relocated.push((offset, 8)),
                        _ => return None,
                    }
                }
                at += block;
            }
        }
        let iat = directory(12).filter(|d| d.1 > 0);
        let mut hashed: Vec<(usize, usize)> = Vec::new();
        let mut h = Sha256::new();
        for s in 0..sections {
            let header = table + 40 * s;
            let name = std::slice::from_raw_parts(base.add(header), 8);
            let size = u32_at(header + 8) as usize;
            let rva = u32_at(header + 12) as usize;
            let characteristics = u32_at(header + 36);
            if characteristics & READ == 0
                || characteristics & WRITE != 0
                || name.starts_with(b".00cfg")
            {
                if characteristics & EXECUTE != 0 {
                    return None;
                }
                continue;
            }
            if rva.checked_add(size)? > image_size {
                return None;
            }
            let mut bytes = std::slice::from_raw_parts(base.add(rva), size).to_vec();
            for &(at, width) in &relocated {
                if at + width <= rva || at >= rva + size {
                    continue;
                }
                if at < rva || at + width > rva + size {
                    return None;
                }
                let field = &mut bytes[at - rva..at - rva + width];
                if width == 8 {
                    let v = u64::from_le_bytes(field.try_into().ok()?);
                    field.copy_from_slice(&v.wrapping_sub(base as u64).to_le_bytes());
                } else {
                    let v = u32::from_le_bytes(field.try_into().ok()?);
                    field.copy_from_slice(&v.wrapping_sub(base as u32).to_le_bytes());
                }
            }
            if let Some((at, len)) = iat {
                let (from, to) = (at.max(rva), (at + len).min(rva + size));
                if from < to {
                    bytes[from - rva..to - rva].fill(0);
                }
            }
            h.update(name);
            h.update((rva as u64).to_le_bytes());
            h.update((size as u64).to_le_bytes());
            h.update(&bytes);
            hashed.push((rva, size));
        }
        let covered = |p: &*const u8| {
            let at = (*p as usize).wrapping_sub(base as usize);
            hashed
                .iter()
                .any(|&(rva, size)| at >= rva && at < rva + size)
        };
        if !covers.iter().all(covered) {
            return None;
        }
        Some(h.finalize().into())
    }

    /// `tanh8` against `tanhf` on every f32 input (NaN inputs excepted:
    /// `tanh_in_place` sends them to `tanhf`).
    pub(super) fn exhaustive_check() -> bool {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8)) as u64;
        let span = (1u64 << 32) / threads;
        std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let end = if t + 1 == threads {
                        1u64 << 32
                    } else {
                        (t + 1) * span
                    };
                    // A worker that cannot be created fails the check
                    // (`spawn` would panic instead).
                    std::thread::Builder::new().spawn_scoped(s, move || {
                        if !(default_mxcsr() && fma3_enabled()) {
                            return false;
                        }
                        let mut lanes = [0f32; 8];
                        let mut bits = t * span;
                        while bits < end {
                            for (i, lane) in lanes.iter_mut().enumerate() {
                                *lane = f32::from_bits((bits + i as u64) as u32);
                            }
                            let mut out = [0f32; 8];
                            // SAFETY: called only after AVX2 and FMA were detected.
                            unsafe {
                                _mm256_storeu_ps(
                                    out.as_mut_ptr(),
                                    tanh8(_mm256_loadu_ps(lanes.as_ptr())),
                                )
                            };
                            for (x, y) in lanes.iter().zip(out) {
                                if !x.is_nan() && x.tanh().to_bits() != y.to_bits() {
                                    return false;
                                }
                            }
                            bits += 8;
                        }
                        true
                    })
                })
                .collect();
            // Workers not joined here are joined when the scope ends.
            workers
                .into_iter()
                .all(|w| w.is_ok_and(|w| w.join().unwrap_or(false)))
        })
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
mod glibc {
    use sha2::{Digest, Sha256};
    use std::arch::x86_64::*;
    use std::ffi::{c_char, c_int, c_void, OsString};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;

    /// Version of the algorithm below; part of the proof record's key.
    const ALGORITHM: &str = "mtg-kernel-vector-tanh-v1:glibc-fdlibm-tanhf-avx2";

    // `s_expm1f.c`'s constants, as bits.
    const LN2_HI: u32 = 0x3f31_7180;
    const LN2_LO: u32 = 0x3717_f7d1;
    const INVLN2: u32 = 0x3fb8_aa3b;
    const Q1: u32 = 0xbd08_8889;
    const Q2: u32 = 0x3ad0_0d01;
    const Q3: u32 = 0xb8a6_70cd;
    const Q4: u32 = 0x3686_7e54;
    const Q5: u32 = 0xb457_edbb;

    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn bits(value: u32) -> __m256 {
        _mm256_castsi256_ps(_mm256_set1_epi32(value as i32))
    }

    /// `b` in the lanes `mask` sets, `a` elsewhere.
    #[inline]
    #[target_feature(enable = "avx2")]
    unsafe fn pick(a: __m256, b: __m256, mask: __m256i) -> __m256 {
        _mm256_blendv_ps(a, b, _mm256_castsi256_ps(mask))
    }

    /// The algorithm on eight lanes. NaN lanes give an unspecified value
    /// (callers replace them).
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn tanh8(x: __m256) -> __m256 {
        let int = |value: i32| _mm256_set1_epi32(value);
        let one = _mm256_set1_ps(1.0);
        let two = _mm256_set1_ps(2.0);
        let half = _mm256_set1_ps(0.5);
        let sign_mask = bits(0x8000_0000);
        let ax = _mm256_andnot_ps(sign_mask, x);
        let sign = _mm256_and_ps(sign_mask, x);
        // `tanhf`'s ranges, on the bits of |x| (non-negative as integers).
        let ix = _mm256_castps_si256(ax);
        // |x| < 2^-55, and zero: x.
        let tiny = _mm256_cmpgt_epi32(int(0x2400_0000), ix);
        // |x| >= 22, and infinity: 1 with x's sign.
        let big = _mm256_cmpgt_epi32(ix, int(0x41af_ffff));
        let at_least_one = _mm256_cmpgt_epi32(ix, int(0x3f7f_ffff));
        // `expm1f` returns its argument below 2^-25, so where |x| < 2^-26.
        let small = _mm256_cmpgt_epi32(int(0x3280_0000), ix);
        // Each branch reads |x| replaced by a fixed value of its own range in
        // the lanes that do not take it: the lanes it returns are unchanged,
        // and the others (discarded) never reach an infinity, an invalid
        // operation, an overflow or an underflow.
        let ax_tanh = pick(pick(ax, half, tiny), _mm256_set1_ps(1.5), big);
        let ax_expm1 = pick(ax_tanh, half, small);
        // `expm1f`'s argument a: 2|x| where |x| >= 1 and -2|x| below, both
        // exact products.
        let negative = _mm256_andnot_ps(_mm256_castsi256_ps(at_least_one), sign_mask);
        let a_small = _mm256_or_ps(_mm256_mul_ps(two, ax_tanh), negative);
        let magnitude = _mm256_mul_ps(two, ax_expm1);
        let a = _mm256_or_ps(magnitude, negative);
        // `expm1f`'s argument reduction a = k ln2 + r, on the bits of |a|.
        let hx = _mm256_castps_si256(magnitude);
        // |a| > 0.5 ln2; below it k = 0 and r = a.
        let reduced = _mm256_cmpgt_epi32(hx, int(0x3eb1_7218));
        // And |a| < 1.5 ln2: k = -1 here, where a >= 2 when it is positive.
        let nearest = _mm256_and_si256(reduced, _mm256_cmpgt_epi32(int(0x3f85_1592), hx));
        // Otherwise k = (int)(invln2 * a + (a < 0 ? -0.5 : 0.5)), truncated.
        let signed_half = _mm256_or_ps(half, _mm256_and_ps(a, sign_mask));
        let k = _mm256_cvttps_epi32(_mm256_add_ps(_mm256_mul_ps(bits(INVLN2), a), signed_half));
        let k = _mm256_and_si256(_mm256_blendv_epi8(k, int(-1), nearest), reduced);
        // With k = 0 these give r = a and c = 0, as `expm1f` has them.
        let t = _mm256_cvtepi32_ps(k);
        let hi = _mm256_sub_ps(a, _mm256_mul_ps(t, bits(LN2_HI)));
        let lo = _mm256_mul_ps(t, bits(LN2_LO));
        let r = _mm256_sub_ps(hi, lo);
        let c = _mm256_sub_ps(_mm256_sub_ps(hi, r), lo);
        // The primary range.
        let hfx = _mm256_mul_ps(half, r);
        let hxs = _mm256_mul_ps(r, hfx);
        let mut q = _mm256_add_ps(bits(Q4), _mm256_mul_ps(hxs, bits(Q5)));
        q = _mm256_add_ps(bits(Q3), _mm256_mul_ps(hxs, q));
        q = _mm256_add_ps(bits(Q2), _mm256_mul_ps(hxs, q));
        q = _mm256_add_ps(bits(Q1), _mm256_mul_ps(hxs, q));
        let r1 = _mm256_add_ps(one, _mm256_mul_ps(hxs, q));
        let t3 = _mm256_sub_ps(_mm256_set1_ps(3.0), _mm256_mul_ps(r1, hfx));
        let e = _mm256_mul_ps(
            hxs,
            _mm256_div_ps(
                _mm256_sub_ps(r1, t3),
                _mm256_sub_ps(_mm256_set1_ps(6.0), _mm256_mul_ps(r, t3)),
            ),
        );
        // k == 0: r - (r*e - hxs).
        let for_zero = _mm256_sub_ps(r, _mm256_sub_ps(_mm256_mul_ps(r, e), hxs));
        // Every other k continues from e = (r*(e - c) - c) - hxs.
        let e = _mm256_sub_ps(_mm256_sub_ps(_mm256_mul_ps(r, _mm256_sub_ps(e, c)), c), hxs);
        // k == -1: 0.5*(r - e) - 0.5.
        let for_minus_one = _mm256_sub_ps(_mm256_mul_ps(half, _mm256_sub_ps(r, e)), half);
        // The others add k to the exponent of a y near 1.
        let exponent = _mm256_slli_epi32::<23>(k);
        let scaled =
            |y: __m256| _mm256_castsi256_ps(_mm256_add_epi32(_mm256_castps_si256(y), exponent));
        let e_minus_r = _mm256_sub_ps(e, r);
        // k <= -2 or k > 56: y = 1 - (e - r), scaled, minus 1.
        let for_far = _mm256_sub_ps(scaled(_mm256_sub_ps(one, e_minus_r)), one);
        // 1 < k < 23: y = (1 - 2^-k) - (e - r), scaled.
        let below_one = _mm256_castsi256_ps(_mm256_sub_epi32(
            int(0x3f80_0000),
            _mm256_srlv_epi32(int(0x0100_0000), k),
        ));
        let for_low = scaled(_mm256_sub_ps(below_one, e_minus_r));
        // 23 <= k <= 56: y = (r - (e + 2^-k)) + 1, scaled.
        let power = _mm256_castsi256_ps(_mm256_slli_epi32::<23>(_mm256_sub_epi32(int(0x7f), k)));
        let for_high = scaled(_mm256_add_ps(
            _mm256_sub_ps(r, _mm256_add_ps(e, power)),
            one,
        ));
        let far = _mm256_or_si256(
            _mm256_cmpgt_epi32(int(-1), k),
            _mm256_cmpgt_epi32(k, int(56)),
        );
        let mut expm1 = pick(for_high, for_low, _mm256_cmpgt_epi32(int(23), k));
        expm1 = pick(expm1, for_far, far);
        expm1 = pick(expm1, for_minus_one, _mm256_cmpeq_epi32(k, int(-1)));
        expm1 = pick(
            expm1,
            for_zero,
            _mm256_cmpeq_epi32(k, _mm256_setzero_si256()),
        );
        expm1 = pick(expm1, a_small, small);
        // `tanhf`: 1 - 2/(t + 2) where |x| >= 1 and -t/(t + 2) below.
        let numerator = pick(_mm256_xor_ps(expm1, sign_mask), two, at_least_one);
        let quotient = _mm256_div_ps(numerator, _mm256_add_ps(expm1, two));
        let z = pick(quotient, _mm256_sub_ps(one, quotient), at_least_one);
        // 1 - 1e-30 is 1 where |x| >= 22; then x's sign, and x where tiny.
        pick(_mm256_xor_ps(pick(z, one, big), sign), x, tiny)
    }

    /// `tanh8` over a slice; NaN inputs take `tanhf`.
    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn tanh_in_place(values: &mut [f32]) {
        let mut chunks = values.chunks_exact_mut(8);
        for chunk in &mut chunks {
            let x = _mm256_loadu_ps(chunk.as_ptr());
            let nan = _mm256_movemask_ps(_mm256_cmp_ps::<_CMP_UNORD_Q>(x, x));
            if nan == 0 {
                _mm256_storeu_ps(chunk.as_mut_ptr(), tanh8(x));
            } else {
                for value in chunk.iter_mut() {
                    *value = value.tanh();
                }
            }
        }
        let rest = chunks.into_remainder();
        if !rest.is_empty() {
            let mut lanes = [0f32; 8];
            lanes[..rest.len()].copy_from_slice(rest);
            tanh_in_place(&mut lanes);
            rest.copy_from_slice(&lanes[..rest.len()]);
        }
    }

    /// Whether the calling thread's MXCSR control bits are the default:
    /// every exception masked, flush-to-zero and denormals-are-zero off,
    /// round to nearest. The proof runs in that state, so it is the one
    /// where `tanh8` is proved; the status flags are ignored.
    pub(super) fn default_mxcsr() -> bool {
        crate::deterministic_math_v1::read_mxcsr_v1() & 0xffc0 == 0x1f80
    }

    /// Whether `tanh8` is proved equal to this process's `tanhf`.
    pub(super) fn proved() -> bool {
        static PROVED: OnceLock<bool> = OnceLock::new();
        *PROVED.get_or_init(|| {
            if cfg!(debug_assertions)
                || std::env::var("MTG_KERNEL_VECTOR_TANH").is_ok_and(|v| v == "0")
                || !is_x86_feature_detected!("avx2")
            {
                return false;
            }
            // The proof covers the default MXCSR state.
            settle(record_dir(), key, || {
                default_mxcsr().then(exhaustive_check).flatten()
            })
        })
    }

    /// `proved` over its inputs (testable): the record folder, the key and
    /// the comparison (whether every bit is equal, if it could be made).
    ///
    /// A process that cannot keep the comparison's result does not make it:
    /// every later process would make it again, and a trainer chain starts
    /// one process per segment. So without a folder, or with one that cannot
    /// be written, this is false and `compare` is not called. The record's
    /// file is written before the comparison for that reason, and named for
    /// its outcome afterwards. A comparison that could not be made leaves no
    /// record.
    pub(super) fn settle(
        directory: Option<PathBuf>,
        key: impl FnOnce() -> Option<String>,
        compare: impl FnOnce() -> Option<bool>,
    ) -> bool {
        let Some(directory) = directory else {
            return false;
        };
        let Some(key) = key() else {
            return false;
        };
        let record = |outcome: &str| directory.join(format!("{key}.{outcome}"));
        let recorded =
            |outcome: &str| std::fs::read_to_string(record(outcome)).is_ok_and(|s| s.trim() == key);
        if recorded("proved") {
            return true;
        }
        if recorded("differs") {
            return false;
        }
        let pending = record(&format!("tmp-{}", std::process::id()));
        if std::fs::create_dir_all(&directory).is_err() || std::fs::write(&pending, &key).is_err() {
            return false;
        }
        let equal = compare();
        let kept = equal.is_some_and(|equal| {
            std::fs::rename(&pending, record(if equal { "proved" } else { "differs" })).is_ok()
        });
        if !kept {
            let _ = std::fs::remove_file(&pending);
        }
        equal == Some(true)
    }

    /// Where proof records live: `MTG_KERNEL_VECTOR_TANH_DIR`, else
    /// `mtg-kernel/vector-tanh` under the user's cache folder
    /// (`$XDG_CACHE_HOME` when it is an absolute path, else `$HOME/.cache`).
    fn record_dir() -> Option<PathBuf> {
        record_dir_from(|name| std::env::var_os(name))
    }

    /// `record_dir` over an environment lookup (testable).
    pub(super) fn record_dir_from(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
        let var = |name: &str| var(name).filter(|value| !value.is_empty());
        if let Some(dir) = var("MTG_KERNEL_VECTOR_TANH_DIR") {
            return Some(dir.into());
        }
        let cache = var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".cache")));
        cache.map(|base| base.join("mtg-kernel").join("vector-tanh"))
    }

    extern "C" {
        fn tanhf(x: f32) -> f32;
        fn dl_iterate_phdr(
            callback: unsafe extern "C" fn(*mut ObjectV1, usize, *mut c_void) -> c_int,
            data: *mut c_void,
        ) -> c_int;
    }

    /// The leading fields of `struct dl_phdr_info` (`<link.h>`) on x86_64.
    #[repr(C)]
    #[allow(dead_code, reason = "the C layout; not every field is read")]
    struct ObjectV1 {
        /// Where the object is loaded: added to each header's `vaddr`.
        base: u64,
        name: *const c_char,
        headers: *const ProgramHeaderV1,
        header_count: u16,
    }

    /// `Elf64_Phdr`.
    #[repr(C)]
    #[allow(dead_code, reason = "the C layout; not every field is read")]
    struct ProgramHeaderV1 {
        kind: u32,
        flags: u32,
        offset: u64,
        vaddr: u64,
        paddr: u64,
        file_size: u64,
        memory_size: u64,
        align: u64,
    }

    /// `hash_object`'s state. Nothing in it allocates while the loader calls.
    struct ImagesV1<'a> {
        hash: Sha256,
        /// Addresses the results depend on, and whether a hashed segment
        /// holds each.
        covers: &'a [*const u8],
        covered: &'a mut [bool],
        /// An object this does not model: no key.
        unmodelled: bool,
    }

    /// Hashes one loaded object's read-only segments as mapped
    /// (`dl_iterate_phdr`'s callback, called with the loader's lock held).
    unsafe extern "C" fn hash_object(
        object: *mut ObjectV1,
        size: usize,
        data: *mut c_void,
    ) -> c_int {
        const LOAD: u32 = 1;
        const WRITE: u32 = 2;
        const READ: u32 = 4;
        // SAFETY: `data` is the `ImagesV1` `images_digest` passed, used by
        // this call alone.
        let images = unsafe { &mut *data.cast::<ImagesV1<'_>>() };
        if size < std::mem::offset_of!(ObjectV1, header_count) + std::mem::size_of::<u16>() {
            images.unmodelled = true;
            return 1;
        }
        // SAFETY: the loader passes a `dl_phdr_info` of at least `size`
        // bytes, which covers the fields read here.
        let object = unsafe { &*object };
        let headers: &[ProgramHeaderV1] = if object.header_count == 0 {
            &[]
        } else {
            // SAFETY: the loader's program header table of a loaded object.
            unsafe { std::slice::from_raw_parts(object.headers, usize::from(object.header_count)) }
        };
        images.hash.update((headers.len() as u64).to_le_bytes());
        for header in headers {
            // A writable segment holds the relocated addresses and the data;
            // code and constants are in the others.
            if header.kind != LOAD || header.flags & WRITE != 0 {
                continue;
            }
            let start = object.base.checked_add(header.vaddr);
            let (Some(start), Ok(len)) = (start, usize::try_from(header.file_size)) else {
                images.unmodelled = true;
                return 1;
            };
            if header.flags & READ == 0 {
                // Execute-only: its bytes cannot be read.
                images.unmodelled = true;
                return 1;
            }
            // SAFETY: a loaded object's readable segment is mapped for its
            // file size at its load address, and stays mapped while the
            // loader's lock is held.
            let bytes = unsafe { std::slice::from_raw_parts(start as *const u8, len) };
            images.hash.update(header.vaddr.to_le_bytes());
            images.hash.update(header.file_size.to_le_bytes());
            images.hash.update(header.flags.to_le_bytes());
            images.hash.update(bytes);
            for (covered, address) in images.covered.iter_mut().zip(images.covers) {
                *covered |= (*address as u64).wrapping_sub(start) < header.file_size;
            }
        }
        0
    }

    /// SHA-256 of the read-only segments, as mapped, of every object loaded
    /// in this process, in the loader's order: the executable, the C library
    /// and its math library, and anything else that could supply or replace
    /// a symbol. Those segments are position independent, so the digest is
    /// the same wherever the objects are loaded; it describes the code that
    /// is running, not whatever files now have its names. `None` for anything
    /// this does not model (an execute-only segment, a short `dl_phdr_info`,
    /// 32-bit ELF structures) or if any of `covers` (addresses the results
    /// depend on) lies outside the hashed segments, which only means no proof
    /// record and so `tanhf` per value.
    pub(super) fn images_digest(covers: &[*const u8]) -> Option<[u8; 32]> {
        // The structures above are the 64-bit ELF ones (not x32's).
        if std::mem::size_of::<usize>() != 8 {
            return None;
        }
        let mut covered = vec![false; covers.len()];
        let mut images = ImagesV1 {
            hash: Sha256::new(),
            covers,
            covered: &mut covered,
            unmodelled: false,
        };
        // SAFETY: `hash_object` matches the callback's C signature and
        // `images` outlives the call.
        unsafe { dl_iterate_phdr(hash_object, std::ptr::from_mut(&mut images).cast()) };
        let ImagesV1 {
            hash, unmodelled, ..
        } = images;
        (!unmodelled && covered.iter().all(|covered| *covered)).then(|| hash.finalize().into())
    }

    /// The proof's key: the algorithm, every loaded image, the CPU features
    /// glibc can choose a `tanhf` by, and the tunables that can turn those
    /// features off for it.
    pub(super) fn key() -> Option<String> {
        use std::os::unix::ffi::OsStrExt;
        // `f32::tanh` calls the `tanhf` this crate links against. Its code
        // and this module's must be in the hashed segments.
        let covers: [*const u8; 4] = [
            tanh8 as unsafe fn(__m256) -> __m256 as *const u8,
            tanh_in_place as unsafe fn(&mut [f32]) as *const u8,
            super::libm_tanh_in_place_v1 as fn(&mut [f32]) as *const u8,
            tanhf as unsafe extern "C" fn(f32) -> f32 as *const u8,
        ];
        let images = images_digest(&covers)?;
        let mut h = Sha256::new();
        h.update(ALGORITHM.as_bytes());
        h.update(images);
        for (feature, on) in [
            ("sse4.1", is_x86_feature_detected!("sse4.1")),
            ("avx", is_x86_feature_detected!("avx")),
            ("avx2", is_x86_feature_detected!("avx2")),
            ("fma", is_x86_feature_detected!("fma")),
            ("avx512f", is_x86_feature_detected!("avx512f")),
            ("avx512vl", is_x86_feature_detected!("avx512vl")),
        ] {
            h.update(feature.as_bytes());
            h.update([u8::from(on)]);
        }
        let tunables = std::env::var_os("GLIBC_TUNABLES").unwrap_or_default();
        h.update((tunables.len() as u64).to_le_bytes());
        h.update(tunables.as_bytes());
        Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    /// The running C library's version, such as `2.39`.
    #[cfg(test)]
    pub(super) fn libc_version() -> String {
        extern "C" {
            fn gnu_get_libc_version() -> *const c_char;
        }
        // SAFETY: glibc returns a pointer to a static NUL-terminated string.
        unsafe { std::ffi::CStr::from_ptr(gnu_get_libc_version()) }
            .to_string_lossy()
            .into_owned()
    }

    /// `tanh8` against `tanhf` on every f32 input (NaN inputs excepted:
    /// `tanh_in_place` sends them to `tanhf`): whether every bit is equal, or
    /// `None` if the comparison could not be made (a worker that could not
    /// be created or that panicked, or one outside the default MXCSR state).
    pub(super) fn exhaustive_check() -> Option<bool> {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8)) as u64;
        let span = (1u64 << 32) / threads;
        let differs = AtomicBool::new(false);
        let differs = &differs;
        let complete = std::thread::scope(|s| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let end = if t + 1 == threads {
                        1u64 << 32
                    } else {
                        (t + 1) * span
                    };
                    // A worker that cannot be created fails the check
                    // (`spawn` would panic instead).
                    std::thread::Builder::new().spawn_scoped(s, move || {
                        // A new thread starts with its creator's MXCSR.
                        if !default_mxcsr() {
                            return false;
                        }
                        let mut lanes = [0f32; 8];
                        let mut bits = t * span;
                        while bits < end && !differs.load(Ordering::Relaxed) {
                            for (i, lane) in lanes.iter_mut().enumerate() {
                                *lane = f32::from_bits((bits + i as u64) as u32);
                            }
                            let mut out = [0f32; 8];
                            // SAFETY: called only after AVX2 was detected.
                            unsafe {
                                _mm256_storeu_ps(
                                    out.as_mut_ptr(),
                                    tanh8(_mm256_loadu_ps(lanes.as_ptr())),
                                )
                            };
                            for (x, y) in lanes.iter().zip(out) {
                                if !x.is_nan() && x.tanh().to_bits() != y.to_bits() {
                                    differs.store(true, Ordering::Relaxed);
                                }
                            }
                            bits += 8;
                        }
                        true
                    })
                })
                .collect();
            // Workers not joined here are joined when the scope ends.
            workers
                .into_iter()
                .all(|w| w.is_ok_and(|w| w.join().unwrap_or(false)))
        });
        if differs.load(Ordering::Relaxed) {
            Some(false)
        } else {
            complete.then_some(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slice form equals `f32::tanh` bit for bit on typical and edge
    /// inputs (every length, so the tail path runs), proved or not.
    #[test]
    fn libm_tanh_in_place_matches_scalar_tanh() {
        let mut values = vec![
            0.0f32,
            -0.0,
            1e-30,
            -1e-6,
            1.220703e-4,
            3.5e-4,
            0.0486,
            0.5,
            -0.8999,
            0.9,
            0.95,
            1.0,
            1.000_001,
            1.394_659,
            2.290_476,
            -5.0,
            9.010_914,
            9.5,
            10.0,
            10.000_001,
            88.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::MIN_POSITIVE,
            f32::MAX,
        ];
        let mut s = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..4000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            values.push(f32::from_bits(s as u32));
            values.push(((s >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 24.0);
        }
        for len in 0..values.len().min(40) {
            let mut v = values[..len].to_vec();
            libm_tanh_in_place_v1(&mut v);
            for (a, b) in values[..len].iter().zip(&v) {
                assert_eq!(a.tanh().to_bits(), b.to_bits(), "{a:e}");
            }
        }
        let mut v = values.clone();
        libm_tanh_in_place_v1(&mut v);
        for (a, b) in values.iter().zip(&v) {
            assert!(
                a.tanh().to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
                "{a:e}"
            );
        }
    }

    /// On a Windows CPU with AVX2 and FMA, the eight-lane algorithm equals
    /// this machine's `tanhf` on every f32 input (the proof `proved` runs).
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn vector_tanh_equals_tanhf_on_every_input_here() {
        if !(is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")) {
            return;
        }
        assert!(windows::exhaustive_check());
    }

    /// The loaded executable and `ucrtbase.dll` have image digests (so a
    /// proof record can be keyed), the same on every call.
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn image_digests_exist_and_are_stable() {
        let first = windows::key();
        assert!(first.is_some());
        assert_eq!(first, windows::key());
    }

    /// The `tanhf` this crate links is `ucrtbase.dll`'s, so a proof record
    /// can be keyed (otherwise the vector form never turns on).
    /// UCRT's FMA3 switch is readable and on here (the state the proof and
    /// every vector call require).
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn ucrt_fma3_paths_are_on() {
        if is_x86_feature_detected!("fma") {
            assert!(windows::fma3_enabled());
            assert!(windows::default_mxcsr());
        }
    }

    /// Proof records are found without `LOCALAPPDATA` (an allowlisted
    /// launcher environment), in the folder Windows' default would give.
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn proof_record_folder_falls_back_to_the_profile() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let tail = |base: &str| PathBuf::from(base).join("mtg-kernel").join("vector-tanh");
        assert_eq!(
            windows::record_dir_from(env(&[
                ("MTG_KERNEL_VECTOR_TANH_DIR", "D:/proofs"),
                ("LOCALAPPDATA", "C:/L"),
                ("USERPROFILE", "C:/U"),
            ])),
            Some(PathBuf::from("D:/proofs"))
        );
        assert_eq!(
            windows::record_dir_from(env(&[("LOCALAPPDATA", "C:/L"), ("USERPROFILE", "C:/U")])),
            Some(tail("C:/L"))
        );
        let profile = PathBuf::from("C:/U").join("AppData").join("Local");
        assert_eq!(
            windows::record_dir_from(env(&[("USERPROFILE", "C:/U")])),
            Some(profile.join("mtg-kernel").join("vector-tanh"))
        );
        assert_eq!(windows::record_dir_from(env(&[])), None);
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn linked_tanhf_is_ucrtbase_export() {
        assert!(
            windows::key().is_some_and(|k| k.len() == 64),
            "{:?}",
            windows::linked_tanhf()
        );
    }

    /// The C library releases whose `tanhf` `glibc::tanh8` was compared with
    /// on every input when it was written: there the comparison must hold.
    /// Elsewhere it only has to give an answer.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    fn glibc_tanhf_is_the_one_written_against() -> bool {
        GLIBC_VERSIONS_COMPARED.contains(&glibc::libc_version().as_str())
    }

    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    const GLIBC_VERSIONS_COMPARED: [&str; 1] = ["2.39"];

    /// On glibc with AVX2, the eight-lane algorithm is compared with this
    /// machine's `tanhf` on every f32 input (the proof `proved` runs), and
    /// equals it where the C library is one it was written against.
    /// Optimized builds only: an unoptimized one takes minutes.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    #[test]
    fn glibc_vector_tanh_is_compared_with_tanhf_on_every_input_here() {
        if cfg!(debug_assertions) || !is_x86_feature_detected!("avx2") {
            return;
        }
        let equal = glibc::exhaustive_check();
        assert!(equal.is_some());
        if glibc_tanhf_is_the_one_written_against() {
            assert_eq!(equal, Some(true));
        }
    }

    /// The eight-lane algorithm at and around every range boundary of
    /// `tanhf` and `expm1f`, both signs, in every lane, against `tanhf`.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    #[test]
    fn glibc_vector_tanh_equals_tanhf_around_every_range_boundary() {
        use std::arch::x86_64::*;
        if !is_x86_feature_detected!("avx2") || !glibc_tanhf_is_the_one_written_against() {
            return;
        }
        // Bits of |x| where `tanhf` or `expm1f(+-2|x|)` changes branch: zero,
        // the smallest normal, 2^-55, 2^-26, 0.25 ln2, 0.75 ln2, 1, 22, the
        // largest finite value and infinity.
        let mut centers = vec![
            0u32,
            0x0080_0000,
            0x2400_0000,
            0x3280_0000,
            0x3e31_7218,
            0x3f05_1592,
            0x3f80_0000,
            0x41b0_0000,
            0x7f7f_ffff,
            0x7f80_0000,
        ];
        // And where the exponent k of 2|x| = k ln2 + r changes, with the
        // middle of each k.
        for k in 1..=64 {
            let middle = k as f32 * std::f32::consts::LN_2 * 0.5;
            let edge = (k as f32 - 0.5) * std::f32::consts::LN_2 * 0.5;
            centers.extend([middle.to_bits(), edge.to_bits()]);
        }
        let mut inputs = Vec::new();
        for center in centers {
            for offset in -48i64..=48 {
                let bits = i64::from(center) + offset;
                if (0..=0x7f80_0000).contains(&bits) {
                    inputs.push(f32::from_bits(bits as u32));
                    inputs.push(-f32::from_bits(bits as u32));
                }
            }
        }
        let mut s = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..20_000 {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            inputs.push(f32::from_bits(s as u32));
            inputs.push(((s >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 48.0);
        }
        inputs.retain(|x| !x.is_nan());
        for lane in 0..8 {
            for &x in &inputs {
                // The other lanes hold values of other ranges.
                let mut lanes = [0.3f32, -7.0, 1e-20, 30.0, -0.9, 2.5, -1e-9, 0.02];
                lanes[lane] = x;
                let mut out = [0f32; 8];
                // SAFETY: AVX2 was detected above.
                unsafe {
                    _mm256_storeu_ps(
                        out.as_mut_ptr(),
                        glibc::tanh8(_mm256_loadu_ps(lanes.as_ptr())),
                    )
                };
                for (input, output) in lanes.iter().zip(out) {
                    assert_eq!(input.tanh().to_bits(), output.to_bits(), "{input:e}");
                }
            }
        }
    }

    /// The loaded images have a digest (so a proof record can be keyed), the
    /// same on every call.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    #[test]
    fn glibc_image_digests_exist_and_are_stable() {
        let first = glibc::key();
        assert!(
            first.as_ref().is_some_and(|key| key.len() == 64),
            "{first:?}"
        );
        assert_eq!(first, glibc::key());
        // An address in no loaded image's read-only segments (the stack).
        let local = 0u8;
        assert_eq!(glibc::images_digest(&[std::ptr::from_ref(&local)]), None);
        assert!(glibc::images_digest(&[]).is_some());
    }

    /// Proof records go to the named folder, else the user's cache folder.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    #[test]
    fn glibc_proof_record_folder_is_the_cache_folder() {
        use std::ffi::OsString;
        use std::path::PathBuf;
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let tail = |base: &str| PathBuf::from(base).join("mtg-kernel").join("vector-tanh");
        assert_eq!(
            glibc::record_dir_from(env(&[
                ("MTG_KERNEL_VECTOR_TANH_DIR", "/proofs"),
                ("XDG_CACHE_HOME", "/x"),
                ("HOME", "/h"),
            ])),
            Some(PathBuf::from("/proofs"))
        );
        assert_eq!(
            glibc::record_dir_from(env(&[("XDG_CACHE_HOME", "/x"), ("HOME", "/h")])),
            Some(tail("/x"))
        );
        // A relative or empty `XDG_CACHE_HOME` is not one.
        assert_eq!(
            glibc::record_dir_from(env(&[("XDG_CACHE_HOME", "x"), ("HOME", "/h")])),
            Some(tail("/h/.cache"))
        );
        assert_eq!(
            glibc::record_dir_from(env(&[
                ("MTG_KERNEL_VECTOR_TANH_DIR", ""),
                ("XDG_CACHE_HOME", ""),
                ("HOME", "/h"),
            ])),
            Some(tail("/h/.cache"))
        );
        assert_eq!(glibc::record_dir_from(env(&[])), None);
    }

    /// The comparison is made once per key and only where its result can be
    /// kept: never without a folder or with one that cannot be written, and
    /// not again once either outcome is recorded.
    #[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
    #[test]
    fn glibc_comparison_is_made_only_where_its_result_is_kept() {
        use std::cell::Cell;
        use std::path::PathBuf;
        let folder = std::env::temp_dir().join(format!(
            "mtg-kernel-vector-tanh-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let key = |name: &str| {
            let name = name.to_owned();
            move || Some(name)
        };
        let made = Cell::new(0);
        let compare = |outcome: Option<bool>| {
            let made = &made;
            move || {
                made.set(made.get() + 1);
                outcome
            }
        };
        let files = |folder: &PathBuf| {
            let mut names: Vec<String> = std::fs::read_dir(folder)
                .map(|entries| {
                    entries
                        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        };

        // No folder, and no key: no comparison.
        assert!(!glibc::settle(None, key("a"), compare(Some(true))));
        assert!(!glibc::settle(
            Some(folder.clone()),
            || None,
            compare(Some(true))
        ));
        assert_eq!(made.get(), 0);
        assert!(!folder.exists());

        // A folder that cannot be made (its parent is a file): no comparison.
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("file");
        std::fs::write(&file, b"").unwrap();
        assert!(!glibc::settle(
            Some(file.join("under")),
            key("a"),
            compare(Some(true))
        ));
        assert_eq!(made.get(), 0);
        std::fs::remove_file(&file).unwrap();

        // Equal: recorded, and read by the next process without comparing.
        assert!(glibc::settle(
            Some(folder.clone()),
            key("a"),
            compare(Some(true))
        ));
        assert_eq!(made.get(), 1);
        assert_eq!(files(&folder), ["a.proved"]);
        assert_eq!(
            std::fs::read_to_string(folder.join("a.proved")).unwrap(),
            "a"
        );
        assert!(glibc::settle(
            Some(folder.clone()),
            key("a"),
            compare(Some(false))
        ));
        assert_eq!(made.get(), 1);

        // Differing: recorded too, and not compared again.
        assert!(!glibc::settle(
            Some(folder.clone()),
            key("b"),
            compare(Some(false))
        ));
        assert!(!glibc::settle(
            Some(folder.clone()),
            key("b"),
            compare(Some(true))
        ));
        assert_eq!(made.get(), 2);
        assert_eq!(files(&folder), ["a.proved", "b.differs"]);

        // A comparison that could not be made leaves nothing, so the next
        // process makes it.
        assert!(!glibc::settle(
            Some(folder.clone()),
            key("c"),
            compare(None)
        ));
        assert_eq!(files(&folder), ["a.proved", "b.differs"]);
        assert!(glibc::settle(
            Some(folder.clone()),
            key("c"),
            compare(Some(true))
        ));
        assert_eq!(made.get(), 4);

        // A record that does not hold its key is not one.
        std::fs::write(folder.join("d.proved"), "a").unwrap();
        assert!(!glibc::settle(
            Some(folder.clone()),
            key("d"),
            compare(Some(false))
        ));
        assert_eq!(made.get(), 5);
        assert_eq!(
            std::fs::read_to_string(folder.join("d.differs")).unwrap(),
            "d"
        );

        std::fs::remove_dir_all(&folder).unwrap();
    }
}

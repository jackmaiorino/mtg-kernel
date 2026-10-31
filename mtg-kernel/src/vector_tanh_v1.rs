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

/// `*v = v.tanh()` for every value, with the same bits.
pub(crate) fn libm_tanh_in_place_v1(values: &mut [f32]) {
    #[cfg(all(windows, target_arch = "x86_64"))]
    if windows::default_mxcsr() && windows::fma3_enabled() && windows::proved() {
        // SAFETY: `proved` holds only where AVX2 and FMA are present.
        unsafe { windows::tanh_in_place(values) };
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
}

//! Host class (the research lead's ruling 10 on the sizing note's Section 5)
//! and peak memory.
//!
//! A host class is the tuple (CPU model, OS family, target triple, trainer
//! binary SHA-256), written in full, with one of the short names
//! `win11-13700k`, `win11-5900xt`, `linux-9950x` or `runpod-epyc-<vcpus>`. A
//! qualification receipt is valid for its class only; any other host (the Mac
//! Studio among them) can run the qualification workload as engineering
//! verification, but its runs cannot be locked into a manifest.
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostClassV1 {
    pub short_name: String,
    pub cpu_model: String,
    pub os_family: String,
    pub target_triple: String,
    pub trainer_binary_sha256: String,
}

/// The target triple this binary was compiled for.
pub fn target_triple_v1() -> String {
    let vendor_os = match std::env::consts::OS {
        "windows" => "pc-windows",
        "linux" => "unknown-linux",
        "macos" => "apple-darwin",
        other => other,
    };
    let environment = if cfg!(target_env = "msvc") {
        "-msvc"
    } else if cfg!(target_env = "gnu") {
        "-gnu"
    } else if cfg!(target_env = "musl") {
        "-musl"
    } else {
        ""
    };
    format!("{}-{vendor_os}{environment}", std::env::consts::ARCH)
}

fn command_output(program: &str, arguments: &[&str]) -> Result<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("{program}: {error}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("{program} printed non-UTF-8 text"))
}

/// The CPU model string the operating system reports: `model name` of
/// `/proc/cpuinfo` on Linux, `machdep.cpu.brand_string` on macOS and the
/// registry's `ProcessorNameString` on Windows.
pub fn cpu_model_v1() -> Result<String> {
    let model = match std::env::consts::OS {
        "linux" => {
            let text = std::fs::read_to_string("/proc/cpuinfo")
                .map_err(|error| format!("/proc/cpuinfo: {error}"))?;
            text.lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    (key.trim() == "model name").then(|| value.trim().to_string())
                })
                .ok_or("/proc/cpuinfo names no model")?
        }
        "macos" => command_output("sysctl", &["-n", "machdep.cpu.brand_string"])?
            .trim()
            .to_string(),
        "windows" => {
            let text = command_output(
                "reg",
                &[
                    "query",
                    r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0",
                    "/v",
                    "ProcessorNameString",
                ],
            )?;
            text.lines()
                .find_map(|line| {
                    let (_, value) = line.split_once("REG_SZ")?;
                    Some(value.trim().to_string())
                })
                .ok_or("the registry names no processor")?
        }
        other => return Err(format!("no CPU model reader for {other}")),
    };
    if model.is_empty() {
        return Err("the CPU model is empty".into());
    }
    Ok(model)
}

/// The host class of this process under `short_name`.
pub fn measure_host_class_v1(short_name: &str, trainer_binary_sha256: &str) -> Result<HostClassV1> {
    if short_name.is_empty()
        || !short_name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(format!(
            "host class name {short_name:?} must be lowercase letters, digits and dashes"
        ));
    }
    Ok(HostClassV1 {
        short_name: short_name.into(),
        cpu_model: cpu_model_v1()?,
        os_family: std::env::consts::OS.into(),
        target_triple: target_triple_v1(),
        trainer_binary_sha256: trainer_binary_sha256.into(),
    })
}

/// Refuses a host class whose short name is not one the ruling defines, or
/// whose tuple does not fit its name: `win11-13700k` is Windows with an
/// i7-13700K, `win11-5900xt` Windows with a Ryzen 9 5900XT, `linux-9950x`
/// Linux with a Ryzen 9 9950X, and `runpod-epyc-<vcpus>` Linux with an EPYC
/// and exactly `<vcpus>` logical processors (`available_vcpus`).
pub fn admit_receipt_host_class_v1(host: &HostClassV1, available_vcpus: usize) -> Result<()> {
    let cpu = host.cpu_model.to_ascii_uppercase();
    let fits = match host.short_name.as_str() {
        "win11-13700k" => host.os_family == "windows" && cpu.contains("13700K"),
        "win11-5900xt" => host.os_family == "windows" && cpu.contains("5900XT"),
        "linux-9950x" => host.os_family == "linux" && cpu.contains("9950X"),
        name => match name.strip_prefix("runpod-epyc-") {
            Some(vcpus) => {
                host.os_family == "linux"
                    && cpu.contains("EPYC")
                    && vcpus.parse::<usize>().ok() == Some(available_vcpus)
                    && !vcpus.starts_with('0')
            }
            None => {
                return Err(format!(
                    "host class {name:?} is not a receipt class (win11-13700k, win11-5900xt, \
                     linux-9950x, runpod-epyc-<vcpus>)"
                ))
            }
        },
    };
    if !fits {
        return Err(format!(
            "host class {} does not fit this host ({}, {})",
            host.short_name, host.os_family, host.cpu_model
        ));
    }
    Ok(())
}

/// Peak resident memory of this process in bytes, when the platform reports
/// it. Measurement only; nothing depends on it.
pub fn peak_rss_bytes_v1() -> Option<u64> {
    platform::peak_rss_bytes()
}

#[cfg(unix)]
mod platform {
    /// `struct rusage` on 64-bit Unix: two `timeval`s (16 bytes each), then
    /// fourteen `long`s, of which `ru_maxrss` is the first.
    #[repr(C)]
    struct Rusage {
        words: [i64; 18],
    }
    extern "C" {
        fn getrusage(who: i32, usage: *mut Rusage) -> i32;
    }
    pub(super) fn peak_rss_bytes() -> Option<u64> {
        if std::mem::size_of::<std::ffi::c_long>() != 8 {
            return None;
        }
        let mut usage = Rusage { words: [0; 18] };
        // SAFETY: `usage` is a zeroed buffer of the size of `struct rusage`
        // on 64-bit Unix, and RUSAGE_SELF (0) reads this process.
        if unsafe { getrusage(0, &mut usage) } != 0 {
            return None;
        }
        let maxrss = u64::try_from(usage.words[4]).ok()?;
        // Linux reports kibibytes, macOS bytes.
        Some(if cfg!(target_os = "macos") {
            maxrss
        } else {
            maxrss.saturating_mul(1024)
        })
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;

    // Every field is part of the Win32 layout; only two are read.
    #[allow(dead_code)]
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }
    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
    }
    pub(super) fn peak_rss_bytes() -> Option<u64> {
        let mut counters = ProcessMemoryCounters {
            cb: u32::try_from(std::mem::size_of::<ProcessMemoryCounters>()).ok()?,
            page_fault_count: 0,
            peak_working_set_size: 0,
            working_set_size: 0,
            quota_peak_paged_pool_usage: 0,
            quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0,
            quota_non_paged_pool_usage: 0,
            pagefile_usage: 0,
            peak_pagefile_usage: 0,
        };
        // SAFETY: the pseudo-handle needs no closing and `counters` is a
        // zeroed `PROCESS_MEMORY_COUNTERS` with its size set.
        let succeeded = unsafe {
            GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) != 0
        };
        succeeded.then_some(counters.peak_working_set_size as u64)
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    pub(super) fn peak_rss_bytes() -> Option<u64> {
        None
    }
}

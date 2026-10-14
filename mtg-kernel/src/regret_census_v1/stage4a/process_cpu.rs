//! Process CPU time includes all threads, unlike elapsed wall time.

#[cfg(windows)]
pub(crate) fn seconds() -> Result<f64, String> {
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // SAFETY: valid current-process pseudo handle and writable FILETIME buffers.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let ticks = |t: FileTime| (u64::from(t.high) << 32) | u64::from(t.low);
    Ok((ticks(kernel) + ticks(user)) as f64 * 1e-7)
}

#[cfg(target_os = "linux")]
pub(crate) fn seconds() -> Result<f64, String> {
    #[repr(C)]
    struct Timespec {
        sec: std::ffi::c_long,
        nano: std::ffi::c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, out: *mut Timespec) -> i32;
    }
    let mut out = Timespec { sec: 0, nano: 0 };
    // SAFETY: CLOCK_PROCESS_CPUTIME_ID is 2 on Linux; out is writable.
    if unsafe { clock_gettime(2, &mut out) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(out.sec as f64 + out.nano as f64 * 1e-9)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) fn seconds() -> Result<f64, String> {
    Err("process CPU clock supported only on Windows and Linux".into())
}

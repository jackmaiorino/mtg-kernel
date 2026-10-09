//! Opt-in phase timers for profiling (`S4A_PROFILE=1`); off by default and
//! never in formal runs. Per-thread accumulators of seconds and call counts.

use serde_json::{json, Value};
use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Instant;

pub(crate) const STEP: usize = 0;
pub(crate) const INFER: usize = 1;
pub(crate) const CLONE: usize = 2;
pub(crate) const SAMPLE: usize = 3;
pub(crate) const CANON: usize = 4;
pub(crate) const TREE: usize = 5;
pub(crate) const LABELS: usize = 6;
const NAMES: [&str; 7] = ["engine_step", "inference", "session_clone", "sampler", "canon_key", "tree", "labels"];

thread_local! {
    static ACC: RefCell<[(f64, u64); 7]> = const { RefCell::new([(0.0, 0); 7]) };
}

fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("S4A_PROFILE").is_some())
}

pub(crate) fn timed<T>(cat: usize, f: impl FnOnce() -> T) -> T {
    if !enabled() {
        return f();
    }
    let t = Instant::now();
    let r = f();
    let dt = t.elapsed().as_secs_f64();
    ACC.with(|a| {
        let mut a = a.borrow_mut();
        a[cat].0 += dt;
        a[cat].1 += 1;
    });
    r
}

/// Takes and resets this thread's accumulators.
pub(crate) fn take() -> Value {
    if !enabled() {
        return Value::Null;
    }
    ACC.with(|a| {
        let mut a = a.borrow_mut();
        let v: serde_json::Map<String, Value> = NAMES
            .iter()
            .zip(a.iter())
            .map(|(n, (s, c))| ((*n).to_owned(), json!({"seconds":s,"calls":c})))
            .collect();
        *a = [(0.0, 0); 7];
        Value::Object(v)
    })
}

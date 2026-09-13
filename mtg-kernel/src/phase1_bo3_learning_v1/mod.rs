//! Explicit native BO3 capture, preparation and opt-in CPU continuation.
//! Legacy objectives/readers remain strict. There is no training loop.
//! Producer artifact verification is distinct from this process's runtime.

mod capture;
mod continuation;
mod preparation;
pub use capture::*;
pub(crate) use capture::{CaptureBuffer, PendingNativeCapture};
pub(crate) use continuation::load_bo3_inference_v1;
pub use continuation::*;
pub use preparation::*;

fn require(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn canonical_size<T: serde::Serialize>(value: &T, maximum: u64) -> Result<u64, String> {
    struct Counter {
        count: u64,
        maximum: u64,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let next = self
                .count
                .checked_add(bytes.len() as u64)
                .filter(|&n| n <= self.maximum)
                .ok_or_else(|| std::io::Error::other("canonical JSON limit reached"))?;
            self.count = next;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { count: 0, maximum };
    serde_json::to_writer(&mut counter, value).map_err(|e| e.to_string())?;
    Ok(counter.count)
}

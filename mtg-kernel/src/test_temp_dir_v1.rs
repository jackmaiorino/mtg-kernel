// Shared cfg(test) RAII temporary directory guard under `std::env::temp_dir()`.
// Removes its directory on normal drop (`!std::thread::panicking()`) while
// preserving it when a test unwinds from a panic so failure artifacts remain
// inspectable for diagnosis.

use std::ffi::OsStr;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_TEST_DIR_ID_V1: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct TempTestDir {
    path: PathBuf,
}

impl TempTestDir {
    pub(crate) fn new_v1(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let seq = NEXT_TEMP_TEST_DIR_ID_V1.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{seq}", std::process::id()));
        fs::create_dir_all(&path).expect("create isolated test temp directory");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Deref for TempTestDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<Path> for TempTestDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for TempTestDir {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TempTestDir;
    use std::fs;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::{Path, PathBuf};

    #[test]
    fn temp_test_dir_removes_directory_on_normal_drop() {
        let captured: PathBuf;
        {
            let dir = TempTestDir::new_v1("temp-test-dir-normal-drop");
            let as_path: &Path = dir.as_ref();
            assert_eq!(as_path, dir.path());
            assert!(dir.is_dir());
            fs::write(dir.join("sentinel.txt"), b"ok").unwrap();
            captured = dir.to_path_buf();
            assert!(captured.join("sentinel.txt").is_file());
        }
        assert!(
            !captured.exists(),
            "normal drop must remove temporary directory {}",
            captured.display()
        );
    }

    #[test]
    fn temp_test_dir_preserves_directory_on_panic_for_diagnosis() {
        let mut captured = PathBuf::new();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let dir = TempTestDir::new_v1("temp-test-dir-panic-preserve");
            captured = dir.to_path_buf();
            fs::write(dir.join("diagnosis.txt"), b"keep-on-panic").unwrap();
            panic!("simulated test failure");
        }));
        assert!(outcome.is_err(), "panic must unwind through catch_unwind");
        assert!(
            captured.join("diagnosis.txt").is_file(),
            "panicking drop must preserve temporary directory {} for diagnosis",
            captured.display()
        );
        fs::remove_dir_all(&captured).unwrap();
        assert!(!captured.exists());
    }
}

use std::fs::{self, File, TryLockError};
use std::io;
use std::path::Path;

pub(super) const NAME: &str = "driver.lock";

/// The driver holds an exclusive lock on this file from before spawn until
/// the child exits. The child gets its own handle, never the driver's lock.
pub(super) struct DriverLock(File);

impl DriverLock {
    pub(super) fn open(spool: &Path) -> io::Result<Self> {
        let path = spool.join(NAME);
        if !fs::symlink_metadata(&path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "follow driver lock must be a regular file",
            ));
        }
        File::open(path).map(Self)
    }

    pub(super) fn held(&self) -> io::Result<bool> {
        match self.0.try_lock_shared() {
            Err(TryLockError::WouldBlock) => Ok(true),
            Err(TryLockError::Error(error)) => Err(error),
            Ok(()) => {
                self.0.unlock()?;
                Ok(false)
            }
        }
    }
}

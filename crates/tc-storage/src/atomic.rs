use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use fs4::FileExt;
use fs4::TryLockError;

use crate::error::{StorageError, StorageResult};

/// Maximum time we will wait for an exclusive file lock before giving up.
const LOCK_TIMEOUT: Duration = Duration::from_secs(5);

/// Backoff between retry attempts while waiting on a lock.
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Write `content` to `path` atomically.
///
/// Implementation: write to `{path}.tmp.{pid}`, `fsync`, then `rename`.
/// On POSIX the rename is atomic -- readers either see the old file or the
/// new one, never a partial write. If the process dies between the write
/// and the rename, the original file is untouched and a stale `.tmp.*`
/// file is left behind (ignored by readers).
pub fn write_atomic(path: &Path, content: &[u8]) -> StorageResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.exists() {
        std::fs::create_dir_all(parent).map_err(|e| StorageError::dir_create(parent, e))?;
    }

    let tmp_name = match path.file_name() {
        Some(n) => {
            let mut s = n.to_os_string();
            s.push(format!(".tmp.{}", std::process::id()));
            s
        }
        None => return Err(StorageError::file_write(path, invalid_path_io_error())),
    };
    let tmp_path = parent.join(&tmp_name);

    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| StorageError::file_write(&tmp_path, e))?;
        f.write_all(content)
            .map_err(|e| StorageError::file_write(&tmp_path, e))?;
        f.sync_all()
            .map_err(|e| StorageError::file_write(&tmp_path, e))?;
    }

    std::fs::rename(&tmp_path, path).map_err(|e| {
        // Best-effort cleanup; the real error is the rename failure.
        let _ = std::fs::remove_file(&tmp_path);
        StorageError::file_write(path, e)
    })?;

    Ok(())
}

/// Run `f` while holding an exclusive advisory lock on `lock_path`.
///
/// If the lock is held by another process, retries with a short backoff
/// until [`LOCK_TIMEOUT`] elapses, at which point [`StorageError::LockTimeout`]
/// is returned. The lock file itself is created (if missing) but its
/// contents are never written -- its presence is purely for `flock`.
pub fn with_exclusive_lock<T, F>(lock_path: &Path, f: F) -> StorageResult<T>
where
    F: FnOnce() -> StorageResult<T>,
{
    if let Some(parent) = lock_path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| StorageError::dir_create(parent, e))?;
    }

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|e| StorageError::file_write(lock_path, e))?;

    acquire_with_retry(&file, lock_path)?;

    let result = f();

    // Release the lock explicitly so readers/writers waiting on the same
    // file see it available immediately (Drop would also release, but
    // explicit unlock keeps semantics obvious).
    let _ = FileExt::unlock(&file);

    result
}

fn acquire_with_retry(file: &File, lock_path: &Path) -> StorageResult<()> {
    let start = Instant::now();
    'retry: loop {
        match FileExt::try_lock(file) {
            Ok(()) => return Ok(()),
            Err(TryLockError::WouldBlock) => {
                if start.elapsed() >= LOCK_TIMEOUT {
                    return Err(StorageError::LockTimeout {
                        path: lock_path.to_path_buf(),
                        seconds: LOCK_TIMEOUT.as_secs(),
                    });
                }
                std::thread::sleep(LOCK_RETRY_INTERVAL);
                continue 'retry;
            }
            Err(TryLockError::Error(e)) => {
                return Err(StorageError::file_write(lock_path, e));
            }
        }
    }
}

fn invalid_path_io_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, "path has no file name")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn write_atomic_creates_file_with_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn write_atomic_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        std::fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn write_atomic_creates_parent_dir() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/deeper/foo.txt");
        write_atomic(&path, b"x").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn write_atomic_leaves_no_tmp_after_success() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("foo.txt");
        write_atomic(&path, b"x").unwrap();
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("foo.txt.tmp."))
            .collect();
        assert!(tmp_files.is_empty(), "stray tmp files: {tmp_files:?}");
    }

    #[test]
    fn lock_serializes_critical_section() {
        let dir = tempdir().unwrap();
        let lock = Arc::new(dir.path().join("lock"));
        let counter = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let lock = Arc::clone(&lock);
                let counter = Arc::clone(&counter);
                let max = Arc::clone(&max);
                thread::spawn(move || {
                    with_exclusive_lock(&lock, || {
                        let c = counter.fetch_add(1, Ordering::SeqCst) + 1;
                        let prev = max.load(Ordering::SeqCst);
                        if c > prev {
                            max.store(c, Ordering::SeqCst);
                        }
                        thread::sleep(Duration::from_millis(5));
                        counter.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            max.load(Ordering::SeqCst),
            1,
            "more than one thread entered the critical section simultaneously",
        );
    }

    #[test]
    fn lock_returns_value() {
        let dir = tempdir().unwrap();
        let lock = dir.path().join("lock");
        let v: i32 = with_exclusive_lock(&lock, || Ok(42)).unwrap();
        assert_eq!(v, 42);
    }

    #[test]
    fn lock_propagates_inner_error() {
        let dir = tempdir().unwrap();
        let lock = dir.path().join("lock");
        let err = with_exclusive_lock(&lock, || Err::<(), _>(StorageError::NotFound)).unwrap_err();
        assert!(matches!(err, StorageError::NotFound));
    }
}

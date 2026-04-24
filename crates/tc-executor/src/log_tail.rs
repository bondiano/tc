use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Poll interval while `follow_to_writer` is waiting for new bytes at EOF.
/// Kept short enough to feel live, long enough to not burn CPU.
pub const FOLLOW_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Read the last `max` lines of `path`.
///
/// The whole file is read into memory -- cheap for typical worker logs
/// (< ~10 MB) and keeps the implementation obvious. On any I/O error,
/// returns a single synthetic line so callers (TUI tail panes) can render
/// something instead of vanishing silently.
pub fn read_tail_lines(path: &Path, max: usize) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return vec![format!("(no log at {})", path.display())],
    };
    let mut tail: Vec<String> = content.lines().rev().take(max).map(String::from).collect();
    tail.reverse();
    tail
}

/// Tail `path` to `writer` until `interrupted` flips to `true`.
///
/// Prints every existing byte first, then polls with
/// [`FOLLOW_POLL_INTERVAL`] at EOF. Blocks the caller thread -- the
/// `interrupted` flag is the *only* exit signal. Signal-hook
/// registration is the caller's responsibility.
///
/// `path` must exist and be readable; error is propagated otherwise.
pub fn follow_to_writer<W: Write>(
    path: &Path,
    writer: &mut W,
    interrupted: &AtomicBool,
) -> io::Result<()> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    // Phase 1: drain existing content.
    'drain: loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break 'drain;
        }
        writer.write_all(line.as_bytes())?;
    }

    // Phase 2: poll for new content until interrupted.
    'follow: while !interrupted.load(Ordering::Relaxed) {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            std::thread::sleep(FOLLOW_POLL_INTERVAL);
            continue 'follow;
        }
        writer.write_all(line.as_bytes())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn read_tail_returns_last_n_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log");
        let content: String = (0..50).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&path, content).unwrap();

        let lines = read_tail_lines(&path, 5);
        assert_eq!(
            lines,
            vec!["line 45", "line 46", "line 47", "line 48", "line 49"]
        );
    }

    #[test]
    fn read_tail_under_max_returns_all_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        let lines = read_tail_lines(&path, 10);
        assert_eq!(lines, vec!["a", "b", "c"]);
    }

    #[test]
    fn read_tail_missing_file_returns_placeholder() {
        let lines = read_tail_lines(Path::new("/nonexistent/log"), 5);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("(no log at"));
    }

    #[test]
    fn follow_drains_existing_and_stops_on_interrupt() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, "hello\nworld\n").unwrap();

        let interrupted = Arc::new(AtomicBool::new(false));
        // Flip interrupt after draining completes so phase 2 exits immediately.
        let flag = Arc::clone(&interrupted);
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::Relaxed);
        });

        let mut buf: Vec<u8> = Vec::new();
        follow_to_writer(&path, &mut buf, &interrupted).unwrap();
        handle.join().unwrap();

        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("hello"));
        assert!(s.contains("world"));
    }

    #[test]
    fn follow_errors_on_missing_file() {
        let interrupted = Arc::new(AtomicBool::new(false));
        let mut buf: Vec<u8> = Vec::new();
        let err = follow_to_writer(
            Path::new("/definitely/does/not/exist"),
            &mut buf,
            &interrupted,
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}

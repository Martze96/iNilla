//! **Capture stdout and stderr** into the log ring (for the remote console and the on-device log
//! viewer) and, optionally, a session log file — without losing them where they went before.
//!
//! benilla prints a great deal of its diagnostics with `println!`/`eprintln!` (the build banner,
//! the preflight, every subsystem's one-line status), and on an iPhone nobody is attached to those
//! file descriptors. So both are pointed at a pipe; a reader thread splits the stream into lines,
//! strips terminal colour codes, pushes each line into [`inilla_core::logring::global`], appends
//! it to the log file (flushed per line, so a crash leaves everything up to its last words) and
//! passes the original bytes through to the old stderr (Xcode's console, `idevicesyslog`, a
//! terminal on desktop).

#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::fd::FromRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

static CAPTURED: AtomicBool = AtomicBool::new(false);

/// Start capturing. Idempotent; returns an error string when the pipe could not be set up (the
/// process then simply keeps its old stdout/stderr).
#[cfg(unix)]
pub fn capture(log_file: Option<&Path>) -> Result<(), String> {
    if CAPTURED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let mut file = match log_file {
        Some(p) => {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            OpenOptions::new().create(true).append(true).open(p).ok()
        }
        None => None,
    };
    // SAFETY: plain POSIX fd plumbing on descriptors this process owns.
    let (read_fd, passthrough) = unsafe {
        let passthrough = libc::dup(2);
        let mut fds = [0 as libc::c_int; 2];
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            CAPTURED.store(false, Ordering::SeqCst);
            return Err("pipe() failed".into());
        }
        libc::dup2(fds[1], 1);
        libc::dup2(fds[1], 2);
        libc::close(fds[1]);
        (fds[0], passthrough)
    };
    // SAFETY: `read_fd` is the pipe's read end, owned from here on by this `File`.
    let reader = unsafe { File::from_raw_fd(read_fd) };
    let mut pass = if passthrough >= 0 {
        // SAFETY: a fresh duplicate of the original stderr.
        Some(unsafe { File::from_raw_fd(passthrough) })
    } else {
        None
    };
    std::thread::Builder::new()
        .name("inilla-stdio".into())
        .spawn(move || {
            let mut r = BufReader::new(reader);
            let mut raw = Vec::with_capacity(512);
            loop {
                raw.clear();
                match r.read_until(b'\n', &mut raw) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if let Some(p) = pass.as_mut() {
                    let _ = p.write_all(&raw);
                }
                let line = strip_ansi(&String::from_utf8_lossy(&raw));
                inilla_core::logring::global().push(&line);
                if let Some(f) = file.as_mut() {
                    let _ = f.write_all(line.as_bytes());
                    if !line.ends_with('\n') {
                        let _ = f.write_all(b"\n");
                    }
                    let _ = f.flush();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The desktop rehearsal on Windows keeps its console: there is no phone to read it from.
#[cfg(not(unix))]
pub fn capture(_log_file: Option<&Path>) -> Result<(), String> {
    Err("stdout/stderr capture is only built for unix targets".into())
}

pub fn is_captured() -> bool {
    CAPTURED.load(Ordering::SeqCst)
}

/// Remove `ESC [ … letter` sequences (the tracing formatter's colours).
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_ansi;

    #[test]
    fn colours_are_stripped() {
        assert_eq!(
            strip_ansi("\u{1b}[2m12:00\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m x"),
            "12:00  INFO x"
        );
    }
}

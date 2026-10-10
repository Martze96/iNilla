//! The process-wide **log ring**: every line the app prints (stdout, stderr, the tracing
//! subscriber) lands here with a sequence number, so the remote console can show the last few
//! thousand lines on connect and then long-poll for new ones, and the on-device log viewer can
//! show the same thing without a PC.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

pub struct LogRing {
    inner: Mutex<Inner>,
    grew: Condvar,
}

struct Inner {
    lines: VecDeque<(u64, String)>,
    next: u64,
    cap: usize,
}

impl LogRing {
    pub fn new(cap: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                lines: VecDeque::with_capacity(cap.min(1 << 16)),
                next: 1,
                cap,
            }),
            grew: Condvar::new(),
        }
    }

    /// Append one line (a trailing newline is dropped).
    pub fn push(&self, line: &str) {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        let seq = g.next;
        g.next += 1;
        if g.lines.len() >= g.cap {
            g.lines.pop_front();
        }
        g.lines.push_back((seq, line.to_string()));
        drop(g);
        self.grew.notify_all();
    }

    /// The sequence number the next line will get.
    pub fn next_seq(&self) -> u64 {
        self.inner.lock().map(|g| g.next).unwrap_or(0)
    }

    /// Every retained line with `seq >= since`, oldest first.
    pub fn since(&self, since: u64) -> Vec<(u64, String)> {
        let Ok(g) = self.inner.lock() else {
            return Vec::new();
        };
        g.lines
            .iter()
            .filter(|(s, _)| *s >= since)
            .cloned()
            .collect()
    }

    /// The last `n` lines.
    pub fn tail(&self, n: usize) -> Vec<(u64, String)> {
        let Ok(g) = self.inner.lock() else {
            return Vec::new();
        };
        let skip = g.lines.len().saturating_sub(n);
        g.lines.iter().skip(skip).cloned().collect()
    }

    /// [`Self::since`], waiting up to `timeout` for a line to arrive when there is none yet (the
    /// console's long-poll).
    pub fn wait_since(&self, since: u64, timeout: Duration) -> Vec<(u64, String)> {
        let Ok(g) = self.inner.lock() else {
            return Vec::new();
        };
        let (g, _) = self
            .grew
            .wait_timeout_while(g, timeout, |g| g.next <= since)
            .unwrap_or_else(|e| e.into_inner());
        g.lines
            .iter()
            .filter(|(s, _)| *s >= since)
            .cloned()
            .collect()
    }
}

/// The one ring the whole process shares.
pub fn global() -> &'static LogRing {
    static RING: OnceLock<LogRing> = OnceLock::new();
    RING.get_or_init(|| LogRing::new(20_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ring_keeps_the_newest_lines_and_numbers_them() {
        let r = LogRing::new(3);
        for i in 0..5 {
            r.push(&format!("line {i}\n"));
        }
        let all = r.since(0);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], (3, "line 2".to_string()));
        assert_eq!(r.since(5), vec![(5, "line 4".to_string())]);
        assert_eq!(r.next_seq(), 6);
        assert_eq!(r.tail(1)[0].1, "line 4");
    }

    #[test]
    fn a_long_poll_wakes_on_a_new_line() {
        let r = std::sync::Arc::new(LogRing::new(10));
        let r2 = r.clone();
        let t = std::thread::spawn(move || r2.wait_since(1, Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        r.push("hello");
        assert_eq!(t.join().unwrap(), vec![(1, "hello".to_string())]);
        assert!(r.wait_since(2, Duration::from_millis(10)).is_empty());
    }
}

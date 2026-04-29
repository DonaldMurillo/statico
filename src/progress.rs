//! Lightweight progress reporter that writes to stderr.
//!
//! Uses carriage returns for in-place updates so piped output stays clean.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Reports analysis progress to stderr.
pub struct ProgressReporter {
    total: usize,
    completed: AtomicUsize,
    quiet: AtomicBool,
    phase: std::sync::Mutex<String>,
}

impl ProgressReporter {
    /// Create a new reporter for `total` items.
    pub fn new(total: usize) -> Self {
        Self {
            total,
            completed: AtomicUsize::new(0),
            quiet: AtomicBool::new(false),
            phase: std::sync::Mutex::new("Initializing".to_string()),
        }
    }

    /// Suppress all output.
    pub fn set_quiet(&self, quiet: bool) {
        self.quiet.store(quiet, Ordering::Relaxed);
    }

    /// Set the current phase label.
    pub fn set_phase(&self, phase: &str) {
        if let Ok(mut p) = self.phase.lock() {
            *p = phase.to_string();
        }
        self.render();
    }

    /// Increment the completed counter and re-render.
    pub fn inc(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
        self.render();
    }

    /// Print the final summary line.
    pub fn finish(&self) {
        if self.quiet.load(Ordering::Relaxed) {
            return;
        }
        let done = self.completed.load(Ordering::Relaxed);
        let _ = writeln!(std::io::stderr(), "\nDone. {} files analyzed.", done);
    }

    fn render(&self) {
        if self.quiet.load(Ordering::Relaxed) {
            return;
        }
        let done = self.completed.load(Ordering::Relaxed);
        let phase = self.phase.lock().map(|p| p.clone()).unwrap_or_default();
        let _ = write!(
            std::io::stderr(),
            "\r[{}/{}] {}...",
            done, self.total, phase
        );
        let _ = std::io::stderr().flush();
    }
}

/// A thread-safe wrapper around `ProgressReporter`.
pub type SharedProgress = Arc<ProgressReporter>;

/// Create a shared progress reporter.
pub fn shared_progress(total: usize) -> SharedProgress {
    Arc::new(ProgressReporter::new(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_lifecycle() {
        let p = ProgressReporter::new(10);
        p.set_quiet(true); // suppress output during tests
        assert_eq!(p.completed.load(Ordering::Relaxed), 0);
        p.inc();
        assert_eq!(p.completed.load(Ordering::Relaxed), 1);
        p.set_phase("Parsing");
        p.finish(); // quiet, so no output
    }

    #[test]
    fn test_shared_progress() {
        let p = shared_progress(5);
        p.set_quiet(true);
        p.inc();
        p.inc();
        assert_eq!(p.completed.load(Ordering::Relaxed), 2);
    }
}

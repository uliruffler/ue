use std::time::{Duration, Instant};
use crossterm::event::{KeyCode, KeyEvent};

/// Result of processing an Esc key
pub(crate) enum EscResult {
    None,         // Not an Esc handling event
    First,        // First Esc registered, waiting for second
    Double,       // Second Esc within threshold
}

/// Detector for double-Esc presses within a configured threshold
pub(crate) struct DoubleEscDetector {
    last_press: Option<Instant>,
    threshold: Duration,
}

impl DoubleEscDetector {
    pub(crate) fn new(threshold_ms: u64) -> Self {
        Self { last_press: None, threshold: Duration::from_millis(threshold_ms) }
    }

    /// Process a key event; returns EscResult if Esc logic applies
    pub(crate) fn process_key(&mut self, key: &KeyEvent) -> EscResult {
        if key.code != KeyCode::Esc || !key.modifiers.is_empty() {
            // Non-plain Esc clears pending first Esc
            self.last_press = None;
            return EscResult::None;
        }
        let now = Instant::now();
        match self.last_press {
            Some(prev) if now.duration_since(prev) <= self.threshold => {
                // Double press detected
                self.last_press = None;
                EscResult::Double
            }
            _ => {
                // First press - record time
                self.last_press = Some(now);
                EscResult::First
            }
        }
    }

    /// Returns true if a first Esc is pending and has timed out (should trigger alternate action)
    pub(crate) fn timed_out(&self) -> bool {
        match self.last_press {
            Some(prev) => Instant::now().duration_since(prev) >= self.threshold,
            None => false,
        }
    }

    /// Clear any pending first Esc state (after handling timeout)
    pub(crate) fn clear(&mut self) { self.last_press = None; }

    /// Poll timeout to pass to event::poll so we wake exactly at deadline
    pub(crate) fn remaining_timeout(&self) -> Duration {
        match self.last_press {
            Some(prev) => {
                let elapsed = Instant::now().duration_since(prev);
                if elapsed >= self.threshold { Duration::from_millis(0) } else { self.threshold - elapsed }
            }
            None => Duration::from_secs(86400), // effectively 'infinite'
        }
    }
}


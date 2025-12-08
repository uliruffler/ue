use crossterm::event::{KeyCode, KeyEvent};
use std::time::{Duration, Instant};

/// Result of processing an Esc key
pub(crate) enum EscResult {
    None,   // Not an Esc handling event
    First,  // First Esc registered, waiting for second
    Double, // Second Esc within threshold
}

/// Detector for double-Esc presses within a configured threshold
pub(crate) struct DoubleEscDetector {
    last_press: Option<Instant>,
    threshold: Duration,
}

impl DoubleEscDetector {
    pub(crate) fn new(threshold_ms: u64) -> Self {
        Self {
            last_press: None,
            threshold: Duration::from_millis(threshold_ms),
        }
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
    pub(crate) fn clear(&mut self) {
        self.last_press = None;
    }

    /// Poll timeout to pass to event::poll so we wake exactly at deadline
    pub(crate) fn remaining_timeout(&self) -> Duration {
        match self.last_press {
            Some(prev) => {
                let elapsed = Instant::now().duration_since(prev);
                if elapsed >= self.threshold {
                    Duration::from_millis(0)
                } else {
                    self.threshold - elapsed
                }
            }
            None => Duration::from_secs(86400), // effectively 'infinite'
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn create_esc_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())
    }

    fn create_char_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty())
    }

    fn create_ctrl_key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn first_esc_returns_first() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));
    }

    #[test]
    fn double_esc_within_threshold_returns_double() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        std::thread::sleep(Duration::from_millis(50));
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::Double));
    }

    #[test]
    fn double_esc_after_threshold_returns_first() {
        let mut detector = DoubleEscDetector::new(100);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        std::thread::sleep(Duration::from_millis(150));
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));
    }

    #[test]
    fn non_esc_key_clears_state() {
        let mut detector = DoubleEscDetector::new(300);
        let esc = create_esc_key();
        let other = create_char_key('a');

        let _ = detector.process_key(&esc);
        let result = detector.process_key(&other);
        assert!(matches!(result, EscResult::None));

        // Next esc should be First again
        let result = detector.process_key(&esc);
        assert!(matches!(result, EscResult::First));
    }

    #[test]
    fn esc_with_modifiers_clears_state() {
        let mut detector = DoubleEscDetector::new(300);
        let plain_esc = create_esc_key();
        let ctrl_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::CONTROL);

        let _ = detector.process_key(&plain_esc);
        let result = detector.process_key(&ctrl_esc);
        assert!(matches!(result, EscResult::None));
    }

    #[test]
    fn timed_out_returns_false_initially() {
        let detector = DoubleEscDetector::new(300);
        assert!(!detector.timed_out());
    }

    #[test]
    fn timed_out_returns_true_after_threshold() {
        let mut detector = DoubleEscDetector::new(50);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        std::thread::sleep(Duration::from_millis(100));
        assert!(detector.timed_out());
    }

    #[test]
    fn timed_out_returns_false_before_threshold() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        std::thread::sleep(Duration::from_millis(50));
        assert!(!detector.timed_out());
    }

    #[test]
    fn clear_resets_state() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        detector.clear();
        assert!(!detector.timed_out());
    }

    #[test]
    fn remaining_timeout_is_large_when_no_pending_esc() {
        let detector = DoubleEscDetector::new(300);
        let timeout = detector.remaining_timeout();
        assert!(timeout.as_secs() > 1000);
    }

    #[test]
    fn remaining_timeout_decreases_after_first_esc() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        let timeout = detector.remaining_timeout();
        assert!(timeout.as_millis() <= 300);
    }

    #[test]
    fn remaining_timeout_is_zero_after_threshold() {
        let mut detector = DoubleEscDetector::new(50);
        let key = create_esc_key();
        let _ = detector.process_key(&key);
        std::thread::sleep(Duration::from_millis(100));
        let timeout = detector.remaining_timeout();
        assert_eq!(timeout.as_millis(), 0);
    }

    #[test]
    fn triple_esc_first_two_are_double_third_is_first() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        let r1 = detector.process_key(&key);
        assert!(matches!(r1, EscResult::First));

        let r2 = detector.process_key(&key);
        assert!(matches!(r2, EscResult::Double));

        let r3 = detector.process_key(&key);
        assert!(matches!(r3, EscResult::First));
    }

    #[test]
    fn ctrl_c_returns_none() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_ctrl_key('c');
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::None));
    }

    #[test]
    fn single_esc_in_normal_mode_workflow() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // First Esc
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Timeout should occur
        std::thread::sleep(Duration::from_millis(350));
        assert!(detector.timed_out());

        // Clear after timeout
        detector.clear();
        assert!(!detector.timed_out());
    }

    #[test]
    fn double_esc_exits_from_any_mode() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // Simulate being in find/selection mode:
        // First Esc should register as First (caller handles mode exit)
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Second Esc within threshold should be Double (exits editor)
        std::thread::sleep(Duration::from_millis(50));
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::Double));
    }

    #[test]
    fn single_esc_clears_mode_but_not_editor() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // First Esc exits mode (registered as First)
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(350));
        assert!(detector.timed_out());

        // After timeout, state should be clearable without opening file selector
        detector.clear();

        // Next Esc should be a new First
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));
    }

    #[test]
    fn rapid_double_esc_in_find_mode_scenario() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // User presses Esc in find mode (exits find, registers First)
        let r1 = detector.process_key(&key);
        assert!(matches!(r1, EscResult::First));

        // User quickly presses Esc again (exits editor)
        std::thread::sleep(Duration::from_millis(100));
        let r2 = detector.process_key(&key);
        assert!(matches!(r2, EscResult::Double));
    }

    #[test]
    fn slow_double_esc_in_selection_mode_scenario() {
        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // User presses Esc in selection mode (clears selection, registers First)
        let r1 = detector.process_key(&key);
        assert!(matches!(r1, EscResult::First));

        // User waits too long
        std::thread::sleep(Duration::from_millis(350));

        // Second Esc should be treated as new First (not Double)
        let r2 = detector.process_key(&key);
        assert!(matches!(r2, EscResult::First));
    }

    #[test]
    fn other_key_after_first_esc_cancels_double_esc() {
        let mut detector = DoubleEscDetector::new(300);
        let esc = create_esc_key();
        let other = create_char_key('j');

        // First Esc in find mode
        let r1 = detector.process_key(&esc);
        assert!(matches!(r1, EscResult::First));

        // User presses another key (navigation, typing, etc.)
        let r2 = detector.process_key(&other);
        assert!(matches!(r2, EscResult::None));

        // Next Esc should be First again (not Double)
        let r3 = detector.process_key(&esc);
        assert!(matches!(r3, EscResult::First));
    }

    #[test]
    fn esc_in_normal_mode_should_trigger_file_selector_on_timeout() {
        // This test documents the expected behavior:
        // In normal edit mode (no find, no selection):
        // 1. User presses Esc -> EscResult::First
        // 2. Timeout occurs -> should open file selector
        // Note: The actual file selector opening is handled in ui.rs
        // This test just verifies the detector part works correctly

        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // First Esc in normal mode
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(350));
        assert!(detector.timed_out());

        // After clearing, detector is ready for next sequence
        detector.clear();
        assert!(!detector.timed_out());
    }

    #[test]
    fn esc_in_find_mode_should_not_trigger_file_selector() {
        // This test documents the expected behavior:
        // In find mode:
        // 1. User presses Esc -> exits find mode (handled in ui.rs)
        // 2. Timeout occurs -> should NOT open file selector
        // The detector behavior is the same, but ui.rs tracks the mode

        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // First Esc (ui.rs would exit find mode here)
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Timeout
        std::thread::sleep(Duration::from_millis(350));
        assert!(detector.timed_out());

        // ui.rs should clear without opening file selector
        detector.clear();
    }

    #[test]
    fn esc_with_selection_should_not_trigger_file_selector() {
        // This test documents the expected behavior:
        // With text selection:
        // 1. User presses Esc -> clears selection (handled in ui.rs)
        // 2. Timeout occurs -> should NOT open file selector

        let mut detector = DoubleEscDetector::new(300);
        let key = create_esc_key();

        // First Esc (ui.rs would clear selection here)
        let result = detector.process_key(&key);
        assert!(matches!(result, EscResult::First));

        // Timeout
        std::thread::sleep(Duration::from_millis(350));
        assert!(detector.timed_out());

        // ui.rs should clear without opening file selector
        detector.clear();
    }
}

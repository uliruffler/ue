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
}

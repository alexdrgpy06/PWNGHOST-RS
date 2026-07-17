use pwncore::epoch::Epoch;

#[derive(Debug, Clone)]
pub struct EpochTracker {
    pub current: Epoch,
    pub total_epochs: u64,
}

impl EpochTracker {
    pub fn new() -> Self {
        Self {
            current: Epoch::new(0),
            total_epochs: 0,
        }
    }

    pub fn advance(&mut self) {
        self.total_epochs += 1;
        let mut next = Epoch::new(self.total_epochs);
        next.inactive_epochs = self.current.inactive_epochs;
        next.active_epochs = self.current.active_epochs;
        next.bored_epochs = self.current.bored_epochs;
        next.sad_epochs = self.current.sad_epochs;
        next.blind_epochs = self.current.blind_epochs;
        self.current = next;
    }

    pub fn finalize_current(&mut self, personality: &pwncore::personality::Personality) {
        let mut finalized = self.current.clone();
        finalized.finalize(personality);
        self.current.inactive_epochs = finalized.inactive_epochs;
        self.current.active_epochs = finalized.active_epochs;
        self.current.bored_epochs = finalized.bored_epochs;
        self.current.sad_epochs = finalized.sad_epochs;
    }
}

impl Default for EpochTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwncore::personality::Personality;

    #[test]
    fn test_epoch_tracking() {
        let mut tracker = EpochTracker::new();
        assert_eq!(tracker.total_epochs, 0);
        tracker.advance();
        assert_eq!(tracker.total_epochs, 1);
        assert_eq!(tracker.current.number, 1);
    }

    #[test]
    fn test_epoch_finalize_preserves_counters() {
        let mut tracker = EpochTracker::new();
        tracker.advance();
        // Mark as inactive
        assert!(!tracker.current.any_activity);
        let p = Personality::default();
        tracker.finalize_current(&p);
        assert_eq!(tracker.current.inactive_epochs, 1);
    }
}

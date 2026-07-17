use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub total_epochs: u64,
    pub last_mood: String,
}

pub fn save_state(path: &str, state: &RecoveryState) -> anyhow::Result<()> {
    let json = serde_json::to_string(state)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_state(path: &str) -> anyhow::Result<RecoveryState> {
    let json = std::fs::read_to_string(path)?;
    let state: RecoveryState = serde_json::from_str(&json)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recovery.json");
        let state = RecoveryState {
            total_epochs: 42,
            last_mood: "happy".into(),
        };
        save_state(path.to_str().unwrap(), &state).unwrap();
        let loaded = load_state(path.to_str().unwrap()).unwrap();
        assert_eq!(loaded.total_epochs, 42);
    }
}

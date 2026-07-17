#[derive(Debug, Clone)]
pub struct Capture {
    pub filename: String,
    pub valid: bool,
}

pub fn validate_handshake(data: &[u8]) -> bool {
    !data.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty() {
        assert!(!validate_handshake(b""));
    }

    #[test]
    fn test_validate_non_empty() {
        assert!(validate_handshake(b"some data"));
    }
}

pub fn connect_pan(_address: &str) -> anyhow::Result<()> {
    Ok(())
}

pub fn disconnect_pan() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_disconnect() {
        assert!(connect_pan("00:11:22:33:44:55").is_ok());
        assert!(disconnect_pan().is_ok());
    }
}

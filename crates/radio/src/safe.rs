pub fn connect_known_wifi(_ssid: &str, _password: &str) -> anyhow::Result<()> {
    Ok(())
}

pub fn disconnect_wifi() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_disconnect_wifi() {
        assert!(connect_known_wifi("test", "pass").is_ok());
        assert!(disconnect_wifi().is_ok());
    }
}

pub fn set_monitor_mode(_iface: &str, _up: bool) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_monitor_mode() {
        let result = set_monitor_mode("wlan0", true);
        assert!(result.is_ok());
    }
}

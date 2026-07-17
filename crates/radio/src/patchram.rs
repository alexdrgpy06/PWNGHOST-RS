pub fn load_patchram(_chip: &str) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_patchram() {
        let result = load_patchram("bcm43436b0");
        assert!(result.is_ok());
    }
}

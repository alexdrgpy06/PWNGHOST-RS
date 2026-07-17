use crate::schema::PwnagotchiConfig;

pub fn migrate_legacy(legacy_toml: &str) -> anyhow::Result<PwnagotchiConfig> {
    let config: PwnagotchiConfig = toml::from_str(legacy_toml)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrate_minimal() {
        let toml_str = r#"
[main]
name = "pwnagotchi"
        "#;
        let result = migrate_legacy(toml_str);
        assert!(result.is_ok());
    }
}

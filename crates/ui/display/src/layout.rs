pub fn render_face(_face: &str) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_face() {
        assert!(render_face("( ◕‿◕ )").is_ok());
    }
}

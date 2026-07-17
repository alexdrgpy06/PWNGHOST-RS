pub const DEJAVU_FONT: &[u8] = b"";

#[cfg(test)]
mod tests {
    #[test]
    fn test_font_bytes() {
        assert!(super::DEJAVU_FONT.is_empty());
    }
}

//! Font handling for e-ink display

use embedded_graphics::mono_font::MonoFont;
use std::collections::HashMap;

/// Font registry
pub struct FontRegistry {
    fonts: HashMap<String, &'static MonoFont<'static>>,
}

impl FontRegistry {
    pub fn new() -> Self {
        let mut registry = Self { fonts: HashMap::new() };
        registry.register_builtin_fonts();
        registry
    }

    fn register_builtin_fonts(&mut self) {
        // In real implementation, these would be embedded fonts
        // self.fonts.insert("dejavu".to_string(), DEJAVU_SANS_MONO_12);
        // self.fonts.insert("dejavu_bold".to_string(), DEJAVU_SANS_MONO_BOLD_12);
        // self.fonts.insert("kaomoji".to_string(), KAOMOJI_FONT);
    }

    pub fn get(&self, name: &str) -> Option<&'static MonoFont<'static>> {
        self.fonts.get(name).copied()
    }

    pub fn register(&mut self, name: String, font: &'static MonoFont<'static>) {
        self.fonts.insert(name, font);
    }
}

/// Get default font for UI
pub fn default_font() -> Option<&'static MonoFont<'static>> {
    None // Would return actual font in real implementation
}

/// Get bold font for headers
pub fn bold_font() -> Option<&'static MonoFont<'static>> {
    None
}

/// Get small font for status
pub fn small_font() -> Option<&'static MonoFont<'static>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_registry() {
        let registry = FontRegistry::new();
        // Just verify it creates without error
    }
}
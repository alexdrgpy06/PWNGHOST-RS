//! Font handling for the display.
//!
//! Wraps `embedded-graphics` built-in monospace ASCII fonts and exposes a
//! small registry so callers can look them up by name.

use embedded_graphics::mono_font::{ascii, MonoFont};
use std::collections::HashMap;

/// Font registry mapping names to built-in monospace fonts.
pub struct FontRegistry {
    fonts: HashMap<String, &'static MonoFont<'static>>,
}

impl FontRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            fonts: HashMap::new(),
        };
        registry.register_builtin_fonts();
        registry
    }

    fn register_builtin_fonts(&mut self) {
        self.fonts.insert("small".to_string(), &ascii::FONT_6X10);
        self.fonts.insert("regular".to_string(), &ascii::FONT_8X13);
        self.fonts
            .insert("bold".to_string(), &ascii::FONT_9X15_BOLD);
        self.fonts.insert("large".to_string(), &ascii::FONT_10X20);
    }

    pub fn get(&self, name: &str) -> Option<&'static MonoFont<'static>> {
        self.fonts.get(name).copied()
    }

    pub fn register(&mut self, name: String, font: &'static MonoFont<'static>) {
        self.fonts.insert(name, font);
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Default font for body text.
pub fn default_font() -> &'static MonoFont<'static> {
    &ascii::FONT_6X10
}

/// Bold font for headers.
pub fn bold_font() -> &'static MonoFont<'static> {
    &ascii::FONT_9X15_BOLD
}

/// Small font for the status bar.
pub fn small_font() -> &'static MonoFont<'static> {
    &ascii::FONT_6X10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_registry() {
        let registry = FontRegistry::new();
        assert!(!registry.is_empty());
        assert!(registry.get("small").is_some());
        assert!(registry.get("bold").is_some());
        assert!(registry.get("nonexistent").is_none());
    }
}

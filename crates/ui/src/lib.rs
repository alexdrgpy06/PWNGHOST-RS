//! Umbrella crate that re-exports the PWNGHOST UI sub-crates so callers can
//! use `ui::display` and `ui::web`.

pub use ui_display as display;
pub use ui_web as web;

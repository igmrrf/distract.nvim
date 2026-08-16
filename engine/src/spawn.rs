//! Where a spawn is placed.
//!
//! `World::spawn` grew a positional argument per placement idea until the
//! signature said nothing about which `Option<f32>` was which. One options
//! struct names them and lets the next placement idea arrive without touching
//! a single call site.

/// Placement for one spawn. Every field is optional; the world supplies its own
/// default for anything left unset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpawnOptions {
    /// Horizontal position in overlay pixels.
    pub x: Option<f32>,
    /// Vertical position in overlay pixels.
    pub y: Option<f32>,
    /// Whether the entity starts facing left.
    pub flip_x: Option<bool>,
}

impl SpawnOptions {
    /// A spawn at an explicit position, in overlay pixels.
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            x: Some(x),
            y: Some(y),
            flip_x: None,
        }
    }
}

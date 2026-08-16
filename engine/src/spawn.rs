//! Where a spawn is placed.
//!
//! `World::spawn` grew a positional argument per placement idea until the
//! signature said nothing about which `Option<f32>` was which. One options
//! struct names them and lets the next placement idea arrive without touching
//! a single call site.

/// Where an entity starts when the spawn gives no explicit coordinates.
///
/// Neovim resolves its own `auto` anchor before sending the command, because
/// deciding it needs the manifest's locomotion class, so only concrete anchors
/// arrive here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// Standing on the floor Neovim last measured.
    Bottom,
    /// The top of the viewport.
    Top,
    /// The middle of the viewport, which is where everything spawned before
    /// floors existed.
    Free,
}

impl Anchor {
    /// Parses an anchor name received over IPC.
    ///
    /// Returns `None` for an unrecognised name, so a spawn still happens at the
    /// world's own default rather than failing outright.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "bottom" => Some(Self::Bottom),
            "top" => Some(Self::Top),
            "free" => Some(Self::Free),
            _ => None,
        }
    }
}

/// Placement for one spawn. Every field is optional; the world supplies its own
/// default for anything left unset.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpawnOptions {
    /// Horizontal position in overlay pixels.
    pub x: Option<f32>,
    /// Vertical position in overlay pixels.
    pub y: Option<f32>,
    /// Depth. Dimensionless, and both the draw order and the parallax input.
    pub z: Option<f32>,
    /// How far motion and size are damped by depth. Neovim computes it from
    /// `z` and its own `position.parallax` config, because the config is the
    /// editor's and the backend it targets may not be able to scale at all.
    pub parallax: Option<f32>,
    /// Where to start when no explicit position is given.
    pub anchor: Option<Anchor>,
    /// Whether the entity starts facing left.
    pub flip_x: Option<bool>,
}

impl SpawnOptions {
    /// A spawn at an explicit position, in overlay pixels.
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            x: Some(x),
            y: Some(y),
            ..Self::default()
        }
    }
}

/// Everything an entity needs at birth beyond its identity.
#[derive(Debug, Clone)]
pub struct EntitySeed {
    pub initial_state: String,
    /// Horizontal position in overlay pixels.
    pub x: f32,
    /// Vertical position in overlay pixels.
    pub y: f32,
    pub flip_x: bool,
    /// Draw order, back to front.
    pub z_index: i32,
    /// Depth, dimensionless.
    pub z: f32,
    /// Motion and size multiplier derived from `z`. Exactly 1 when parallax is
    /// off, which is the default.
    pub parallax: f32,
}

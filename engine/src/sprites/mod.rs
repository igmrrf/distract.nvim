//! Procedurally drawn sprite sets, ported from `lua/distract/sprites/`.
//!
//! Both backends draw the same art: the terminal renderer generates these in
//! Lua and the overlay generates them here, from the same pose curves and the
//! same shading model. Previously the overlay fell back to a separate
//! four-frame set, so manifest frame indices resolved to unrelated art and
//! states like `idle` and `sleep` could land on the same picture.

pub mod cat;
pub mod crab;
pub mod sun;

use std::collections::HashMap;
use std::sync::OnceLock;

use image::RgbaImage;

use crate::sprite_gen::Canvas;

/// A complete set of frames for one asset, plus the mapping from state name to
/// the frame indices that state animates through.
#[derive(Debug, Clone)]
pub struct SpriteSet {
    pub frames: Vec<RgbaImage>,
    /// State name -> 0-based indices into `frames`.
    pub layout: HashMap<String, Vec<usize>>,
    pub width: u32,
    pub height: u32,
}

impl SpriteSet {
    fn new(width: u32, height: u32) -> Self {
        Self {
            frames: Vec::new(),
            layout: HashMap::new(),
            width,
            height,
        }
    }

    /// Appends a state's frames and records its 0-based index range.
    fn add<P>(&mut self, state: &str, poses: Vec<P>, draw: impl Fn(&P) -> Canvas) {
        let start = self.frames.len();
        for pose in &poses {
            self.frames.push(draw(pose).to_image());
        }
        self.layout
            .insert(state.to_string(), (start..start + poses.len()).collect());
    }

    /// Frame indices for a state, or `[0]` when the state is unknown so a
    /// manifest referencing a missing state still draws something.
    pub fn frames_for(&self, state: &str) -> Vec<usize> {
        self.layout.get(state).cloned().unwrap_or_else(|| vec![0])
    }
}

/// Generation is not free, so each asset is drawn once on first use and cached
/// for the process lifetime.
pub fn get(name: &str) -> &'static SpriteSet {
    match name {
        "crab" => crab_set(),
        "sun" => sun_set(),
        _ => cat_set(),
    }
}

/// Whether an asset name has a built-in procedural sprite set.
pub fn is_builtin(name: &str) -> bool {
    matches!(name, "cat" | "crab" | "sun")
}

pub fn cat_set() -> &'static SpriteSet {
    static SET: OnceLock<SpriteSet> = OnceLock::new();
    SET.get_or_init(cat::build)
}

pub fn crab_set() -> &'static SpriteSet {
    static SET: OnceLock<SpriteSet> = OnceLock::new();
    SET.get_or_init(crab::build)
}

pub fn sun_set() -> &'static SpriteSet {
    static SET: OnceLock<SpriteSet> = OnceLock::new();
    SET.get_or_init(sun::build)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_produces_more_than_the_old_four_frames() {
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            assert!(
                set.frames.len() > 4,
                "{} has only {} frames",
                name,
                set.frames.len()
            );
        }
    }

    #[test]
    fn every_frame_matches_the_declared_canvas_size() {
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            for (i, frame) in set.frames.iter().enumerate() {
                assert_eq!(
                    frame.dimensions(),
                    (set.width, set.height),
                    "{} frame {} has the wrong size",
                    name,
                    i
                );
            }
        }
    }

    #[test]
    fn every_layout_index_is_in_range() {
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            for (state, indices) in &set.layout {
                assert!(!indices.is_empty(), "{}/{} has no frames", name, state);
                for &i in indices {
                    assert!(
                        i < set.frames.len(),
                        "{}/{} references frame {} of {}",
                        name,
                        state,
                        i,
                        set.frames.len()
                    );
                }
            }
        }
    }

    #[test]
    fn every_frame_draws_something() {
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            for (i, frame) in set.frames.iter().enumerate() {
                let opaque = frame.pixels().filter(|p| p[3] > 0).count();
                assert!(opaque > 8, "{} frame {} is nearly empty", name, i);
            }
        }
    }

    #[test]
    fn states_do_not_share_frame_indices() {
        // Overlapping ranges would mean two states drawing the same art, which
        // is the exact failure the ported set exists to remove.
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            let mut seen = std::collections::HashSet::new();
            for indices in set.layout.values() {
                for &i in indices {
                    assert!(seen.insert(i), "{} reuses frame {}", name, i);
                }
            }
        }
    }

    #[test]
    fn neighbouring_frames_within_a_state_differ() {
        // Two identical frames in a row read as a stutter in the animation.
        for name in ["cat", "crab", "sun"] {
            let set = get(name);
            for (state, indices) in &set.layout {
                for pair in indices.windows(2) {
                    assert_ne!(
                        set.frames[pair[0]], set.frames[pair[1]],
                        "{}/{} frames {} and {} are identical",
                        name, state, pair[0], pair[1]
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_state_falls_back_to_the_first_frame() {
        assert_eq!(get("cat").frames_for("no_such_state"), vec![0]);
    }
}

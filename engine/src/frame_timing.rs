//! How long one animation frame is shown for.
//!
//! Its own module because the precedence is a contract rather than a detail: a
//! manifest `fps` wins, imported art without one is timed by the delays stored
//! in the file, and anything else falls back. `lua/distract/engine.lua` applies
//! the same order, which is what makes a GIF asset run at one speed on both
//! backends.

use crate::asset::LoadedAsset;
use crate::manifest::AnimationConfig;

/// What one animation frame is shown for when nothing declares a rate.
const FALLBACK_FRAME_SECONDS: f32 = 0.1;

const MS_PER_SECOND: f32 = 1000.0;

/// How long the entity's current animation frame is shown for, in seconds.
///
/// A manifest `fps` wins. Imported art whose state declares none is timed by
/// the delays stored in the file, which is the only rate an animation authored
/// elsewhere carries; `lua/distract/engine.lua` applies the same precedence, so
/// a GIF asset runs at one speed on both backends.
pub fn frame_duration_seconds(
    anim: &AnimationConfig,
    frame_idx: usize,
    asset: &LoadedAsset,
) -> f32 {
    if anim.fps > 0.0 {
        return 1.0 / anim.fps;
    }

    let delay_ms = anim
        .frames
        .get(frame_idx)
        .and_then(|sheet_index| asset.frame_delays_ms.get(*sheet_index))
        .copied()
        .unwrap_or(0);

    if delay_ms > 0 {
        delay_ms as f32 / MS_PER_SECOND
    } else {
        FALLBACK_FRAME_SECONDS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest;
    use crate::manifest::AssetManifest;

    fn timed_asset(delays_ms: Vec<u32>) -> crate::asset::LoadedAsset {
        let manifest = AssetManifest::default_cat();
        let mut asset = crate::asset::AssetManager::load_asset(manifest, 0)
            .expect("the built-in cat must load for the timing tests");
        asset.frame_delays_ms = delays_ms;
        asset
    }

    fn animation(fps: f32, frames: Vec<usize>) -> manifest::AnimationConfig {
        manifest::AnimationConfig {
            frames,
            fps,
            loop_anim: true,
            flip_x: false,
        }
    }

    #[test]
    fn a_declared_fps_outranks_the_files_own_timing() {
        let asset = timed_asset(vec![500, 500]);
        let anim = animation(20.0, vec![0, 1]);

        assert_eq!(frame_duration_seconds(&anim, 0, &asset), 0.05);
    }

    #[test]
    fn imported_art_without_an_fps_runs_at_the_files_delay() {
        let asset = timed_asset(vec![200, 80]);
        let anim = animation(0.0, vec![0, 1]);

        assert_eq!(frame_duration_seconds(&anim, 0, &asset), 0.2);
        assert_eq!(frame_duration_seconds(&anim, 1, &asset), 0.08);
    }

    #[test]
    fn art_with_neither_an_fps_nor_a_delay_falls_back() {
        let asset = timed_asset(Vec::new());
        let anim = animation(0.0, vec![0]);

        assert_eq!(
            frame_duration_seconds(&anim, 0, &asset),
            FALLBACK_FRAME_SECONDS
        );
    }
}

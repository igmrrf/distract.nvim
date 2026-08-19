use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use image::RgbaImage;

use crate::asset_decode::{declared_frame_size, load_gif, load_spritesheet};
use crate::manifest::AssetManifest;
use crate::sprites;

/// Holds loaded and sliced frames for an asset.
///
/// Frames are stored once, in their authored orientation. Mirroring happens at
/// blit time via [`crate::compositor::Compositor::blend_sprite`] and at draw
/// time in the GPU path, rather than keeping a second flipped copy of every
/// frame alive for the process lifetime.
#[derive(Debug, Clone)]
pub struct LoadedAsset {
    pub name: String,
    pub manifest: AssetManifest,
    pub frames: Vec<RgbaImage>,
    pub frame_w: u32,
    pub frame_h: u32,
    /// How long each frame is shown for, when the source file says so.
    ///
    /// Only GIFs carry timing of their own; everything else leaves this empty
    /// and is animated at whatever rate its manifest declares. `ecs.rs` applies
    /// the same precedence `lua/distract/engine.lua` does, so one manifest runs
    /// at one speed on both backends.
    pub frame_delays_ms: Vec<u32>,
    /// Hash of the manifest this asset was built from, so a repeated
    /// registration of the same manifest can be skipped.
    pub manifest_hash: u64,
}

pub struct AssetManager {
    assets: HashMap<String, LoadedAsset>,
    generation: u64,
}

impl AssetManager {
    pub fn new() -> Self {
        let mut mgr = Self {
            assets: HashMap::new(),
            generation: 0,
        };
        // Register default procedural assets. These are built from the shared
        // sprite generator and cannot fail.
        for manifest in [
            AssetManifest::default_cat(),
            AssetManifest::default_crab(),
            AssetManifest::default_sun(),
        ] {
            if let Err(err) = mgr.register_manifest(manifest) {
                // A built-in failing to load is a bug, not a user error.
                log::error!("built-in asset failed to load: {}", err);
            }
        }
        mgr
    }
}

impl Default for AssetManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable hash of a manifest.
///
/// Serialising through `serde_json::Value` first is deliberate: the manifest's
/// `HashMap` fields have a per-instance iteration order, so hashing the direct
/// JSON encoding would produce a different hash for two identical manifests.
/// `Value`'s object type is ordered, so the encoding is canonical.
fn manifest_hash(manifest: &AssetManifest) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    match serde_json::to_value(manifest) {
        Ok(value) => value.to_string().hash(&mut hasher),
        // An unserialisable manifest can never match a previous hash, which is
        // the safe direction: it reloads rather than reusing stale frames.
        Err(_) => (
            manifest.name.as_str(),
            std::ptr::addr_of!(*manifest) as usize,
        )
            .hash(&mut hasher),
    }
    hasher.finish()
}

impl AssetManager {
    pub fn get(&self, name: &str) -> Option<&LoadedAsset> {
        self.assets.get(name)
    }

    /// Every loaded asset, in unspecified order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &LoadedAsset)> {
        self.assets.iter()
    }

    /// Bumped whenever frames change. The GPU atlas rebuilds only when this
    /// moves, so a spawn that re-sends an unchanged manifest costs nothing.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Registers a manifest, loading its frames.
    ///
    /// Neovim resends the manifest on every spawn, so an unchanged manifest is
    /// a no-op: without this, spawning ten cats decodes the same spritesheet
    /// ten times. Returns `true` when frames were actually (re)loaded.
    pub fn register_manifest(&mut self, manifest: AssetManifest) -> Result<bool, String> {
        let name = manifest.name.clone();
        // Checked here rather than per frame: a manifest that cannot work is
        // worth one message when it arrives, not thirty a second forever.
        manifest.validate_capabilities()?;
        let hash = manifest_hash(&manifest);

        if let Some(existing) = self.assets.get(&name) {
            if existing.manifest_hash == hash {
                return Ok(false);
            }
        }

        let loaded = Self::load_asset(manifest, hash)?;
        self.assets.insert(name, loaded);
        self.generation += 1;
        Ok(true)
    }

    pub fn load_asset(manifest: AssetManifest, hash: u64) -> Result<LoadedAsset, String> {
        let name = manifest.name.clone();
        let mut frames = Vec::new();
        let mut frame_delays_ms = Vec::new();
        let mut frame_w = 32;
        let mut frame_h = 32;

        if let Some(ref path_str) = manifest.spritesheet.path {
            let path = Path::new(path_str);
            if path.exists() {
                let (loaded, w, h) = if path_str.to_lowercase().ends_with(".gif") {
                    let decoded = load_gif(path, declared_frame_size(&manifest))?;
                    frame_delays_ms = decoded.delays_ms;
                    (decoded.frames, decoded.width, decoded.height)
                } else {
                    load_spritesheet(path, &manifest)?
                };
                frames = loaded;
                frame_w = w;
                frame_h = h;
            } else if manifest.asset_type != "procedural" && !sprites::is_builtin(&name) {
                return Err(format!("Spritesheet file not found at path '{}'", path_str));
            }
        }

        // If no frames were loaded from file, draw the built-in procedural set.
        if frames.is_empty() {
            let set = sprites::get(&name);
            frames = set.frames.clone();
            frame_w = set.width;
            frame_h = set.height;
            frame_delays_ms.clear();
        }

        Ok(LoadedAsset {
            name,
            manifest,
            frames,
            frame_w,
            frame_h,
            frame_delays_ms,
            manifest_hash: hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(m: &AssetManifest) -> u64 {
        manifest_hash(m)
    }

    #[test]
    fn identical_manifests_hash_identically() {
        // Two separately built manifests have independently seeded HashMaps, so
        // this is exactly the case a naive JSON hash would get wrong.
        let a = AssetManifest::default_cat();
        let b = AssetManifest::default_cat();
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn changed_manifests_hash_differently() {
        let a = AssetManifest::default_cat();
        let mut b = AssetManifest::default_cat();
        b.initial_state = "sleep".to_string();
        assert_ne!(hash_of(&a), hash_of(&b));
    }

    #[test]
    fn re_registering_an_unchanged_manifest_is_a_no_op() {
        let mut mgr = AssetManager::new();
        // The built-in cat is already registered from the same manifest.
        assert_eq!(
            mgr.register_manifest(AssetManifest::default_cat()),
            Ok(false)
        );

        let mut changed = AssetManifest::default_cat();
        changed.initial_state = "sleep".to_string();
        assert_eq!(mgr.register_manifest(changed), Ok(true));
    }
}

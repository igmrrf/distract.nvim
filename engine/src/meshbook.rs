//! Voxel meshes for every frame of every loaded asset, in one buffer pair.
//!
//! The 3D counterpart of `atlas.rs`, and for the same reason: uploading one
//! vertex and one index buffer when the asset set changes, then drawing ranges
//! out of them, keeps per-frame traffic to the instance list. Rebuilding a mesh
//! per frame drawn would decode geometry at 60 FPS to draw the same pet.

use std::collections::HashMap;

use crate::asset::AssetManager;
use crate::voxel::{self, MeshVertex, VoxelOptions};

/// Ceiling on the geometry one book holds.
///
/// A 48-wide pet frame is on the order of ten thousand vertices, so this is room
/// for hundreds of frames. It exists because the asset set is user-supplied: an
/// import of a hundred 200-pixel-wide states must degrade visibly rather than
/// exhaust memory.
pub const MAX_VERTICES: usize = 4_000_000;

/// Where one frame's mesh lives in the shared buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshRange {
    pub first_index: u32,
    pub index_count: u32,
    /// The voxel grid this frame was built on: `[cols, rows, depth]`.
    pub extent: [u32; 3],
}

#[derive(Debug, Default)]
pub struct MeshBook {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    /// Asset name -> per-frame range, indexed the same way as
    /// `LoadedAsset::frames` and `Atlas::rects`.
    ranges: HashMap<String, Vec<MeshRange>>,
    /// Frames left out because the book was full. Reported rather than hidden: a
    /// pet that silently stopped having a model is indistinguishable from one
    /// whose art failed to load.
    pub skipped_frames: usize,
    pub options: VoxelOptions,
}

impl MeshBook {
    /// Extrudes every frame the manager currently holds.
    pub fn build(manager: &AssetManager, options: VoxelOptions) -> Self {
        let mut book = Self {
            options,
            ..Default::default()
        };

        let mut names: Vec<&String> = manager.iter().map(|(name, _)| name).collect();
        names.sort();

        for name in names {
            let Some(asset) = manager.get(name) else {
                continue;
            };
            let mut frame_ranges = Vec::with_capacity(asset.frames.len());
            for frame in &asset.frames {
                frame_ranges.push(book.push_frame(frame, options));
            }
            book.ranges.insert(name.clone(), frame_ranges);
        }

        book
    }

    fn push_frame(&mut self, frame: &image::RgbaImage, options: VoxelOptions) -> MeshRange {
        let mesh = voxel::build(frame, options);
        let extent = mesh.extent;
        if self.vertices.len() + mesh.vertices.len() > MAX_VERTICES {
            self.skipped_frames += 1;
            return MeshRange {
                first_index: 0,
                index_count: 0,
                extent,
            };
        }

        let base_vertex = self.vertices.len() as u32;
        let first_index = self.indices.len() as u32;
        self.vertices.extend_from_slice(&mesh.vertices);
        self.indices
            .extend(mesh.indices.iter().map(|index| index + base_vertex));

        MeshRange {
            first_index,
            index_count: mesh.indices.len() as u32,
            extent,
        }
    }

    /// The range for one frame, wrapping the frame index the way the atlas does
    /// so a manifest pointing past the end draws a frame rather than nothing.
    pub fn range(&self, asset: &str, frame: usize) -> Option<MeshRange> {
        let frames = self.ranges.get(asset)?;
        if frames.is_empty() {
            return None;
        }
        Some(frames[frame % frames.len()])
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn asset_count(&self) -> usize {
        self.ranges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voxel::{DEFAULT_DEPTH, DEFAULT_MAX_WIDTH};

    fn manager() -> AssetManager {
        AssetManager::new()
    }

    #[test]
    fn every_frame_of_every_builtin_gets_its_own_range() {
        let manager = manager();
        let book = MeshBook::build(&manager, VoxelOptions::default());

        assert_eq!(book.asset_count(), manager.iter().count());
        for (name, asset) in manager.iter() {
            for frame in 0..asset.frames.len() {
                assert!(
                    book.range(name, frame).is_some(),
                    "{} frame {}",
                    name,
                    frame
                );
            }
        }
    }

    #[test]
    fn the_ranges_partition_the_index_buffer_without_overlapping() {
        let manager = manager();
        let book = MeshBook::build(&manager, VoxelOptions::default());

        let mut spans: Vec<(u32, u32)> = Vec::new();
        for (name, asset) in manager.iter() {
            for frame in 0..asset.frames.len() {
                let range = book.range(name, frame).expect("a range per frame");
                spans.push((range.first_index, range.first_index + range.index_count));
            }
        }
        spans.sort();

        let mut previous_end = 0;
        for (first, end) in spans {
            assert_eq!(
                first, previous_end,
                "a gap or an overlap in the index buffer"
            );
            previous_end = end;
        }
        assert_eq!(previous_end as usize, book.indices.len());
    }

    #[test]
    fn indices_address_the_shared_vertex_buffer_directly() {
        let book = MeshBook::build(&manager(), VoxelOptions::default());
        let highest = book.indices.iter().copied().max().expect("geometry");
        assert_eq!(highest as usize, book.vertices.len() - 1);
    }

    #[test]
    fn a_frame_index_past_the_end_wraps_rather_than_drawing_nothing() {
        let manager = manager();
        let book = MeshBook::build(&manager, VoxelOptions::default());
        let frame_count = manager.get("cat").expect("the built-in cat").frames.len();
        assert_eq!(book.range("cat", frame_count + 1), book.range("cat", 1));
    }

    #[test]
    fn an_unknown_asset_has_no_range() {
        let book = MeshBook::build(&manager(), VoxelOptions::default());
        assert!(book.range("no_such_pet", 0).is_none());
    }

    #[test]
    fn nothing_is_skipped_at_the_built_in_scale() {
        let book = MeshBook::build(&manager(), VoxelOptions::default());
        assert_eq!(book.skipped_frames, 0);
        assert!(!book.is_empty());
    }

    #[test]
    fn the_extent_is_carried_so_the_instance_transform_can_reach_pixels() {
        let manager = manager();
        let book = MeshBook::build(&manager, VoxelOptions::default());
        let cat = manager.get("cat").expect("the built-in cat");
        let range = book.range("cat", 0).expect("a range");

        assert_eq!(range.extent[0], cat.frame_w.min(DEFAULT_MAX_WIDTH));
        assert_eq!(range.extent[2], DEFAULT_DEPTH);
    }

    #[test]
    fn a_thicker_slab_produces_the_same_ranges_at_a_different_depth() {
        let thin = MeshBook::build(
            &manager(),
            VoxelOptions {
                max_width: 48,
                depth: 2,
            },
        );
        let thick = MeshBook::build(
            &manager(),
            VoxelOptions {
                max_width: 48,
                depth: 8,
            },
        );

        assert_eq!(
            thin.indices.len(),
            thick.indices.len(),
            "depth is not a face count"
        );
        assert_eq!(thin.range("cat", 0).map(|range| range.extent[2]), Some(2));
        assert_eq!(thick.range("cat", 0).map(|range| range.extent[2]), Some(8));
    }
}

//! The built-in assets' manifests.
//!
//! One module per asset, each a static state table and nothing else. They live
//! here rather than in `manifest.rs` because three long data tables are what put
//! that module past 1,400 lines, and §5 of the standards exempts pure data tables
//! in dedicated files from the size cap.
//!
//! `AssetManifest::default_cat` and its siblings are still the entry points, so
//! nothing that reads a built-in manifest had to change.

pub mod cat;
pub mod crab;
pub mod sun;

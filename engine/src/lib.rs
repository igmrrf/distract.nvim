#![allow(unexpected_cfgs)]

pub mod asset;
pub mod asset_decode;
pub mod atlas;
pub mod bounds;
pub mod camera;
pub mod commands;
pub mod compositor;
pub mod ecs;
pub mod entity;
pub mod entity_step;
pub mod frame_timing;
pub mod gpu;
pub mod gpu3d;
pub mod gpu_bindings;
pub mod gpu_setup;
pub mod ipc;
pub mod journal;
pub mod manifest;
pub mod manifests;
pub mod mesh_draw;
pub mod meshbook;
pub mod obstacles;
pub mod overlay_placement;
pub mod path;
pub mod physics_config;
pub mod platform;
pub mod render;
pub mod response;
pub mod shading;
pub mod spawn;
pub mod sprite_draw;
pub mod sprite_gen;
pub mod sprites;
pub mod spritesheet;
pub mod state_machine;
pub mod subscription;
pub mod voxel;
pub mod wrap;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;
pub mod world_spawn;

#![allow(unexpected_cfgs)]

pub mod asset;
pub mod atlas;
pub mod bounds;
pub mod commands;
pub mod compositor;
pub mod ecs;
pub mod gpu;
pub mod ipc;
pub mod journal;
pub mod manifest;
pub mod obstacles;
pub mod overlay_placement;
pub mod platform;
pub mod response;
pub mod spawn;
pub mod sprite_gen;
pub mod sprites;
pub mod subscription;
pub mod wrap;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

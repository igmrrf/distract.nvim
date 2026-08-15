#![allow(unexpected_cfgs)]

pub mod asset;
pub mod atlas;
pub mod compositor;
pub mod ecs;
pub mod gpu;
pub mod ipc;
pub mod manifest;
pub mod platform;
pub mod sprite_gen;
pub mod sprites;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

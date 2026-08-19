//! The built-in crab's manifest.
//!
//! A static state table and nothing else. §5 of the standards exempts pure data
//! tables in dedicated files from the size cap, and keeping all three of these in
//! `manifest.rs` is what put that module past 1,400 lines.

use std::collections::HashMap;

use crate::manifest::{
    AnimationConfig, AssetManifest, Capabilities, CustomActionConfig, GROUNDED, PhysicsConfig,
    SpritesheetConfig, StateDefinition, TransitionConfig, WrapMode,
};
use crate::sprites;

/// Built-in Crab manifest with sideways walk, claw clipping, and burrowing.
pub fn manifest() -> AssetManifest {
    let mut states = HashMap::new();

    // Idle / standing
    let mut idle_trans = HashMap::new();
    idle_trans.insert("typing".to_string(), "walk_fast".to_string());
    idle_trans.insert("moving".to_string(), "walk".to_string());
    idle_trans.insert("scrolling".to_string(), "clip_claws".to_string());
    idle_trans.insert("idle".to_string(), "sleep".to_string());

    states.insert(
        "idle".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("idle"),
                fps: 2.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                wrap_mode: WrapMode::Clamp,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: idle_trans,
                timeout_ms: Some(8000),
                on_timeout: Some("clip_claws".to_string()),
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Walk sideways
    let mut walk_trans = HashMap::new();
    walk_trans.insert("typing".to_string(), "walk_fast".to_string());
    walk_trans.insert("idle".to_string(), "idle".to_string());

    states.insert(
        "walk".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("walk"),
                fps: 5.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 1.2,
                target_vy: 0.0,
                wrap_mode: WrapMode::Bounce,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: walk_trans,
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Walk fast
    let mut crab_walk_fast_trans = HashMap::new();
    crab_walk_fast_trans.insert("scrolling".to_string(), "clip_claws".to_string());
    crab_walk_fast_trans.insert("idle".to_string(), "idle".to_string());

    states.insert(
        "walk_fast".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("walk_fast"),
                fps: 10.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 2.8,
                target_vy: 0.0,
                wrap_mode: WrapMode::Bounce,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: crab_walk_fast_trans,
                timeout_ms: Some(2000),
                on_timeout: Some("walk".to_string()),
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Clip claws
    states.insert(
        "clip_claws".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("clip_claws"),
                fps: 6.0,
                loop_anim: false,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_finish: Some("idle".to_string()),
                timeout_ms: Some(1500),
                on_timeout: Some("idle".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Burrow
    states.insert(
        "burrow".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("burrow"),
                fps: 4.0,
                loop_anim: false,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.5,
                ..Default::default()
            },
            transitions: TransitionConfig {
                timeout_ms: Some(3000),
                on_timeout: Some("sleep".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Sleep
    let mut sleep_trans = HashMap::new();
    sleep_trans.insert("typing".to_string(), "clip_claws".to_string());
    sleep_trans.insert("moving".to_string(), "idle".to_string());

    states.insert(
        "sleep".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::crab_set().frames_for("sleep"),
                fps: 1.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: sleep_trans,
                ..Default::default()
            },
            is_locked: false,
        },
    );

    let mut custom_actions = HashMap::new();
    custom_actions.insert(
        "clip".to_string(),
        CustomActionConfig {
            target_state: "clip_claws".to_string(),
            duration_ms: Some(1500),
            return_state: Some("idle".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "burrow".to_string(),
        CustomActionConfig {
            target_state: "burrow".to_string(),
            duration_ms: Some(3000),
            return_state: Some("sleep".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "sleep".to_string(),
        CustomActionConfig {
            target_state: "sleep".to_string(),
            duration_ms: None,
            return_state: None,
            is_locked: Some(false),
        },
    );
    custom_actions.insert(
        "wake".to_string(),
        CustomActionConfig {
            target_state: "idle".to_string(),
            duration_ms: None,
            return_state: None,
            is_locked: Some(false),
        },
    );
    custom_actions.insert(
        "walk".to_string(),
        CustomActionConfig {
            target_state: "walk".to_string(),
            duration_ms: None,
            return_state: None,
            is_locked: Some(false),
        },
    );

    AssetManifest {
        name: "crab".to_string(),
        asset_type: "procedural".to_string(),
        spritesheet: SpritesheetConfig::default(),
        initial_state: "idle".to_string(),
        states,
        custom_actions,
        z_index: Some(10),
        capabilities: Capabilities {
            locomotion: Some(vec![GROUNDED.to_string()]),
        },
        locomotion: Some(GROUNDED.to_string()),
        render: None,
    }
}

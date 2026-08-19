//! The built-in sun's manifest.
//!
//! A static state table and nothing else. §5 of the standards exempts pure data
//! tables in dedicated files from the size cap, and keeping all three of these in
//! `manifest.rs` is what put that module past 1,400 lines.

use std::collections::HashMap;

use crate::manifest::{
    AnimationConfig, AssetManifest, Capabilities, CustomActionConfig, OMNIDIRECTIONAL,
    PhysicsConfig, SpritesheetConfig, StateDefinition, TransitionConfig, WrapMode,
};
use crate::sprites;

/// Built-in Sun manifest with shining, rise, set, eclipse, and solar flare.
pub fn manifest() -> AssetManifest {
    let mut states = HashMap::new();

    // Shining (default state)
    let mut shine_trans = HashMap::new();
    shine_trans.insert("scrolling".to_string(), "flare".to_string());

    states.insert(
        "shining".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::sun_set().frames_for("shining"),
                fps: 2.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.2,
                target_vy: 0.0,
                wrap_mode: WrapMode::Wrap,
                path_type: Some("sine".to_string()),
                path_amplitude: Some(15.0),
                path_frequency: Some(2.0),
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: shine_trans,
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Rising (moves upwards)
    states.insert(
        "rising".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::sun_set().frames_for("rising"),
                fps: 2.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.5,
                target_vy: -1.0,
                wrap_mode: WrapMode::Clamp,
                ..Default::default()
            },
            transitions: TransitionConfig {
                timeout_ms: Some(4000),
                on_timeout: Some("shining".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Setting (moves downwards)
    states.insert(
        "setting".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::sun_set().frames_for("setting"),
                fps: 2.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.5,
                target_vy: 1.0,
                wrap_mode: WrapMode::Clamp,
                ..Default::default()
            },
            transitions: TransitionConfig {
                timeout_ms: Some(4000),
                on_timeout: Some("shining".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Eclipse (moon blocks sun corona)
    states.insert(
        "eclipse".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::sun_set().frames_for("eclipse"),
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
                timeout_ms: Some(8000),
                on_timeout: Some("shining".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Solar flare
    states.insert(
        "flare".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::sun_set().frames_for("flare"),
                fps: 6.0,
                loop_anim: false,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.4,
                target_vy: 0.0,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_finish: Some("shining".to_string()),
                timeout_ms: Some(2000),
                on_timeout: Some("shining".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    let mut custom_actions = HashMap::new();
    custom_actions.insert(
        "eclipse".to_string(),
        CustomActionConfig {
            target_state: "eclipse".to_string(),
            duration_ms: Some(8000),
            return_state: Some("shining".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "rise".to_string(),
        CustomActionConfig {
            target_state: "rising".to_string(),
            duration_ms: Some(4000),
            return_state: Some("shining".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "set".to_string(),
        CustomActionConfig {
            target_state: "setting".to_string(),
            duration_ms: Some(4000),
            return_state: Some("shining".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "flare".to_string(),
        CustomActionConfig {
            target_state: "flare".to_string(),
            duration_ms: Some(2000),
            return_state: Some("shining".to_string()),
            is_locked: Some(true),
        },
    );

    AssetManifest {
        name: "sun".to_string(),
        asset_type: "procedural".to_string(),
        spritesheet: SpritesheetConfig::default(),
        initial_state: "shining".to_string(),
        states,
        custom_actions,
        z_index: Some(-10),
        capabilities: Capabilities {
            locomotion: Some(vec![OMNIDIRECTIONAL.to_string()]),
        },
        locomotion: Some(OMNIDIRECTIONAL.to_string()),
        render: None,
    }
}

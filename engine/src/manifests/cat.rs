//! The built-in cat's manifest.
//!
//! A static state table and nothing else. §5 of the standards exempts pure data
//! tables in dedicated files from the size cap, and keeping all three of these in
//! `manifest.rs` is what put that module past 1,400 lines.

use std::collections::HashMap;

use crate::manifest::{
    AnimationConfig, AssetManifest, BALLISTIC, Capabilities, CustomActionConfig, GROUNDED,
    PhysicsConfig, SpritesheetConfig, StateDefinition, TransitionConfig, WrapMode,
};
use crate::sprites;

/// Built-in procedural Cat manifest with walk, jump, yawn, sleep, sit behaviors.
pub fn manifest() -> AssetManifest {
    let mut states = HashMap::new();

    // Idle state: transitions to walk on typing/moving, sleep on idle timeout
    let mut idle_transitions = HashMap::new();
    idle_transitions.insert("typing".to_string(), "walk_fast".to_string());
    idle_transitions.insert("moving".to_string(), "walk".to_string());
    idle_transitions.insert("scrolling".to_string(), "yawn".to_string());
    idle_transitions.insert("idle".to_string(), "sleep".to_string());

    states.insert(
        "idle".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("idle"),
                fps: 2.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                friction: 0.1,
                wrap_mode: WrapMode::Clamp,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: idle_transitions,
                timeout_ms: Some(6000),
                on_timeout: Some("sleep".to_string()),
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Walk right
    let mut walk_transitions = HashMap::new();
    walk_transitions.insert("typing".to_string(), "walk_fast".to_string());
    walk_transitions.insert("idle".to_string(), "idle".to_string());
    walk_transitions.insert("scrolling".to_string(), "yawn".to_string());

    states.insert(
        "walk".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("walk"),
                fps: 6.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 1.5,
                target_vy: 0.0,
                wrap_mode: WrapMode::Wrap,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: walk_transitions,
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Walk fast (sprinting when typing)
    let mut walk_fast_transitions = HashMap::new();
    walk_fast_transitions.insert("idle".to_string(), "idle".to_string());
    walk_fast_transitions.insert("moving".to_string(), "walk".to_string());
    walk_fast_transitions.insert("scrolling".to_string(), "yawn".to_string());

    states.insert(
        "walk_fast".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("walk_fast"),
                fps: 12.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 3.5,
                target_vy: 0.0,
                wrap_mode: WrapMode::Wrap,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: walk_fast_transitions,
                timeout_ms: Some(1500),
                on_timeout: Some("walk".to_string()),
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Jump: launches vertically with gravity, returns to idle on landing
    states.insert(
        "jump".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("jump"),
                fps: 10.0,
                loop_anim: false,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 2.0,
                target_vy: 0.0,
                jump_impulse_y: Some(-2.2),
                gravity: 0.32,
                wrap_mode: WrapMode::Bounce,
                // The one state that leaves the ground, and the reason the
                // cat declares `ballistic` at all.
                locomotion: Some(BALLISTIC.to_string()),
                ..Default::default()
            },
            transitions: TransitionConfig {
                // The jump ends when the cat lands, not when a clock says
                // it should have. `timeout_ms` stays as the
                // floor-never-reached fallback: a duration tuned against
                // `gravity` and `jump_impulse_y` is a number that has to be
                // re-tuned by hand every time either of them moves.
                on_land: Some("idle".to_string()),
                timeout_ms: Some(1200),
                on_timeout: Some("idle".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Yawn
    states.insert(
        "yawn".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("yawn"),
                fps: 3.0,
                loop_anim: false,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_finish: Some("sleep".to_string()),
                timeout_ms: Some(2000),
                on_timeout: Some("sleep".to_string()),
                ..Default::default()
            },
            is_locked: true,
        },
    );

    // Sleep
    let mut sleep_transitions = HashMap::new();
    sleep_transitions.insert("typing".to_string(), "yawn".to_string());
    sleep_transitions.insert("moving".to_string(), "idle".to_string());

    states.insert(
        "sleep".to_string(),
        StateDefinition {
            animation: AnimationConfig {
                frames: sprites::cat_set().frames_for("sleep"),
                fps: 1.0,
                loop_anim: true,
                flip_x: false,
            },
            physics: PhysicsConfig {
                target_vx: 0.0,
                target_vy: 0.0,
                friction: 0.2,
                ..Default::default()
            },
            transitions: TransitionConfig {
                on_event: sleep_transitions,
                ..Default::default()
            },
            is_locked: false,
        },
    );

    // Custom actions
    let mut custom_actions = HashMap::new();
    custom_actions.insert(
        "jump".to_string(),
        CustomActionConfig {
            target_state: "jump".to_string(),
            duration_ms: Some(1200),
            return_state: Some("idle".to_string()),
            is_locked: Some(true),
        },
    );
    custom_actions.insert(
        "yawn".to_string(),
        CustomActionConfig {
            target_state: "yawn".to_string(),
            duration_ms: Some(2000),
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

    AssetManifest {
        name: "cat".to_string(),
        asset_type: "sprite".to_string(),
        spritesheet: SpritesheetConfig::default(),
        initial_state: "idle".to_string(),
        states,
        custom_actions,
        z_index: Some(10),
        capabilities: Capabilities {
            locomotion: Some(vec![GROUNDED.to_string(), BALLISTIC.to_string()]),
        },
        locomotion: Some(GROUNDED.to_string()),
        render: None,
    }
}

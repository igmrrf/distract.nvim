//! Solid ground and hazards a plugin registered, and the geometry that resolves
//! them.
//!
//! Collected in Neovim and pushed here, never discovered by this engine: only
//! the editor can run a Tree-sitter query or read a fold, and an engine that
//! went looking for its own obstacles is the divergence class the physics parity
//! harness exists to catch — exactly as with the floor.
//!
//! In overlay pixels, matching every other coordinate the engine is sent. The
//! resolution rules below are mirrored line for line by `lua/distract/obstacles.lua`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The most obstacles a client may push.
///
/// A Tree-sitter query over a large file can produce thousands of matches; the
/// physics pass is per entity per obstacle per frame, so the list is bounded at
/// the boundary rather than trusted.
pub const MAX_OBSTACLES: usize = 128;

/// What an obstacle does to an entity that reaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObstacleKind {
    /// A one-way floor: it catches an entity falling onto it from above and is
    /// what a grounded entity walks along. Passing upward through it is free,
    /// which is what makes a jump onto a platform work.
    #[serde(rename = "solid_platform", alias = "platform")]
    SolidPlatform,
    /// A vertical wall an entity turns away from.
    #[serde(rename = "hazard")]
    Hazard,
}

/// One registered rectangle, in overlay pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Obstacle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(rename = "type", alias = "kind")]
    pub kind: ObstacleKind,
}

impl Obstacle {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    fn spans(&self, left: f32, right: f32) -> bool {
        self.x < right && left < self.right()
    }
}

/// Reads an obstacle list, accepting `{}` as well as `[]`.
///
/// `vim.json.encode` writes an empty Lua table as `{}`, and a session whose
/// provider currently finds nothing sends exactly that. Rejecting the encoding
/// would make "no platforms right now" an error on the overlay and a no-op in the
/// terminal. A *keyed* table is still an error: that is a mistake, not an empty
/// list.
///
/// # Errors
/// When the value is neither a list of obstacles nor an empty table.
pub fn list_allowing_empty_table<'de, D>(deserializer: D) -> Result<Vec<Obstacle>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Obstacles {
        List(Vec<Obstacle>),
        Table(HashMap<String, serde::de::IgnoredAny>),
    }

    match Option::<Obstacles>::deserialize(deserializer)? {
        None => Ok(Vec::new()),
        Some(Obstacles::List(obstacles)) => Ok(obstacles),
        Some(Obstacles::Table(table)) if table.is_empty() => Ok(Vec::new()),
        Some(Obstacles::Table(_)) => Err(serde::de::Error::custom(
            "obstacles must be a list; a keyed table is a mistake, not an empty list",
        )),
    }
}

/// The footprint an entity occupies, in overlay pixels.
#[derive(Debug, Clone, Copy)]
pub struct Footprint {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Footprint {
    pub fn right(&self) -> f32 {
        self.left + self.width
    }

    pub fn feet(&self) -> f32 {
        self.top + self.height
    }
}

/// Which way an entity was pushed out of a hazard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushDirection {
    /// Turned back to the left, so its heading is now negative.
    Left,
    /// Turned back to the right.
    Right,
}

/// Where an entity is put when it is turned away from a hazard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deflection {
    pub x: f32,
    pub direction: PushDirection,
}

/// The platform a falling entity has just crossed, if any.
///
/// Crossing is what counts, not overlapping: an entity resting on a platform has
/// its feet exactly on the top edge and keeps re-crossing it every frame as
/// gravity re-accelerates it, which is what holds it there. Something moving
/// upward through a platform crosses nothing.
///
/// The highest crossed edge wins, so falling past several platforms in one frame
/// lands on the first one reached rather than the last one tested.
pub fn crossed_platform(
    obstacles: &[Obstacle],
    footprint: Footprint,
    feet_before: f32,
) -> Option<f32> {
    let feet_after = footprint.feet();
    if feet_after < feet_before {
        return None;
    }

    obstacles
        .iter()
        .filter(|obstacle| obstacle.kind == ObstacleKind::SolidPlatform)
        .filter(|obstacle| obstacle.spans(footprint.left, footprint.right()))
        .filter(|obstacle| feet_before <= obstacle.y && obstacle.y <= feet_after)
        .map(|obstacle| obstacle.y)
        .min_by(|left, right| left.total_cmp(right))
}

/// The surface a grounded entity stands on, given the floor it would otherwise
/// use.
///
/// A grounded state has no gravity — a walking cat's `y` never changes on its
/// own — so standing on a platform is a matter of which surface is under its
/// feet, resolved every frame. Only platforms between the entity's own top and
/// its floor count: one above its head is scenery, and one below the floor is
/// unreachable.
pub fn standing_surface(obstacles: &[Obstacle], footprint: Footprint, floor: f32) -> f32 {
    obstacles
        .iter()
        .filter(|obstacle| obstacle.kind == ObstacleKind::SolidPlatform)
        .filter(|obstacle| obstacle.spans(footprint.left, footprint.right()))
        .filter(|obstacle| obstacle.y >= footprint.top && obstacle.y <= floor)
        .map(|obstacle| obstacle.y)
        .min_by(|left, right| left.total_cmp(right))
        .unwrap_or(floor)
}

/// Where a hazard puts an entity that has walked into it.
///
/// The side is decided by which edge the entity is nearer to, so an entity that
/// arrived from the left is returned to the left. A hazard the entity is exactly
/// centred on returns it the way it came, which is the direction its heading
/// already describes.
pub fn deflection(
    obstacles: &[Obstacle],
    footprint: Footprint,
    heading_x: f32,
) -> Option<Deflection> {
    let hazard = obstacles
        .iter()
        .filter(|obstacle| obstacle.kind == ObstacleKind::Hazard)
        .find(|obstacle| {
            obstacle.spans(footprint.left, footprint.right())
                && obstacle.y < footprint.feet()
                && footprint.top < obstacle.y + obstacle.height
        })?;

    let overlap_from_left = footprint.right() - hazard.x;
    let overlap_from_right = hazard.right() - footprint.left;
    let came_from_left = if overlap_from_left == overlap_from_right {
        heading_x >= 0.0
    } else {
        overlap_from_left < overlap_from_right
    };

    if came_from_left {
        Some(Deflection {
            x: hazard.x - footprint.width,
            direction: PushDirection::Left,
        })
    } else {
        Some(Deflection {
            x: hazard.right(),
            direction: PushDirection::Right,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform(x: f32, y: f32, width: f32) -> Obstacle {
        Obstacle {
            x,
            y,
            width,
            height: 1.0,
            kind: ObstacleKind::SolidPlatform,
        }
    }

    fn hazard(x: f32, y: f32, width: f32, height: f32) -> Obstacle {
        Obstacle {
            x,
            y,
            width,
            height,
            kind: ObstacleKind::Hazard,
        }
    }

    fn footprint(left: f32, top: f32) -> Footprint {
        Footprint {
            left,
            top,
            width: 20.0,
            height: 10.0,
        }
    }

    #[test]
    fn an_empty_lua_table_is_an_empty_obstacle_list() {
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default, deserialize_with = "list_allowing_empty_table")]
            obstacles: Vec<Obstacle>,
        }

        let empty: Wrapper = serde_json::from_str(r#"{"obstacles":{}}"#)
            .expect("vim.json.encode writes an empty table as {}");
        assert!(empty.obstacles.is_empty());

        let absent: Wrapper = serde_json::from_str("{}").expect("an absent list is empty");
        assert!(absent.obstacles.is_empty());

        let listed: Wrapper = serde_json::from_str(
            r#"{"obstacles":[{"x":1,"y":2,"width":3,"height":4,"type":"solid_platform"}]}"#,
        )
        .expect("a real list parses");
        assert_eq!(listed.obstacles.len(), 1);
        assert_eq!(listed.obstacles[0].kind, ObstacleKind::SolidPlatform);

        assert!(
            serde_json::from_str::<Wrapper>(r#"{"obstacles":{"first":{"x":1}}}"#).is_err(),
            "a keyed table is a mistake worth reporting"
        );
    }

    #[test]
    fn a_falling_entity_lands_on_the_platform_it_crossed() {
        let obstacles = [platform(0.0, 100.0, 200.0)];
        // Feet were at 95, are now at 105: the edge at 100 was crossed.
        let landed = crossed_platform(&obstacles, footprint(10.0, 95.0), 85.0);
        assert_eq!(landed, Some(100.0));
    }

    #[test]
    fn an_entity_resting_on_a_platform_keeps_being_caught_by_it() {
        let obstacles = [platform(0.0, 100.0, 200.0)];
        // Resting: feet exactly on the edge, then gravity nudges them below it.
        let landed = crossed_platform(&obstacles, footprint(10.0, 90.5), 100.0);
        assert_eq!(landed, Some(100.0));
    }

    #[test]
    fn a_platform_is_one_way() {
        let obstacles = [platform(0.0, 100.0, 200.0)];
        // Moving upward through the same edge: feet were below, are now above.
        assert_eq!(
            crossed_platform(&obstacles, footprint(10.0, 80.0), 105.0),
            None
        );
    }

    #[test]
    fn a_platform_beside_the_entity_catches_nothing() {
        let obstacles = [platform(500.0, 100.0, 200.0)];
        assert_eq!(
            crossed_platform(&obstacles, footprint(10.0, 95.0), 85.0),
            None
        );
    }

    #[test]
    fn falling_past_several_platforms_lands_on_the_first_one_reached() {
        let obstacles = [
            platform(0.0, 160.0, 200.0),
            platform(0.0, 120.0, 200.0),
            platform(0.0, 200.0, 200.0),
        ];
        let landed = crossed_platform(&obstacles, footprint(10.0, 250.0), 100.0);
        assert_eq!(landed, Some(120.0));
    }

    #[test]
    fn a_grounded_entity_stands_on_the_highest_platform_under_its_feet() {
        let obstacles = [platform(0.0, 300.0, 400.0), platform(0.0, 250.0, 400.0)];
        let surface = standing_surface(&obstacles, footprint(10.0, 240.0), 400.0);
        assert_eq!(surface, 250.0);
    }

    #[test]
    fn a_grounded_entity_falls_back_to_the_floor_when_it_walks_off_the_end() {
        let obstacles = [platform(0.0, 250.0, 100.0)];
        let surface = standing_surface(&obstacles, footprint(300.0, 240.0), 400.0);
        assert_eq!(surface, 400.0);
    }

    #[test]
    fn a_platform_above_the_entitys_head_is_scenery() {
        let obstacles = [platform(0.0, 50.0, 400.0)];
        let surface = standing_surface(&obstacles, footprint(10.0, 240.0), 400.0);
        assert_eq!(surface, 400.0);
    }

    #[test]
    fn a_hazard_returns_an_entity_the_way_it_came() {
        let obstacles = [hazard(100.0, 0.0, 20.0, 400.0)];
        // Arrived from the left: its right edge is just inside the hazard.
        let deflected = deflection(&obstacles, footprint(95.0, 100.0), 1.0)
            .expect("the footprint overlaps the hazard");
        assert_eq!(deflected.direction, PushDirection::Left);
        assert_eq!(deflected.x, 80.0);

        // Arrived from the right.
        let deflected = deflection(&obstacles, footprint(105.0, 100.0), -1.0)
            .expect("the footprint overlaps the hazard");
        assert_eq!(deflected.direction, PushDirection::Right);
        assert_eq!(deflected.x, 120.0);
    }

    #[test]
    fn a_hazard_the_entity_is_not_touching_deflects_nothing() {
        let obstacles = [hazard(100.0, 0.0, 20.0, 40.0)];
        // Horizontally over it, vertically well below.
        assert_eq!(deflection(&obstacles, footprint(95.0, 300.0), 1.0), None);
        // Vertically level, horizontally clear.
        assert_eq!(deflection(&obstacles, footprint(400.0, 10.0), 1.0), None);
    }

    #[test]
    fn a_platform_never_deflects_and_a_hazard_never_supports() {
        let obstacles = [
            platform(100.0, 100.0, 20.0),
            hazard(100.0, 100.0, 20.0, 20.0),
        ];
        assert_eq!(
            deflection(&obstacles[..1], footprint(95.0, 95.0), 1.0),
            None
        );
        assert_eq!(
            standing_surface(&obstacles[1..], footprint(95.0, 50.0), 400.0),
            400.0
        );
    }
}

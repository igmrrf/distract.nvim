//! Where a wrapping sprite's departing half is drawn.
//!
//! `wrap_mode = "wrap"` lets an entity hang off an edge — physics only teleports
//! it once it is entirely past — so the part that has left one edge has to appear
//! at the other in the same frame, or the sprite reads as stopping at the edge
//! and then popping.
//!
//! The overlay draws it as extra whole quads at complementary positions rather
//! than as UV-sliced ones: the render pass is scissored to the bounds, so the
//! part of a complementary quad that falls outside is clipped by the GPU and the
//! visible result is identical with no per-slice UV arithmetic. The in-terminal
//! renderer cannot do that — a float has to be on the grid — so
//! `lua/distract/placement.lua` slices instead, from the same 1D rule.

use crate::bounds::Bounds;

/// How far a quad is shifted to draw a wrapped half, in overlay pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Offset {
    pub dx: f32,
    pub dy: f32,
}

/// Every offset a wrapping sprite must be drawn at, the entity's own included.
///
/// One offset for a sprite fully inside the bounds, two while it crosses one
/// edge, and four in a corner — which is the case worth testing first, because a
/// renderer that handles one axis at a time draws three of the four.
pub fn offsets(position: (f32, f32), size: (f32, f32), bounds: Bounds) -> Vec<Offset> {
    let horizontal = axis_offsets(position.0, size.0, bounds.left, bounds.right());
    let vertical = axis_offsets(position.1, size.1, bounds.top, bounds.bottom());

    let mut all = Vec::with_capacity(horizontal.len() * vertical.len());
    for dy in &vertical {
        for dx in &horizontal {
            all.push(Offset { dx: *dx, dy: *dy });
        }
    }
    all
}

/// The shifts one axis needs: always zero, plus the wrap if an edge is crossed.
fn axis_offsets(position: f32, size: f32, min: f32, max: f32) -> Vec<f32> {
    let extent = max - min;
    if extent <= 0.0 {
        return vec![0.0];
    }

    let mut shifts = vec![0.0];
    if position + size > max {
        shifts.push(-extent);
    }
    if position < min {
        shifts.push(extent);
    }
    shifts
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Bounds = Bounds {
        left: 0.0,
        top: 0.0,
        width: 800.0,
        height: 600.0,
    };

    #[test]
    fn a_sprite_well_inside_the_bounds_is_drawn_once() {
        let offsets = offsets((100.0, 100.0), (40.0, 20.0), WINDOW);
        assert_eq!(offsets, vec![Offset { dx: 0.0, dy: 0.0 }]);
    }

    #[test]
    fn a_sprite_crossing_the_right_edge_is_drawn_again_on_the_left() {
        let offsets = offsets((790.0, 100.0), (40.0, 20.0), WINDOW);
        assert_eq!(
            offsets,
            vec![
                Offset { dx: 0.0, dy: 0.0 },
                Offset {
                    dx: -800.0,
                    dy: 0.0
                },
            ]
        );
    }

    #[test]
    fn a_sprite_still_off_the_left_edge_is_drawn_again_on_the_right() {
        let offsets = offsets((-10.0, 100.0), (40.0, 20.0), WINDOW);
        assert_eq!(
            offsets,
            vec![Offset { dx: 0.0, dy: 0.0 }, Offset { dx: 800.0, dy: 0.0 },]
        );
    }

    #[test]
    fn a_sprite_leaving_a_corner_needs_four_quads() {
        let offsets = offsets((790.0, 590.0), (40.0, 20.0), WINDOW);
        assert_eq!(offsets.len(), 4);
        assert!(offsets.contains(&Offset { dx: 0.0, dy: 0.0 }));
        assert!(offsets.contains(&Offset {
            dx: -800.0,
            dy: 0.0
        }));
        assert!(offsets.contains(&Offset {
            dx: 0.0,
            dy: -600.0
        }));
        assert!(offsets.contains(&Offset {
            dx: -800.0,
            dy: -600.0
        }));
    }

    #[test]
    fn the_offsets_are_measured_against_a_scoped_rectangle_not_the_window() {
        let scope = Bounds {
            left: 100.0,
            top: 50.0,
            width: 200.0,
            height: 100.0,
        };
        // Inside the window, but past the scope's right edge.
        let offsets = offsets((290.0, 60.0), (40.0, 20.0), scope);
        assert_eq!(
            offsets,
            vec![
                Offset { dx: 0.0, dy: 0.0 },
                Offset {
                    dx: -200.0,
                    dy: 0.0
                },
            ]
        );
    }

    #[test]
    fn a_rectangle_with_no_room_asks_for_one_quad_rather_than_none() {
        let empty = Bounds {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
        };
        assert_eq!(
            offsets((0.0, 0.0), (10.0, 10.0), empty),
            vec![Offset { dx: 0.0, dy: 0.0 }]
        );
    }
}

//! What one tick costs at the scale a particle system would want.
//!
//! `future.md` §5.5 makes this the gate on ambient weather: the ECS was built for
//! three entities and rain wants hundreds, and if a per-entity tick misses the
//! frame budget then weather starts as a batched particle path in the *core*
//! rather than as a plugin. Answering that needs a measurement, not an opinion.
//!
//! The assertion is deliberately loose — this runs on whatever machine CI gives
//! it, in a debug build — and its job is to catch an order-of-magnitude
//! regression, not to police microseconds. The number it prints is the answer to
//! the design question; run with `--nocapture` to see it.

use std::time::Instant;

use distract_engine::ecs::World;
use distract_engine::spawn::SpawnOptions;

/// Entities the weather question is about.
const ENTITIES: usize = 200;
/// Ticks averaged over, enough to swamp one-off allocation.
const TICKS: usize = 120;
/// One frame at 60 FPS.
const FRAME_BUDGET_MS: f64 = 1000.0 / 60.0;

#[test]
fn two_hundred_entities_step_inside_one_frame() {
    let mut world = World::new(1920.0, 1080.0);

    for index in 0..ENTITIES {
        let x = (index % 40) as f32 * 48.0;
        let y = (index / 40) as f32 * 200.0;
        world
            .spawn("cat", None, SpawnOptions::at(x, y))
            .expect("the built-in cat spawns");
    }
    assert_eq!(world.entities.len(), ENTITIES);

    // One untimed tick first: the first call resolves every asset and allocates,
    // and attributing that to the steady-state cost would answer a different
    // question.
    world.update(1.0 / 60.0);

    let started = Instant::now();
    for _ in 0..TICKS {
        world.update(1.0 / 60.0);
    }
    let per_tick_ms = started.elapsed().as_secs_f64() * 1000.0 / TICKS as f64;

    println!(
        "{ENTITIES} entities: {per_tick_ms:.3} ms per tick \
         ({:.1}% of a 60 FPS frame, debug build)",
        per_tick_ms / FRAME_BUDGET_MS * 100.0
    );

    assert!(
        per_tick_ms < FRAME_BUDGET_MS,
        "{ENTITIES} entities cost {per_tick_ms:.3} ms per tick, over the \
         {FRAME_BUDGET_MS:.1} ms frame budget: a particle system would have to be \
         a batched core path rather than a plugin"
    );
}

#[test]
fn the_cost_of_a_tick_is_linear_in_the_entity_count() {
    fn cost_ms(count: usize) -> f64 {
        let mut world = World::new(1920.0, 1080.0);
        for index in 0..count {
            world
                .spawn(
                    "cat",
                    None,
                    SpawnOptions::at((index % 40) as f32 * 48.0, 100.0),
                )
                .expect("the built-in cat spawns");
        }
        world.update(1.0 / 60.0);

        let started = Instant::now();
        for _ in 0..TICKS {
            world.update(1.0 / 60.0);
        }
        started.elapsed().as_secs_f64() * 1000.0 / TICKS as f64
    }

    let small = cost_ms(50);
    let large = cost_ms(200);
    println!("50 entities: {small:.3} ms per tick, 200 entities: {large:.3} ms per tick");

    // Four times the entities for at most eight times the cost. Anything worse
    // than that is a quadratic term — an entity consulting every other entity —
    // which is what would actually rule a particle plugin out.
    assert!(
        large < small * 8.0 + 0.5,
        "200 entities cost {large:.3} ms against {small:.3} ms for 50: \
         that is worse than linear, so something is comparing entities pairwise"
    );
}

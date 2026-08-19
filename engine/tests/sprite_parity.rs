//! Cross-engine sprite-art parity, Rust half.
//!
//! The same procedural art exists twice: `lua/distract/sprites/*.lua` draws it
//! for the terminal renderer and `engine/src/sprites/*.rs` draws it for the
//! overlay, from the same pose curves and the same shading model. Nothing
//! compared the two, so three assets times two implementations were six files
//! free to drift the moment one was touched.
//!
//! This is the physics-parity arrangement applied to art. Rust produces the
//! goldens and asserts it still reproduces them, so a change to a pose curve or
//! a shading term fails here. `tests/sprite_parity_spec.lua` asserts the *Lua*
//! generators reproduce the same pixels, within the tolerance the two ports'
//! float widths make unavoidable. Neither suite runs the other's toolchain —
//! they meet at the JSON in `tests/fixtures/sprites/`.
//!
//! Regenerate after an intentional art change:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test sprite_parity
//! ```
//!
//! Rust is the reference by construction, which is this harness's own blind
//! spot: if Rust is wrong, both suites agree on the wrong answer.
//! `goldens_describe_the_dimensions_the_manifests_index` is computed from the
//! asset specifications rather than from either generator.

use distract_engine::sprites;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const ASSETS: [&str; 3] = ["cat", "crab", "sun"];

/// A fully transparent pixel. Six characters so a row reads as a fixed-width
/// grid in a diff, which is the only way a pixel dump is reviewable by eye.
const TRANSPARENT: &str = "------";

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct AssetDump {
    name: String,
    width: u32,
    height: u32,
    /// State name -> 0-based indices into `frames`. Pinned alongside the pixels
    /// because a state pointing at the wrong frames is the same defect class as
    /// a frame drawn wrongly, and is cheaper to catch here than on a screen.
    layout: BTreeMap<String, Vec<usize>>,
    /// One entry per frame: rows joined by `;`, pixels within a row by `,`,
    /// each pixel `rrggbb` or `------`.
    frames: Vec<String>,
}

fn dump(name: &str) -> AssetDump {
    let set = sprites::get(name);
    let frames = set
        .frames
        .iter()
        .map(|image| {
            (0..set.height)
                .map(|y| {
                    (0..set.width)
                        .map(|x| {
                            let pixel = image.get_pixel(x, y);
                            if pixel[3] == 0 {
                                TRANSPARENT.to_string()
                            } else {
                                format!("{:02x}{:02x}{:02x}", pixel[0], pixel[1], pixel[2])
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .collect();

    AssetDump {
        name: name.to_string(),
        width: set.width,
        height: set.height,
        layout: set
            .layout
            .iter()
            .map(|(state, indices)| (state.clone(), indices.clone()))
            .collect(),
        frames,
    }
}

fn fixture_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is `engine/`; fixtures are shared with the Lua suite.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .join("tests/fixtures/sprites")
}

#[test]
fn rust_sprite_art_matches_the_goldens() {
    let dir = fixture_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();

    for name in ASSETS {
        let actual = dump(name);
        let golden_path = dir.join(format!("{name}.golden.json"));

        if update {
            std::fs::write(
                &golden_path,
                serde_json::to_string_pretty(&actual).expect("dump serialises"),
            )
            .expect("golden writable");
            continue;
        }

        let raw = std::fs::read_to_string(&golden_path).unwrap_or_else(|_| {
            panic!(
                "no golden for {name}. Generate with \
                 UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test sprite_parity"
            )
        });
        let expected: AssetDump = serde_json::from_str(&raw).expect("golden parses");

        assert_shape(name, &expected, &actual);
        assert_pixels(name, &expected, &actual);
    }
}

/// Everything about an asset except its pixels.
fn assert_shape(name: &str, expected: &AssetDump, actual: &AssetDump) {
    assert_eq!(
        expected.width, actual.width,
        "{name}: golden is {} wide, generator produced {}",
        expected.width, actual.width
    );
    assert_eq!(
        expected.height, actual.height,
        "{name}: golden is {} tall, generator produced {}",
        expected.height, actual.height
    );
    assert_eq!(
        expected.layout, actual.layout,
        "{name}: the state-to-frame layout changed"
    );
    assert_eq!(
        expected.frames.len(),
        actual.frames.len(),
        "{name}: golden has {} frames, generator produced {}",
        expected.frames.len(),
        actual.frames.len()
    );
}

/// Reports the first differing frame and the first differing pixel in it.
///
/// A whole-frame `assert_eq` would print two 24x16 hex grids and bury the one
/// pixel that moved.
fn assert_pixels(name: &str, expected: &AssetDump, actual: &AssetDump) {
    for (index, (want, got)) in expected.frames.iter().zip(actual.frames.iter()).enumerate() {
        if want == got {
            continue;
        }
        let want_pixels: Vec<&str> = want.split([';', ',']).collect();
        let got_pixels: Vec<&str> = got.split([';', ',']).collect();
        let first = want_pixels
            .iter()
            .zip(got_pixels.iter())
            .position(|(left, right)| left != right)
            .expect("frames differ, so some pixel differs");
        let column = first as u32 % actual.width;
        let row = first as u32 / actual.width;
        panic!(
            "{name} frame {index}: pixel at ({column}, {row}) is {} in the golden \
             and {} from the generator",
            want_pixels[first], got_pixels[first]
        );
    }
}

/// Guards the harness's own blind spot.
///
/// Every golden comes from Rust, so Rust cannot be wrong as far as the goldens
/// are concerned. These numbers are the asset specifications from
/// `docs/superpowers/specs/` and the manifest integrity sweep, not a reading of either
/// generator.
#[test]
fn goldens_describe_the_dimensions_the_manifests_index() {
    for (name, width, height, frame_count, state_count) in [
        ("cat", 24, 16, 29, 6),
        ("crab", 24, 16, 25, 6),
        ("sun", 16, 16, 25, 5),
    ] {
        let set = sprites::get(name);
        assert_eq!(set.width, width, "{name} is not {width} sprite pixels wide");
        assert_eq!(set.height, height, "{name} is not {height} tall");
        assert_eq!(
            set.frames.len(),
            frame_count,
            "{name} does not have {frame_count} frames"
        );
        assert_eq!(
            set.layout.len(),
            state_count,
            "{name} does not declare {state_count} states"
        );

        for (state, indices) in &set.layout {
            assert!(
                !indices.is_empty(),
                "{name} state {state} animates through no frames"
            );
            for index in indices {
                assert!(
                    *index < set.frames.len(),
                    "{name} state {state} indexes frame {index} of {}",
                    set.frames.len()
                );
            }
        }
    }
}

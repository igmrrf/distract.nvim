//! Cross-engine voxel-meshing parity, Rust half.
//!
//! The same extrusion exists twice: `engine/src/voxel.rs` builds the models the
//! overlay draws on the GPU and `lua/distract/voxel.lua` builds the models the
//! in-terminal backends rasterise. A pet that meshes differently on the two is a
//! pet that changes shape when the overlay opens, so the two are pinned to the
//! same goldens — the arrangement `physics_parity` and `sprite_parity` already
//! use, for the same reason.
//!
//! **The golden is the mesh, not a picture.** Comparing rasterised pixels would
//! fold a meshing difference and a rasterising difference into one number, and
//! the two engines rasterise deliberately differently: one on a GPU under a
//! perspective camera, one in Lua under an orthographic one.
//!
//! **Every fixture declares its own source grid.** Sprite art is only equal
//! across the engines within a measured drift, so meshing each engine's own cat
//! would compare two things that were already allowed to differ. A declared grid
//! makes the input identical and the meshing the only variable.
//!
//! Regenerate after an intentional meshing change:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test --manifest-path engine/Cargo.toml --test voxel_parity
//! ```

use distract_engine::voxel::{self, VoxelOptions};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A pixel in a fixture source: `null` for transparent, `"rrggbb"` otherwise.
///
/// A hex string rather than a triple so a source grid reads as a picture in a
/// diff, which is the only way a reviewer can see what a fixture is a model of.
type SourcePixel = Option<String>;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct VoxelGolden {
    /// Why this fixture exists, and any knife edge it deliberately avoids.
    description: String,
    max_width: u32,
    depth: u32,
    source_cols: u32,
    source_rows: u32,
    /// Row-major, `source_cols * source_rows` entries.
    source: Vec<SourcePixel>,
    /// The grid the source resampled to: `[cols, rows, depth]`.
    extent: [u32; 3],
    /// One entry per vertex, in emission order:
    /// `"x,y,z|nx,ny,nz|rrggbb"`. Emission order is part of the contract — the
    /// index list addresses it.
    vertices: Vec<String>,
    indices: Vec<u32>,
}

struct Fixture {
    name: &'static str,
    description: &'static str,
    options: VoxelOptions,
    cols: u32,
    rows: u32,
    /// One character per pixel: `.` transparent, anything else a keyed colour.
    art: &'static [&'static str],
}

/// Colours the fixture art keys into. Distinct enough that a channel swap shows.
fn colour_for(key: char) -> [u8; 3] {
    match key {
        'r' => [200, 40, 40],
        'g' => [40, 190, 60],
        'b' => [50, 70, 210],
        'w' => [240, 240, 235],
        _ => [128, 128, 128],
    }
}

fn fixtures() -> Vec<Fixture> {
    vec![
        Fixture {
            name: "single_voxel",
            description: "One pixel. Nothing hides any of its six faces.",
            options: VoxelOptions {
                max_width: 48,
                depth: 4,
            },
            cols: 1,
            rows: 1,
            art: &["r"],
        },
        Fixture {
            name: "two_adjacent",
            description: "Two neighbours: the face between them is never emitted.",
            options: VoxelOptions {
                max_width: 48,
                depth: 4,
            },
            cols: 2,
            rows: 1,
            art: &["rg"],
        },
        Fixture {
            name: "hollow_ring",
            description: "A ring, so every voxel has a solid neighbour on some \
                          sides and empty space on others. Catches a face-culling \
                          test that only looks in one direction.",
            options: VoxelOptions {
                max_width: 48,
                depth: 4,
            },
            cols: 5,
            rows: 5,
            art: &["rrrrr", "r...r", "r...r", "r...r", "rrrrr"],
        },
        Fixture {
            name: "thin_slab",
            description: "Depth 1: the front and back faces coincide in thickness \
                          but are still both emitted, because each is the outside \
                          of the model from one side.",
            options: VoxelOptions {
                max_width: 48,
                depth: 1,
            },
            cols: 3,
            rows: 2,
            art: &["rg.", ".bw"],
        },
        Fixture {
            name: "ragged_silhouette",
            description: "An asymmetric blob with single-pixel spurs. Every corner \
                          order and winding mistake shows up here and nowhere in a \
                          rectangle.",
            options: VoxelOptions {
                max_width: 48,
                depth: 3,
            },
            cols: 7,
            rows: 6,
            art: &[
                "..rr...", ".rrrr..", "rrrrrr.", ".rrrr.g", "..rr...", ".r...r.",
            ],
        },
        Fixture {
            name: "wide_resampled",
            description: "Wider than the cap, so the nearest-neighbour fit runs. \
                          The stripes are 8 source pixels wide against a cap of 12, \
                          which is deliberately not a whole ratio: an off-by-one in \
                          the fit arithmetic moves a stripe edge.",
            options: VoxelOptions {
                max_width: 12,
                depth: 2,
            },
            cols: 32,
            rows: 8,
            art: &[
                "rrrrrrrr........gggggggg........",
                "rrrrrrrr........gggggggg........",
                "........bbbbbbbb........wwwwwwww",
                "........bbbbbbbb........wwwwwwww",
                "rrrrrrrr........gggggggg........",
                "rrrrrrrr........gggggggg........",
                "........bbbbbbbb........wwwwwwww",
                "........bbbbbbbb........wwwwwwww",
            ],
        },
    ]
}

fn source_image(fixture: &Fixture) -> RgbaImage {
    RgbaImage::from_fn(fixture.cols, fixture.rows, |col, row| {
        let key = fixture.art[row as usize]
            .chars()
            .nth(col as usize)
            .unwrap_or('.');
        if key == '.' {
            return Rgba([0, 0, 0, 0]);
        }
        let [red, green, blue] = colour_for(key);
        Rgba([red, green, blue, 255])
    })
}

fn source_list(fixture: &Fixture) -> Vec<SourcePixel> {
    let mut pixels = Vec::with_capacity((fixture.cols * fixture.rows) as usize);
    for row in 0..fixture.rows {
        for col in 0..fixture.cols {
            let key = fixture.art[row as usize]
                .chars()
                .nth(col as usize)
                .unwrap_or('.');
            if key == '.' {
                pixels.push(None);
                continue;
            }
            let [red, green, blue] = colour_for(key);
            pixels.push(Some(format!("{red:02x}{green:02x}{blue:02x}")));
        }
    }
    pixels
}

/// A number as it appears in a golden: voxel coordinates are whole or exact
/// halves, so this is lossless and reads as a coordinate rather than as a float.
fn coordinate(value: f32) -> String {
    if value == value.trunc() {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn dump(fixture: &Fixture) -> VoxelGolden {
    let mesh = voxel::build(&source_image(fixture), fixture.options);
    let vertices = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let position = vertex
                .position
                .iter()
                .map(|axis| coordinate(*axis))
                .collect::<Vec<_>>()
                .join(",");
            // Snorm8 on the GPU, unit normals in the golden: 127 is exactly 1.0
            // once normalised, so this loses nothing and Lua has no i8.
            let normal = vertex.normal[0..3]
                .iter()
                .map(|axis| format!("{}", axis / 127))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{position}|{normal}|{:02x}{:02x}{:02x}",
                vertex.colour[0], vertex.colour[1], vertex.colour[2]
            )
        })
        .collect();

    VoxelGolden {
        description: fixture
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        max_width: fixture.options.max_width,
        depth: fixture.options.depth,
        source_cols: fixture.cols,
        source_rows: fixture.rows,
        source: source_list(fixture),
        extent: mesh.extent,
        vertices,
        indices: mesh.indices,
    }
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .join("tests/fixtures/voxels")
}

#[test]
fn rust_meshing_matches_the_goldens() {
    let dir = fixture_dir();
    let update = std::env::var("UPDATE_GOLDEN").is_ok();
    if update {
        std::fs::create_dir_all(&dir).expect("fixture directory creatable");
    }

    for fixture in fixtures() {
        let actual = dump(&fixture);
        let path = dir.join(format!("{}.golden.json", fixture.name));

        if update {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&actual).expect("golden serialises"),
            )
            .expect("golden writable");
            continue;
        }

        let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "no golden for {}. Generate with UPDATE_GOLDEN=1 cargo test \
                 --manifest-path engine/Cargo.toml --test voxel_parity",
                fixture.name
            )
        });
        let expected: VoxelGolden = serde_json::from_str(&raw).expect("golden parses");

        assert_eq!(
            expected.extent, actual.extent,
            "{}: the grid the source fitted to moved",
            fixture.name
        );
        assert_eq!(
            expected.vertices.len(),
            actual.vertices.len(),
            "{}: face culling changed the vertex count",
            fixture.name
        );
        for (index, (want, got)) in expected.vertices.iter().zip(&actual.vertices).enumerate() {
            assert_eq!(want, got, "{}: vertex {} differs", fixture.name, index);
        }
        assert_eq!(
            expected.indices, actual.indices,
            "{}: the index list differs",
            fixture.name
        );
    }
}

#[test]
fn every_fixture_declares_why_it_exists() {
    for fixture in fixtures() {
        assert!(
            fixture.description.len() > 20,
            "{}: a fixture without a description is one nobody can safely change",
            fixture.name
        );
    }
}

#[test]
fn the_fixture_art_is_rectangular() {
    for fixture in fixtures() {
        assert_eq!(
            fixture.art.len(),
            fixture.rows as usize,
            "{}: row count disagrees with the art",
            fixture.name
        );
        for (index, row) in fixture.art.iter().enumerate() {
            assert_eq!(
                row.chars().count(),
                fixture.cols as usize,
                "{}: row {} is not {} wide",
                fixture.name,
                index,
                fixture.cols
            );
        }
    }
}

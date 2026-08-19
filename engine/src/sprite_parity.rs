use crate::sprites;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TRANSPARENT: &str = "------";

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub struct AssetDump {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub layout: BTreeMap<String, Vec<usize>>,
    pub frames: Vec<String>,
}

pub fn dump(name: &str) -> AssetDump {
    let set = sprites::get(name);
    let frames = set
        .frames
        .iter()
        .map(|image| {
            (0..set.height)
                .map(|coordinate_y| {
                    (0..set.width)
                        .map(|coordinate_x| {
                            let pixel = image.get_pixel(coordinate_x, coordinate_y);
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

pub fn assert_shape(name: &str, expected: &AssetDump, actual: &AssetDump) {
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

pub fn assert_pixels(name: &str, expected: &AssetDump, actual: &AssetDump) {
    for (frame_index, (want, got)) in expected.frames.iter().zip(actual.frames.iter()).enumerate() {
        if want == got {
            continue;
        }
        let want_pixels: Vec<&str> = want.split([';', ',']).collect();
        let got_pixels: Vec<&str> = got.split([';', ',']).collect();
        let first_difference = want_pixels
            .iter()
            .zip(got_pixels.iter())
            .position(|(left, right)| left != right)
            .expect("frames differ, so some pixel differs");
        let column = first_difference as u32 % actual.width;
        let row = first_difference as u32 / actual.width;
        panic!(
            "{name} frame {frame_index}: pixel at ({column}, {row}) is {} in the golden and {} from the generator",
            want_pixels[first_difference], got_pixels[first_difference]
        );
    }
}

pub fn verify_manifest_dimensions(
    name: &str,
    width: u32,
    height: u32,
    frame_count: usize,
    state_count: usize,
) {
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

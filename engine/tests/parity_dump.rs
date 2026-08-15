//! Development aid for keeping the Rust and Lua sprite generators in sync.
//!
//! `engine/src/sprites/` is a port of `lua/distract/sprites/`. This dumps the
//! Rust frames in the same textual form the Lua side can produce, so the two
//! can be diffed pixel for pixel after a change to either.
//!
//! Run with:
//!
//! ```text
//! DUMP_TO=/tmp/rust_frames.txt cargo test --test parity_dump -- --ignored
//! ```
//!
//! The two ports are not expected to agree bit for bit: Lua computes in f64 and
//! Rust in f32, so a handful of pixels on the exact boundary of a shaded
//! ellipse fall on different sides. Measured drift is under 2% of pixels and is
//! not visible. A large jump means a transcription error.

use distract_engine::sprites;

#[test]
#[ignore = "development aid; needs DUMP_TO"]
fn dump_frames() {
    let Ok(target) = std::env::var("DUMP_TO") else {
        eprintln!("set DUMP_TO=<path> to dump frames");
        return;
    };

    let mut out = String::new();
    for name in ["cat", "crab", "sun"] {
        let set = sprites::get(name);
        let frames: Vec<String> = set
            .frames
            .iter()
            .map(|img| {
                (0..set.height)
                    .map(|y| {
                        (0..set.width)
                            .map(|x| {
                                let p = img.get_pixel(x, y);
                                if p[3] == 0 {
                                    "------".to_string()
                                } else {
                                    format!("{:02x}{:02x}{:02x}", p[0], p[1], p[2])
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect::<Vec<_>>()
                    .join(";")
            })
            .collect();

        out.push_str(&format!(
            "{}\t{}\t{}\n",
            name,
            set.frames.len(),
            frames.join("|")
        ));
    }

    std::fs::write(target, out.trim_end()).expect("could not write dump");
}

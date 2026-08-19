use distract_engine::sprite_parity::{
    AssetDump, assert_pixels, assert_shape, dump, verify_manifest_dimensions,
};
use std::path::{Path, PathBuf};

const ASSETS: [&str; 3] = ["cat", "crab", "sun"];

fn fixture_dir() -> PathBuf {
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

#[test]
fn goldens_describe_the_dimensions_the_manifests_index() {
    for (name, width, height, frame_count, state_count) in [
        ("cat", 24, 16, 29, 6),
        ("crab", 24, 16, 25, 6),
        ("sun", 16, 16, 25, 5),
    ] {
        verify_manifest_dimensions(name, width, height, frame_count, state_count);
    }
}

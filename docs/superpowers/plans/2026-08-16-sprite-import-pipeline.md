# High-fidelity sprite import pipeline — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A standalone Rust CLI (`import_sprite`) that turns a source GIF or PNG-frame folder into a background-removed spritesheet PNG (consumed by the existing, already-tested overlay backend) plus a raw-RGBA sidecar (consumed by a new kitty-backend loader) and a manifest scaffold — extending real image fidelity to every backend actually capable of showing it, with zero behavior change to halfblock or any existing asset.

**Architecture:** Rust CLI does all decode/background-removal/packing/encoding work using the already-vendored `image` crate, writing two runtime artifacts (`_sheet.png`, `_frames.rgba`) plus a Lua manifest scaffold from one decoded frame set. On the Lua side, `sprite_sources.get_pixel_frames` gains an opt-in second parameter that resolves a fourth, independently-cached art source (`native_sprite.lua`) ahead of its existing per-asset-name cache, so two backends requesting different fidelity for the same asset can never leak into each other.

**Tech Stack:** Rust (`image = "0.24"`, already a dependency — no new dependencies), Lua/LuaJIT (Neovim's embedded runtime — no `string.pack`/`unpack` available).

**Spec:** `docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md`

## Global Constraints

- No new external dependencies, Rust or Lua (repo standard: minimal dependencies, reuse before inventing).
- Zero `.unwrap()`/`.expect()` in production Rust paths (`#[cfg(test)]` code is exempt) — every fallible operation returns `Result<T, String>`, matching `export_sprites`' existing precedent in this codebase (no `thiserror`/`anyhow` currently in use here).
- Lua: `local` by default; expected failures return `nil, err`, never `error()`/throw, for anything that depends on file/asset content (malformed `.rgba`, missing file) — only genuine programmer-contract violations (bad arguments) use `error()`.
- Every new Rust file ≤ 400 lines, every function ≤ 60 lines, ≤ 3 positional parameters (a struct beyond that) — repo-wide caps.
- `cargo run --manifest-path engine/Cargo.toml --bin import_sprite` is invoked from the **repository root**; every relative path in this plan (`assets/...`, `lua/distract/manifests/...`) is repo-root-relative, matching what's already written into `lua/distract/manifests/cat_walking.lua`.
- `sprite_sources.lua`'s existing per-asset `sprite_cache` (keyed by `asset_name` only) must never learn about backend/resolution — the native-resolution path is resolved before it, never inside it (see spec § 3.2 and Task 10 below).
- No changes to `cat`, `crab`, `sun` procedural sprites, or to the halfblock backend's behavior for any existing asset.

---

## Part A — Import CLI (Rust)

### Task 1: CLI scaffold and argument parsing

**Files:**
- Create: `engine/src/bin/import_sprite/main.rs`
- Modify: `engine/Cargo.toml` (new `[[bin]]` entry)

**Interfaces:**
- Produces: `struct Args { gif: Option<PathBuf>, frames_dir: Option<PathBuf>, name: String, states: Option<String>, out: PathBuf, manifest_out: PathBuf, bg_tolerance: f32 }`, `fn parse_args() -> Result<Args, String>` — every later task's `main.rs` wiring reads these exact field names.

- [ ] **Step 1: Add the new binary to `engine/Cargo.toml`**

```toml
[[bin]]
name = "import_sprite"
path = "src/bin/import_sprite/main.rs"
```

- [ ] **Step 2: Write `main.rs` with argument parsing and its test**

```rust
use std::path::PathBuf;
use std::process;

const DEFAULT_BG_TOLERANCE: f32 = 0.12;

struct Args {
    gif: Option<PathBuf>,
    frames_dir: Option<PathBuf>,
    name: String,
    states: Option<String>,
    out: PathBuf,
    manifest_out: PathBuf,
    bg_tolerance: f32,
}

fn parse_args_from(mut raw_args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut gif = None;
    let mut frames_dir = None;
    let mut name: Option<String> = None;
    let mut states = None;
    let mut out = None;
    let mut manifest_out = None;
    let mut bg_tolerance = DEFAULT_BG_TOLERANCE;

    while let Some(flag) = raw_args.next() {
        let mut take_value = || raw_args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--gif" => gif = Some(PathBuf::from(take_value()?)),
            "--frames" => frames_dir = Some(PathBuf::from(take_value()?)),
            "--name" => name = Some(take_value()?),
            "--states" => states = Some(take_value()?),
            "--out" => out = Some(PathBuf::from(take_value()?)),
            "--manifest-out" => manifest_out = Some(PathBuf::from(take_value()?)),
            "--bg-tolerance" => {
                bg_tolerance = take_value()?
                    .parse()
                    .map_err(|_| "--bg-tolerance needs a number".to_string())?;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }

    let name = name.ok_or("--name is required")?;
    if gif.is_some() == frames_dir.is_some() {
        return Err("exactly one of --gif or --frames is required".to_string());
    }

    let out = out.unwrap_or_else(|| PathBuf::from(format!("assets/{name}")));
    let manifest_out =
        manifest_out.unwrap_or_else(|| PathBuf::from(format!("lua/distract/manifests/{name}.lua")));

    Ok(Args { gif, frames_dir, name, states, out, manifest_out, bg_tolerance })
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1))
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("import_sprite: {message}");
            process::exit(1);
        }
    };
    eprintln!("import_sprite: parsed args for asset '{}'", args.name);
    let _ = args; // wired up fully in Task 7
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(flags: &[&str]) -> Vec<String> {
        flags.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn requires_exactly_one_of_gif_or_frames() {
        let neither = parse_args_from(args(&["--name", "x"]).into_iter());
        assert!(neither.is_err());

        let both = parse_args_from(
            args(&["--gif", "a.gif", "--frames", "dir", "--name", "x"]).into_iter(),
        );
        assert!(both.is_err());
    }

    #[test]
    fn defaults_out_and_manifest_out_from_name() {
        let parsed =
            parse_args_from(args(&["--gif", "a.gif", "--name", "cat_walking"]).into_iter())
                .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("assets/cat_walking"));
        assert_eq!(
            parsed.manifest_out,
            PathBuf::from("lua/distract/manifests/cat_walking.lua")
        );
        assert_eq!(parsed.bg_tolerance, DEFAULT_BG_TOLERANCE);
    }

    #[test]
    fn explicit_flags_override_defaults() {
        let parsed = parse_args_from(
            args(&[
                "--frames", "dir", "--name", "x", "--out", "/tmp/out",
                "--manifest-out", "/tmp/x.lua", "--bg-tolerance", "0.2",
            ])
            .into_iter(),
        )
        .expect("parse");
        assert_eq!(parsed.out, PathBuf::from("/tmp/out"));
        assert_eq!(parsed.manifest_out, PathBuf::from("/tmp/x.lua"));
        assert_eq!(parsed.bg_tolerance, 0.2);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/Cargo.toml engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): scaffold CLI argument parsing"
```

---

### Task 2: Frame decoding (GIF and PNG folder)

**Files:**
- Create: `engine/src/bin/import_sprite/decode.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (add `mod decode;`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub struct DecodedFrame { pub image: RgbaImage, pub delay_ms: Option<u32> }`, `pub fn decode_gif(path: &Path) -> Result<Vec<DecodedFrame>, String>`, `pub fn decode_png_folder(dir: &Path) -> Result<Vec<DecodedFrame>, String>` — Task 7's `run()` matches on `(&args.gif, &args.frames_dir)` and calls these by these exact names.

- [ ] **Step 1: Write `decode.rs` with its tests (test-first: write the test, watch it fail to compile since the functions don't exist yet, then implement)**

```rust
use std::fs;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage};

pub struct DecodedFrame {
    pub image: RgbaImage,
    pub delay_ms: Option<u32>,
}

pub fn decode_gif(path: &Path) -> Result<Vec<DecodedFrame>, String> {
    let file = fs::File::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let decoder = GifDecoder::new(std::io::BufReader::new(file))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let frames = decoder
        .into_frames()
        .collect_frames()
        .map_err(|error| format!("{}: {error}", path.display()))?;

    if frames.is_empty() {
        return Err(format!("{}: no frames decoded", path.display()));
    }

    Ok(frames
        .into_iter()
        .map(|frame| {
            let delay: Duration = frame.delay().into();
            DecodedFrame {
                image: frame.into_buffer(),
                delay_ms: Some(delay.as_millis() as u32),
            }
        })
        .collect())
}

pub fn decode_png_folder(dir: &Path) -> Result<Vec<DecodedFrame>, String> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map_err(|error| format!("{}: {error}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("png"))
        .collect();
    paths.sort();

    if paths.is_empty() {
        return Err(format!("{}: no PNG frames found", dir.display()));
    }

    paths
        .into_iter()
        .map(|path| {
            let image = image::open(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?
                .to_rgba8();
            Ok(DecodedFrame { image, delay_ms: None })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Rgba};

    fn write_fixture_gif(path: &Path, colors: &[[u8; 4]]) {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for color in colors {
                let mut image = RgbaImage::new(2, 2);
                for pixel in image.pixels_mut() {
                    *pixel = Rgba(*color);
                }
                let frame = image::Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1));
                encoder.encode_frame(frame).expect("encode frame");
            }
        }
        fs::write(path, bytes).expect("write fixture gif");
    }

    #[test]
    fn decode_gif_reads_every_frame_with_its_delay() {
        let path = std::env::temp_dir().join("distract_decode_gif_test.gif");
        write_fixture_gif(&path, &[[255, 0, 0, 255], [0, 255, 0, 255]]);

        let frames = decode_gif(&path).expect("decode");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].delay_ms, Some(100));
        assert_eq!(frames[0].image.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert_eq!(frames[1].image.get_pixel(0, 0), &Rgba([0, 255, 0, 255]));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_gif_rejects_an_unreadable_path() {
        let result = decode_gif(Path::new("/does/not/exist.gif"));
        assert!(result.is_err());
    }

    #[test]
    fn decode_png_folder_sorts_by_filename_and_has_no_delay() {
        let dir = std::env::temp_dir().join("distract_decode_png_folder_test");
        fs::create_dir_all(&dir).expect("create fixture dir");

        for (index, color) in [[10u8, 10, 10, 255], [20, 20, 20, 255]].iter().enumerate() {
            let mut image = RgbaImage::new(1, 1);
            image.put_pixel(0, 0, Rgba(*color));
            image
                .save(dir.join(format!("{index:04}.png")))
                .expect("write fixture png");
        }

        let frames = decode_png_folder(&dir).expect("decode");

        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].delay_ms, None);
        assert_eq!(frames[0].image.get_pixel(0, 0), &Rgba([10, 10, 10, 255]));
        assert_eq!(frames[1].image.get_pixel(0, 0), &Rgba([20, 20, 20, 255]));

        fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Add `mod decode;` to `main.rs`**

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite decode::`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/src/bin/import_sprite/decode.rs engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): decode GIF and PNG-folder sources"
```

---

### Task 3: Background removal (corner flood-fill, soft alpha)

**Files:**
- Create: `engine/src/bin/import_sprite/background.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (add `mod background;`)

**Interfaces:**
- Consumes: nothing from earlier tasks (operates on a plain `&RgbaImage`).
- Produces: `pub fn remove_background(frame: &RgbaImage, tolerance: f32, feather: f32) -> RgbaImage` — Task 7 calls this once per decoded frame.

- [ ] **Step 1: Write `background.rs` with its tests**

```rust
use std::collections::VecDeque;

use image::{Rgba, RgbaImage};

pub fn remove_background(frame: &RgbaImage, tolerance: f32, feather: f32) -> RgbaImage {
    let (width, height) = frame.dimensions();
    let reference = corner_average(frame);

    let mut output = frame.clone();
    let mut visited = vec![false; (width * height) as usize];
    let mut queue = VecDeque::new();

    for &(x, y) in &corners(width, height) {
        let index = (y * width + x) as usize;
        if !visited[index] {
            visited[index] = true;
            queue.push_back((x, y));
        }
    }

    while let Some((x, y)) = queue.pop_front() {
        let pixel = *frame.get_pixel(x, y);
        let distance = color_distance(pixel, reference);
        let alpha = if distance <= tolerance {
            0.0
        } else {
            ((distance - tolerance) / feather).min(1.0)
        };
        output.put_pixel(
            x,
            y,
            Rgba([pixel[0], pixel[1], pixel[2], (alpha * 255.0).round() as u8]),
        );

        if distance > tolerance + feather {
            continue;
        }

        for (nx, ny) in neighbors(x, y, width, height) {
            let index = (ny * width + nx) as usize;
            if !visited[index] {
                visited[index] = true;
                queue.push_back((nx, ny));
            }
        }
    }

    output
}

fn corners(width: u32, height: u32) -> [(u32, u32); 4] {
    [(0, 0), (width - 1, 0), (0, height - 1), (width - 1, height - 1)]
}

fn corner_average(frame: &RgbaImage) -> [f32; 3] {
    let (width, height) = frame.dimensions();
    let mut sum = [0.0f32; 3];
    for &(x, y) in &corners(width, height) {
        let pixel = frame.get_pixel(x, y);
        sum[0] += pixel[0] as f32;
        sum[1] += pixel[1] as f32;
        sum[2] += pixel[2] as f32;
    }
    [sum[0] / 4.0, sum[1] / 4.0, sum[2] / 4.0]
}

fn color_distance(pixel: Rgba<u8>, reference: [f32; 3]) -> f32 {
    let dr = (pixel[0] as f32 - reference[0]) / 255.0;
    let dg = (pixel[1] as f32 - reference[1]) / 255.0;
    let db = (pixel[2] as f32 - reference[2]) / 255.0;
    (dr * dr + dg * dg + db * db).sqrt() / 3.0f32.sqrt()
}

fn neighbors(x: u32, y: u32, width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut result = Vec::with_capacity(4);
    if x > 0 {
        result.push((x - 1, y));
    }
    if x + 1 < width {
        result.push((x + 1, y));
    }
    if y > 0 {
        result.push((x, y - 1));
    }
    if y + 1 < height {
        result.push((x, y + 1));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uniform_background_becomes_fully_transparent_and_the_subject_stays_opaque() {
        let mut frame = RgbaImage::new(4, 4);
        for pixel in frame.pixels_mut() {
            *pixel = Rgba([10, 10, 10, 255]);
        }
        frame.put_pixel(2, 2, Rgba([255, 0, 0, 255]));

        let output = remove_background(&frame, 0.12, 0.04);

        assert_eq!(output.get_pixel(0, 0)[3], 0);
        assert_eq!(output.get_pixel(2, 2), &Rgba([255, 0, 0, 255]));
    }

    #[test]
    fn a_disconnected_background_colored_pixel_inside_the_subject_is_not_erased() {
        let mut frame = RgbaImage::new(5, 5);
        for pixel in frame.pixels_mut() {
            *pixel = Rgba([255, 0, 0, 255]);
        }
        for x in 0..5 {
            frame.put_pixel(x, 0, Rgba([10, 10, 10, 255]));
        }
        frame.put_pixel(2, 2, Rgba([10, 10, 10, 255]));

        let output = remove_background(&frame, 0.12, 0.04);

        assert_eq!(output.get_pixel(0, 0)[3], 0);
        assert_eq!(output.get_pixel(2, 2)[3], 255, "isolated same-colored pixel must stay opaque");
    }

    #[test]
    fn a_pixel_just_past_tolerance_gets_a_feathered_not_binary_alpha() {
        let mut frame = RgbaImage::new(3, 1);
        frame.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        frame.put_pixel(1, 0, Rgba([15, 15, 15, 255])); // small step past pure background
        frame.put_pixel(2, 0, Rgba([0, 0, 0, 255]));

        let output = remove_background(&frame, 0.0, 0.5);

        let middle_alpha = output.get_pixel(1, 0)[3];
        assert!(middle_alpha > 0 && middle_alpha < 255, "expected a feathered value, got {middle_alpha}");
    }
}
```

- [ ] **Step 2: Add `mod background;` to `main.rs`**

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite background::`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/src/bin/import_sprite/background.rs engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): corner flood-fill background removal with soft alpha"
```

---

### Task 4: Padding and grid packing

**Files:**
- Create: `engine/src/bin/import_sprite/pack.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (add `mod pack;`)

**Interfaces:**
- Consumes: `Vec<RgbaImage>` (the background-removed frames from Task 3).
- Produces: `pub fn pad_to_common_canvas(frames: &[RgbaImage]) -> (Vec<RgbaImage>, u32, u32)`, `pub fn grid_dimensions(frame_count: usize) -> (u32, u32)`, `pub fn pack_spritesheet(frames: &[RgbaImage], columns: u32, frame_width: u32, frame_height: u32) -> RgbaImage` — Task 7 calls all three by these names, in this order.

- [ ] **Step 1: Write `pack.rs` with its tests**

```rust
use image::{imageops, RgbaImage};

pub fn pad_to_common_canvas(frames: &[RgbaImage]) -> (Vec<RgbaImage>, u32, u32) {
    let canvas_width = frames.iter().map(RgbaImage::width).max().unwrap_or(0);
    let canvas_height = frames.iter().map(RgbaImage::height).max().unwrap_or(0);

    let padded = frames
        .iter()
        .map(|frame| {
            let mut canvas = RgbaImage::new(canvas_width, canvas_height);
            let x_offset = (canvas_width - frame.width()) / 2;
            let y_offset = canvas_height - frame.height();
            imageops::overlay(&mut canvas, frame, x_offset.into(), y_offset.into());
            canvas
        })
        .collect();

    (padded, canvas_width, canvas_height)
}

pub fn grid_dimensions(frame_count: usize) -> (u32, u32) {
    let columns = (frame_count as u32).clamp(1, 8);
    let rows = (frame_count as u32).div_ceil(columns);
    (columns, rows)
}

pub fn pack_spritesheet(frames: &[RgbaImage], columns: u32, frame_width: u32, frame_height: u32) -> RgbaImage {
    let rows = (frames.len() as u32).div_ceil(columns);
    let mut sheet = RgbaImage::new(columns * frame_width, rows * frame_height);

    for (index, frame) in frames.iter().enumerate() {
        let column = index as u32 % columns;
        let row = index as u32 / columns;
        imageops::overlay(&mut sheet, frame, (column * frame_width).into(), (row * frame_height).into());
    }

    sheet
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn padding_bottom_aligns_and_centers_a_narrower_shorter_frame() {
        let mut tall = RgbaImage::new(2, 4);
        tall.put_pixel(0, 3, Rgba([1, 1, 1, 255]));
        let narrow = RgbaImage::new(2, 2);

        let (padded, width, height) = pad_to_common_canvas(&[tall, narrow]);

        assert_eq!((width, height), (2, 4));
        assert_eq!(padded[0].get_pixel(0, 3), &Rgba([1, 1, 1, 255]));
        assert_eq!(padded[1].dimensions(), (2, 4));
        assert_eq!(padded[1].get_pixel(0, 3)[3], 0, "padding is transparent");
    }

    #[test]
    fn grid_dimensions_caps_columns_at_eight() {
        assert_eq!(grid_dimensions(32), (8, 4));
        assert_eq!(grid_dimensions(3), (3, 1));
        assert_eq!(grid_dimensions(1), (1, 1));
    }

    #[test]
    fn pack_spritesheet_places_frames_left_to_right_top_to_bottom() {
        let mut first = RgbaImage::new(2, 2);
        first.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        let mut second = RgbaImage::new(2, 2);
        second.put_pixel(0, 0, Rgba([0, 255, 0, 255]));

        let sheet = pack_spritesheet(&[first, second], 2, 2, 2);

        assert_eq!(sheet.dimensions(), (4, 2));
        assert_eq!(sheet.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        assert_eq!(sheet.get_pixel(2, 0), &Rgba([0, 255, 0, 255]));
    }
}
```

- [ ] **Step 2: Add `mod pack;` to `main.rs`**

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite pack::`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/src/bin/import_sprite/pack.rs engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): pad frames to a common canvas and pack a grid spritesheet"
```

---

### Task 5: The `.rgba` sidecar writer/reader

**Files:**
- Create: `engine/src/bin/import_sprite/rgba_sidecar.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (add `mod rgba_sidecar;`)

**Interfaces:**
- Consumes: `Vec<RgbaImage>` (the padded frames from Task 4) plus the common `frame_width`/`frame_height`.
- Produces: `pub fn write_rgba_sidecar(path: &Path, frame_width: u32, frame_height: u32, frames: &[RgbaImage]) -> Result<(), String>`, `pub fn read_rgba_sidecar(path: &Path) -> Result<(u32, u32, Vec<RgbaImage>), String>`. Task 7 calls the writer; Task 9's Lua reader must match this exact byte layout (spec § 2.5): magic `"DRGB"` (4 bytes), version `1u8`, `frame_width`/`frame_height`/`frame_count` as little-endian `u32`, then raw RGBA8 frame bytes concatenated.

- [ ] **Step 1: Write `rgba_sidecar.rs` with its tests**

```rust
use std::fs;
use std::path::Path;

use image::RgbaImage;

const MAGIC: &[u8; 4] = b"DRGB";
const VERSION: u8 = 1;
const HEADER_SIZE: usize = 17;

pub fn write_rgba_sidecar(
    path: &Path,
    frame_width: u32,
    frame_height: u32,
    frames: &[RgbaImage],
) -> Result<(), String> {
    let frame_byte_len = frame_width as usize * frame_height as usize * 4;
    let mut bytes = Vec::with_capacity(HEADER_SIZE + frames.len() * frame_byte_len);
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.extend_from_slice(&frame_width.to_le_bytes());
    bytes.extend_from_slice(&frame_height.to_le_bytes());
    bytes.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    for frame in frames {
        bytes.extend_from_slice(frame.as_raw());
    }

    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn read_rgba_sidecar(path: &Path) -> Result<(u32, u32, Vec<RgbaImage>), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;

    if bytes.len() < HEADER_SIZE {
        return Err(format!("{}: truncated header", path.display()));
    }
    if &bytes[0..4] != MAGIC {
        return Err(format!("{}: bad magic", path.display()));
    }
    if bytes[4] != VERSION {
        return Err(format!("{}: unsupported version {}", path.display(), bytes[4]));
    }

    let frame_width = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
    let frame_height = u32::from_le_bytes(bytes[9..13].try_into().unwrap());
    let frame_count = u32::from_le_bytes(bytes[13..17].try_into().unwrap());

    let frame_byte_len = (frame_width * frame_height * 4) as usize;
    let expected_len = HEADER_SIZE + frame_count as usize * frame_byte_len;
    if bytes.len() != expected_len {
        return Err(format!(
            "{}: declares {} frame bytes, has {}",
            path.display(),
            expected_len - HEADER_SIZE,
            bytes.len() - HEADER_SIZE
        ));
    }

    let mut frames = Vec::with_capacity(frame_count as usize);
    for index in 0..frame_count as usize {
        let start = HEADER_SIZE + index * frame_byte_len;
        let raw = bytes[start..start + frame_byte_len].to_vec();
        let image = RgbaImage::from_raw(frame_width, frame_height, raw)
            .ok_or_else(|| format!("{}: frame {index} has the wrong byte length", path.display()))?;
        frames.push(image);
    }

    Ok((frame_width, frame_height, frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn a_written_sidecar_reads_back_pixel_for_pixel() {
        let mut frame_one = RgbaImage::new(2, 2);
        frame_one.put_pixel(0, 0, Rgba([255, 0, 0, 200]));
        let mut frame_two = RgbaImage::new(2, 2);
        frame_two.put_pixel(1, 1, Rgba([0, 255, 0, 100]));
        let frames = vec![frame_one.clone(), frame_two.clone()];

        let path = std::env::temp_dir().join("distract_rgba_sidecar_round_trip_test.rgba");
        write_rgba_sidecar(&path, 2, 2, &frames).expect("write");
        let (width, height, read_frames) = read_rgba_sidecar(&path).expect("read");

        assert_eq!((width, height), (2, 2));
        assert_eq!(read_frames, frames);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_truncated_file_is_rejected_not_panicked_on() {
        let path = std::env::temp_dir().join("distract_rgba_sidecar_truncated_test.rgba");
        std::fs::write(&path, b"DRGB").expect("write fixture");

        assert!(read_rgba_sidecar(&path).is_err());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn bad_magic_is_rejected() {
        let path = std::env::temp_dir().join("distract_rgba_sidecar_bad_magic_test.rgba");
        std::fs::write(&path, [0u8; 20]).expect("write fixture");

        assert!(read_rgba_sidecar(&path).is_err());

        std::fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Add `mod rgba_sidecar;` to `main.rs`**

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite rgba_sidecar::`
Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/src/bin/import_sprite/rgba_sidecar.rs engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): write and read the .rgba sidecar format"
```

---

### Task 6: Manifest scaffold generator

**Files:**
- Create: `engine/src/bin/import_sprite/manifest_scaffold.rs`
- Modify: `engine/src/bin/import_sprite/main.rs` (add `mod manifest_scaffold;`)

**Interfaces:**
- Consumes: nothing from earlier tasks except the frame count and layout numbers Task 7 will pass in.
- Produces: `pub struct StateSpec { pub name: String, pub start: usize, pub end: usize }`, `pub fn parse_states_arg(raw: &str, total_frames: usize) -> Result<Vec<StateSpec>, String>`, `pub fn default_state(total_frames: usize) -> Vec<StateSpec>`, `pub struct ManifestParams<'a> { pub name: &'a str, pub sheet_path: &'a str, pub native_path: &'a str, pub frame_width: u32, pub frame_height: u32, pub columns: u32, pub rows: u32, pub states: &'a [StateSpec], pub fps: f32 }`, `pub fn render_manifest(params: &ManifestParams) -> String`.

- [ ] **Step 1: Write `manifest_scaffold.rs` with its tests**

```rust
pub struct StateSpec {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

pub fn parse_states_arg(raw: &str, total_frames: usize) -> Result<Vec<StateSpec>, String> {
    let mut states = Vec::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        let (name, range) = entry.split_once(':').ok_or_else(|| format!("'{entry}' is not name:start-end"))?;
        let (start_text, end_text) =
            range.split_once('-').ok_or_else(|| format!("'{range}' is not start-end"))?;
        let start: usize = start_text.trim().parse().map_err(|_| format!("'{start_text}' is not a number"))?;
        let end: usize = end_text.trim().parse().map_err(|_| format!("'{end_text}' is not a number"))?;
        if end < start || end >= total_frames {
            return Err(format!(
                "state '{name}' range {start}-{end} is out of bounds for {total_frames} frames"
            ));
        }
        states.push(StateSpec { name: name.to_string(), start, end });
    }
    if states.is_empty() {
        return Err("no states parsed".to_string());
    }
    Ok(states)
}

pub fn default_state(total_frames: usize) -> Vec<StateSpec> {
    vec![StateSpec { name: "default".to_string(), start: 0, end: total_frames - 1 }]
}

pub struct ManifestParams<'a> {
    pub name: &'a str,
    pub sheet_path: &'a str,
    pub native_path: &'a str,
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub states: &'a [StateSpec],
    pub fps: f32,
}

pub fn render_manifest(params: &ManifestParams) -> String {
    let initial_state = &params.states[0].name;
    let mut states_lua = String::new();
    for state in params.states {
        let frames: Vec<String> = (state.start..=state.end).map(|frame| frame.to_string()).collect();
        states_lua.push_str(&format!(
            "    {} = {{\n      animation = {{ frames = {{ {} }}, fps = {:.1}, loop_anim = true, flip_x = false }},\n      physics = {{ target_vx = 2.0, target_vy = 0.0, wrap_mode = \"wrap\" }}, -- placeholder: tune per asset\n      transitions = {{ on_event = {{}} }},\n    }},\n",
            state.name,
            frames.join(", "),
            params.fps,
        ));
    }

    format!(
        "local M = {{\n  name = \"{}\",\n  asset_type = \"sprite\",\n  spritesheet = {{\n    path = \"{}\",\n    native_path = \"{}\",\n    frame_width = {},\n    frame_height = {},\n    columns = {},\n    rows = {},\n  }},\n  anchor = \"bottom\",\n  initial_state = \"{}\",\n  locomotion = \"grounded\",\n  capabilities = {{ locomotion = {{ \"grounded\" }} }},\n  states = {{\n{}  }},\n}}\nreturn M\n",
        params.name,
        params.sheet_path,
        params.native_path,
        params.frame_width,
        params.frame_height,
        params.columns,
        params.rows,
        initial_state,
        states_lua,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_states_arg_reads_one_or_more_ranges() {
        let states = parse_states_arg("walk:0-31,idle:32-35", 36).expect("parse");
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].name, "walk");
        assert_eq!(states[0].start, 0);
        assert_eq!(states[0].end, 31);
        assert_eq!(states[1].name, "idle");
    }

    #[test]
    fn parse_states_arg_rejects_a_range_past_the_frame_count() {
        assert!(parse_states_arg("walk:0-99", 10).is_err());
    }

    #[test]
    fn default_state_covers_every_frame() {
        let states = default_state(5);
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].name, "default");
        assert_eq!((states[0].start, states[0].end), (0, 4));
    }

    #[test]
    fn render_manifest_includes_every_state_and_the_native_path() {
        let states = vec![StateSpec { name: "walk".to_string(), start: 0, end: 1 }];
        let params = ManifestParams {
            name: "cat_walking",
            sheet_path: "assets/cat_walking/cat_walking_sheet.png",
            native_path: "assets/cat_walking/cat_walking_frames.rgba",
            frame_width: 128,
            frame_height: 72,
            columns: 8,
            rows: 4,
            states: &states,
            fps: 12.0,
        };

        let manifest = render_manifest(&params);

        assert!(manifest.contains("name = \"cat_walking\""));
        assert!(manifest.contains("native_path = \"assets/cat_walking/cat_walking_frames.rgba\""));
        assert!(manifest.contains("walk = {"));
        assert!(manifest.contains("frames = { 0, 1 }"));
        assert!(manifest.contains("initial_state = \"walk\""));
    }
}
```

- [ ] **Step 2: Add `mod manifest_scaffold;` to `main.rs`**

- [ ] **Step 3: Run the tests**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite manifest_scaffold::`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add engine/src/bin/import_sprite/manifest_scaffold.rs engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): generate a manifest scaffold from parsed states"
```

---

### Task 7: Wire `run()` end-to-end and add the CLI integration test

**Files:**
- Modify: `engine/src/bin/import_sprite/main.rs`

**Interfaces:**
- Consumes: every function produced by Tasks 2–6, by the exact names listed there.
- Produces: `fn run(args: Args) -> Result<(), String>`, called from `main()`.

- [ ] **Step 1: Replace `main()`'s placeholder body with the full pipeline, and add `run`/`average_fps`**

```rust
fn average_fps(frames: &[decode::DecodedFrame]) -> f32 {
    let delays: Vec<u32> = frames.iter().filter_map(|frame| frame.delay_ms).collect();
    if delays.is_empty() {
        return 12.0;
    }
    let average_ms = delays.iter().sum::<u32>() as f32 / delays.len() as f32;
    1000.0 / average_ms
}

fn run(args: Args) -> Result<(), String> {
    let decoded: Vec<decode::DecodedFrame> = match (&args.gif, &args.frames_dir) {
        (Some(path), None) => decode::decode_gif(path)?,
        (None, Some(dir)) => decode::decode_png_folder(dir)?,
        _ => unreachable!("parse_args_from already validated exactly one source"),
    };

    let cutout: Vec<_> = decoded
        .iter()
        .map(|frame| background::remove_background(&frame.image, args.bg_tolerance, FEATHER_BAND))
        .collect();

    let (padded, frame_width, frame_height) = pack::pad_to_common_canvas(&cutout);
    let (columns, rows) = pack::grid_dimensions(padded.len());
    let sheet = pack::pack_spritesheet(&padded, columns, frame_width, frame_height);

    std::fs::create_dir_all(&args.out).map_err(|error| format!("{}: {error}", args.out.display()))?;

    let sheet_path = args.out.join(format!("{}_sheet.png", args.name));
    sheet.save(&sheet_path).map_err(|error| format!("{}: {error}", sheet_path.display()))?;

    let native_path = args.out.join(format!("{}_frames.rgba", args.name));
    rgba_sidecar::write_rgba_sidecar(&native_path, frame_width, frame_height, &padded)?;

    let states = match &args.states {
        Some(raw) => manifest_scaffold::parse_states_arg(raw, padded.len())?,
        None => manifest_scaffold::default_state(padded.len()),
    };

    let fps = average_fps(&decoded);
    let sheet_path_text = sheet_path.display().to_string();
    let native_path_text = native_path.display().to_string();
    let params = manifest_scaffold::ManifestParams {
        name: &args.name,
        sheet_path: &sheet_path_text,
        native_path: &native_path_text,
        frame_width,
        frame_height,
        columns,
        rows,
        states: &states,
        fps,
    };
    let manifest_text = manifest_scaffold::render_manifest(&params);

    if let Some(parent) = args.manifest_out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    std::fs::write(&args.manifest_out, manifest_text)
        .map_err(|error| format!("{}: {error}", args.manifest_out.display()))?;

    eprintln!(
        "import_sprite: wrote {} frames to {} and {}, manifest at {}",
        padded.len(),
        sheet_path.display(),
        native_path.display(),
        args.manifest_out.display()
    );
    Ok(())
}
```

Replace the `main()` body (from Task 1) with:

```rust
const FEATHER_BAND: f32 = 0.04;

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("import_sprite: {message}");
            process::exit(1);
        }
    };

    if let Err(message) = run(args) {
        eprintln!("import_sprite: {message}");
        process::exit(1);
    }
}
```

(Move the `FEATHER_BAND` constant next to `DEFAULT_BG_TOLERANCE` at the top of the file.)

- [ ] **Step 2: Add the integration test in `main.rs`'s own test module**

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Rgba, RgbaImage};

    fn write_fixture_gif(path: &std::path::Path) {
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for color in [[200u8, 50, 50, 255], [50, 200, 50, 255], [50, 50, 200, 255]] {
                let mut image = RgbaImage::new(4, 4);
                for pixel in image.pixels_mut() {
                    *pixel = Rgba(color);
                }
                let frame = image::Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1));
                encoder.encode_frame(frame).expect("encode frame");
            }
        }
        std::fs::write(path, bytes).expect("write fixture gif");
    }

    #[test]
    fn a_full_run_produces_a_sheet_a_sidecar_and_a_manifest() {
        let gif_path = std::env::temp_dir().join("distract_import_sprite_cli_test.gif");
        write_fixture_gif(&gif_path);
        let out_dir = std::env::temp_dir().join("distract_import_sprite_cli_test_out");
        let manifest_path = std::env::temp_dir().join("distract_import_sprite_cli_test.lua");
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();

        let args = Args {
            gif: Some(gif_path.clone()),
            frames_dir: None,
            name: "cli_test_asset".to_string(),
            states: None,
            out: out_dir.clone(),
            manifest_out: manifest_path.clone(),
            bg_tolerance: DEFAULT_BG_TOLERANCE,
        };

        run(args).expect("run");

        let sheet_path = out_dir.join("cli_test_asset_sheet.png");
        let native_path = out_dir.join("cli_test_asset_frames.rgba");
        assert!(sheet_path.exists());
        assert!(native_path.exists());
        assert!(manifest_path.exists());

        let (frame_width, frame_height, frames) =
            rgba_sidecar::read_rgba_sidecar(&native_path).expect("read sidecar");
        assert_eq!(frames.len(), 3);
        assert_eq!((frame_width, frame_height), (4, 4));

        let manifest_text = std::fs::read_to_string(&manifest_path).expect("read manifest");
        assert!(manifest_text.contains("name = \"cli_test_asset\""));
        assert!(manifest_text.contains("default = {"));

        std::fs::remove_file(&gif_path).ok();
        std::fs::remove_dir_all(&out_dir).ok();
        std::fs::remove_file(&manifest_path).ok();
    }
}
```

- [ ] **Step 3: Run every test in the binary**

Run: `cargo test --manifest-path engine/Cargo.toml --bin import_sprite`
Expected: all tests from Tasks 1–7 pass (17 total: 3+3+3+3+3+4+1... count will match whatever accumulated; the key check is 0 failures).

- [ ] **Step 4: Run the full existing engine suite to confirm nothing else broke**

Run: `cargo test --manifest-path engine/Cargo.toml --all-targets --all-features`
Expected: 137 previously-passing tests still pass, plus the new `import_sprite` tests.

- [ ] **Step 5: `cargo fmt` and `cargo clippy`**

Run: `cargo fmt --manifest-path engine/Cargo.toml --all -- --check && cargo clippy --manifest-path engine/Cargo.toml --all-targets --all-features -- -D warnings`
Expected: both clean. Fix anything clippy flags before committing.

- [ ] **Step 6: Commit**

```bash
git add engine/src/bin/import_sprite/main.rs
git commit -m "feat(import_sprite): wire the pipeline end-to-end with a CLI integration test"
```

---

## Part B — Runtime wiring (Lua)

### Task 8: `native_resolution` capability field

**Corrected understanding — read before starting:** kitty is **not** a
hardcoded built-in the way halfblock/overlay are, and it must stay that way.
`lua/distract/kitty/init.lua:68-80` (`M.setup`) registers it dynamically,
*conditionally*, only after `M.is_available()` (`kitty/init.lua:53-59`)
confirms `termguicolors` and a real protocol probe both succeed
(`kitty/detect.lua`). Until that happens, `BUILT_IN_SUBSTITUTIONS`' `kitty =
{ to = M.HALFBLOCK, why = "...not implemented yet" }` entry in `backends.lua`
is the **correct, deliberate** fallback — not stale text, not a gap to close.
`tests/backends_spec.lua:96-107` already exercises exactly this self-
registration flow. **Do not add `kitty` to `BUILT_IN_CAPABILITIES`,
`BUILT_IN_ALIASES`, or remove it from `BUILT_IN_SUBSTITUTIONS`.** The only
change `backends.lua` needs is the new capability field, applied to the two
backends that really are unconditional built-ins.

**Files:**
- Modify: `lua/distract/backends.lua`
- Modify: `lua/distract/kitty/init.lua` (add the field to its own `M.CAPABILITIES`)
- Modify: `tests/backends_spec.lua` (new tests + fix one now-invalid existing call)

**Interfaces:**
- Produces: `DistractBackendCapabilities.native_resolution: boolean`, required by `M.register`'s validation from now on.

- [ ] **Step 1: Write the failing tests in `tests/backends_spec.lua`**

Add near the existing capability tests (inside `describe("distract.backends"`):

```lua
it("halfblock and overlay report native_resolution explicitly", function()
  local backends = require("distract.backends")
  assert.is_false(backends.capabilities(backends.HALFBLOCK).native_resolution)
  assert.is_true(backends.capabilities(backends.OVERLAY).native_resolution)
end)

it("register requires native_resolution alongside scale and alpha", function()
  local backends = require("distract.backends")
  assert.is_false(pcall(backends.register, "missing_field", { scale = true, alpha = "pixel" }))
  backends.reset()
end)
```

Fix the existing test at `tests/backends_spec.lua:96-107` (`"lets a backend register itself out of being a substitution"`), which currently calls `backends.register("kitty", { scale = true, alpha = "pixel" }, { "ghostty" })` — this will start failing the moment Step 2's stricter validation lands, not because the *feature* it tests changed, but because its fixture capability table is now incomplete:

```lua
it("lets a backend register itself out of being a substitution", function()
  backends.register("kitty", { scale = true, alpha = "pixel", native_resolution = true }, { "ghostty" })

  assert.are_equal("kitty", backends.resolve("kitty", true))
  assert.are_equal("kitty", backends.resolve("ghostty", true))
  assert.is_true(backends.supports_parallax("kitty"))
  assert.is_true(vim.tbl_contains(backends.names(), "kitty"))

  backends.reset()
  assert.are_equal("halfblock", backends.resolve("kitty", true))
end)
```

- [ ] **Step 2: Run the tests to verify the new ones fail and the fixed one still passes**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | grep -B1 -A3 "native_resolution\|register itself"`
Expected: the two new tests FAIL (`native_resolution` is `nil`, `pcall` succeeds when it shouldn't); the edited existing test still PASSES (its fixture already has the field).

- [ ] **Step 3: Implement in `backends.lua`**

Update the class annotation:

```lua
---@class DistractBackendCapabilities
---@field scale boolean
---@field alpha "cell"|"pixel"
---@field native_resolution boolean
```

Update the two unconditional built-ins only:

```lua
---@type table<string, DistractBackendCapabilities>
local BUILT_IN_CAPABILITIES = {
  [M.HALFBLOCK] = { scale = false, alpha = "cell", native_resolution = false },
  [M.OVERLAY] = { scale = true, alpha = "pixel", native_resolution = true },
}
```

`BUILT_IN_ALIASES` and `BUILT_IN_SUBSTITUTIONS` are **not touched** — kitty stays a substitution until it registers itself, exactly as today.

Update `M.register`'s validation to require the new field for *every* caller, built-in or not:

```lua
if
  type(caps) ~= "table"
  or type(caps.scale) ~= "boolean"
  or caps.alpha == nil
  or type(caps.native_resolution) ~= "boolean"
then
  error("distract.backends.register: capabilities need `scale`, `alpha`, and `native_resolution`")
end

capabilities[name] = { scale = caps.scale, alpha = caps.alpha, native_resolution = caps.native_resolution }
```

- [ ] **Step 4: Implement in `kitty/init.lua`**

The real caller that self-registers kitty needs its own capability table updated — this is the one line that actually makes kitty report `native_resolution = true` once it activates for real:

```lua
---@type DistractBackendCapabilities
M.CAPABILITIES = { scale = true, alpha = "pixel", native_resolution = true }
```

- [ ] **Step 5: Run the tests to verify everything passes**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | tail -20`
Expected: all pass, including the 2 new tests and the fixed existing one. `tests/kitty_spec.lua`'s 40 tests are unaffected by this task (they don't touch `backends.lua`).

- [ ] **Step 6: `stylua` and `luacheck`**

Run: `stylua --check lua plugin tests && luacheck lua/distract/backends.lua lua/distract/kitty/init.lua tests/backends_spec.lua`
(If `luacheck` fails with a Lua-version error unrelated to these files, that's a pre-existing environment issue — confirm by running `luacheck` against a file you didn't touch; do not attempt to fix system Lua tooling as part of this task unless the failure is new.)

- [ ] **Step 7: Commit**

```bash
git add lua/distract/backends.lua lua/distract/kitty/init.lua tests/backends_spec.lua
git commit -m "feat(backends): add native_resolution to the capability schema"
```

---

### Task 9: `native_sprite.lua` — the `.rgba` sidecar reader

**Files:**
- Create: `lua/distract/native_sprite.lua`
- Create: `tests/native_sprite_spec.lua`

**Interfaces:**
- Consumes: nothing from earlier Lua tasks.
- Produces: `M.source_of(manifest) -> table|nil` (shape `{ native_path = string }`), `M.same_source(left, right) -> boolean`, `M.load(path) -> table[]|nil, string|nil` (per-frame pixel matrix, `nil, err` on failure — never throws for file/format problems). Task 10 calls `native_sprite.source_of`, `native_sprite.same_source`, and `native_sprite.load` by these exact names.

- [ ] **Step 1: Write the failing tests in `tests/native_sprite_spec.lua`**

```lua
local native_sprite = require("distract.native_sprite")

--- Builds a minimal valid .rgba buffer: header + one 1x1 opaque red pixel.
local function build_fixture(path)
  local header = "DRGB" .. string.char(1) -- magic + version
  local function u32(n)
    return string.char(n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256)
  end
  local body = header .. u32(1) .. u32(1) .. u32(1) .. string.char(255, 0, 0, 255)
  local file = io.open(path, "wb")
  file:write(body)
  file:close()
end

describe("distract.native_sprite", function()
  local fixture_path = vim.fn.tempname() .. ".rgba"

  after_each(function()
    os.remove(fixture_path)
  end)

  it("source_of returns nil when the manifest has no native_path", function()
    assert.is_nil(native_sprite.source_of({ spritesheet = { path = "x.png" } }))
    assert.is_nil(native_sprite.source_of(nil))
  end)

  it("source_of returns the native_path when present", function()
    local source = native_sprite.source_of({ spritesheet = { native_path = "assets/x/x.rgba" } })
    assert.are.same({ native_path = "assets/x/x.rgba" }, source)
  end)

  it("same_source compares by native_path, nil-safe", function()
    assert.is_true(native_sprite.same_source(nil, nil))
    assert.is_false(native_sprite.same_source(nil, { native_path = "a" }))
    assert.is_true(native_sprite.same_source({ native_path = "a" }, { native_path = "a" }))
    assert.is_false(native_sprite.same_source({ native_path = "a" }, { native_path = "b" }))
  end)

  it("load decodes a valid fixture into a one-pixel frame", function()
    build_fixture(fixture_path)

    local frames, err = native_sprite.load(fixture_path)

    assert.is_nil(err)
    assert.are.equal(1, #frames)
    assert.are.same({ 255, 0, 0 }, frames[1][1][1])
  end)

  it("load returns nil, err for a missing file instead of throwing", function()
    local frames, err = native_sprite.load("/does/not/exist.rgba")
    assert.is_nil(frames)
    assert.is_not_nil(err)
  end)

  it("load returns nil, err for bad magic instead of throwing", function()
    local file = io.open(fixture_path, "wb")
    file:write("XXXX" .. string.char(1, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 255, 0, 0, 255))
    file:close()

    local frames, err = native_sprite.load(fixture_path)

    assert.is_nil(frames)
    assert.is_not_nil(err)
  end)

  it("load caches by path so a second call does not re-read the file", function()
    build_fixture(fixture_path)
    local first = native_sprite.load(fixture_path)
    os.remove(fixture_path) -- if load() re-reads, this proves it by failing
    local second = native_sprite.load(fixture_path)
    assert.are.equal(first, second)
  end)
end)
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | grep -i "native_sprite"`
Expected: FAIL — `module 'distract.native_sprite' not found`.

- [ ] **Step 3: Implement `lua/distract/native_sprite.lua`**

```lua
--- Reading the `.rgba` sidecar an asset's spritesheet may declare.
---
--- Deliberately not a PNG decoder: the terminal backends are meant to work
--- with zero dependency on the compiled Rust engine, and this repo has no
--- PNG/zlib parser to reuse. The `.rgba` format is a fixed, uncompressed
--- header + raw pixel dump (see `engine/src/bin/import_sprite/rgba_sidecar.rs`
--- for the writer this must stay byte-compatible with) specifically so this
--- reader never needs to be more than byte arithmetic.

local M = {}

local HEADER_SIZE = 17
local MAGIC = "DRGB"
local VERSION = 1

local cache = {}

---@class DistractNativeSpriteSource
---@field native_path string as written in the manifest

---@param manifest table|nil
---@return DistractNativeSpriteSource|nil
function M.source_of(manifest)
  local spritesheet = manifest and manifest.spritesheet
  local native_path = spritesheet and spritesheet.native_path
  if not native_path then
    return nil
  end
  return { native_path = native_path }
end

---@param left DistractNativeSpriteSource|nil
---@param right DistractNativeSpriteSource|nil
---@return boolean
function M.same_source(left, right)
  if left == nil or right == nil then
    return left == right
  end
  return left.native_path == right.native_path
end

local function read_u32_le(bytes, offset)
  local b1, b2, b3, b4 = bytes:byte(offset, offset + 3)
  return b1 + b2 * 256 + b3 * 65536 + b4 * 16777216
end

local function decode_frames(bytes, frame_width, frame_height, frame_count)
  local frames = {}
  local cursor = HEADER_SIZE + 1
  for frame_index = 1, frame_count do
    local rows = {}
    for y = 1, frame_height do
      local row = {}
      for x = 1, frame_width do
        local red, green, blue, alpha = bytes:byte(cursor, cursor + 3)
        row[x] = alpha == 0 and false or { red, green, blue }
        cursor = cursor + 4
      end
      rows[y] = row
    end
    frames[frame_index] = rows
  end
  return frames
end

---@param path string
---@return table[]|nil frames
---@return string|nil error_message
function M.load(path)
  if cache[path] then
    return cache[path]
  end

  local file = io.open(path, "rb")
  if not file then
    return nil, string.format("cannot open '%s'", path)
  end
  local bytes = file:read("*a")
  file:close()

  if #bytes < HEADER_SIZE then
    return nil, string.format("'%s' is truncated (missing header)", path)
  end
  if bytes:sub(1, 4) ~= MAGIC then
    return nil, string.format("'%s' has bad magic", path)
  end
  local version = bytes:byte(5)
  if version ~= VERSION then
    return nil, string.format("'%s' has unsupported version %d", path, version)
  end

  local frame_width = read_u32_le(bytes, 6)
  local frame_height = read_u32_le(bytes, 10)
  local frame_count = read_u32_le(bytes, 14)
  local frame_byte_len = frame_width * frame_height * 4
  local expected_size = HEADER_SIZE + frame_count * frame_byte_len
  if #bytes ~= expected_size then
    return nil,
      string.format(
        "'%s' declares %d bytes of frame data, has %d",
        path,
        expected_size - HEADER_SIZE,
        #bytes - HEADER_SIZE
      )
  end

  local frames = decode_frames(bytes, frame_width, frame_height, frame_count)
  cache[path] = frames
  return frames
end

return M
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | grep -B1 -A1 "native_sprite\|Test Summary"`
Expected: all 7 tests pass.

- [ ] **Step 5: `stylua` and `luacheck`**

Run: `stylua --check lua/distract/native_sprite.lua tests/native_sprite_spec.lua && luacheck lua/distract/native_sprite.lua tests/native_sprite_spec.lua`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add lua/distract/native_sprite.lua tests/native_sprite_spec.lua
git commit -m "feat(native_sprite): read the .rgba sidecar format"
```

---

### Task 10: `sprite_sources.lua` — the fourth source, resolved ahead of the shared cache

**Files:**
- Modify: `lua/distract/sprite_sources.lua`
- Modify: `tests/sprite_assets_spec.lua`

**Interfaces:**
- Consumes: `native_sprite.source_of`, `native_sprite.same_source`, `native_sprite.load` (Task 9).
- Produces: `M.get_pixel_frames(asset_name, opts)` — `opts` is optional; `opts.native_resolution` is the new second parameter every call site (Task 11) will pass. `M.bind_manifest(asset_name, manifest)` keeps its existing signature but now also resolves native sources internally.

- [ ] **Step 1: Write the failing tests in `tests/sprite_assets_spec.lua`**

Add near the existing `bind_manifest`/`get_pixel_frames` tests:

```lua
it("get_pixel_frames ignores native_resolution when the manifest has no native_path", function()
  local sources = require("distract.sprite_sources")
  sources.bind_manifest("native_test_no_native", { spritesheet = { path = "x.gif" } })
  local without_opts = sources.get_pixel_frames("native_test_no_native")
  local with_native = sources.get_pixel_frames("native_test_no_native", { native_resolution = true })
  assert.are.equal(without_opts, with_native)
  sources.unbind_manifest("native_test_no_native")
end)

it("get_pixel_frames returns native frames only when native_resolution is requested", function()
  local sources = require("distract.sprite_sources")
  local native_sprite = require("distract.native_sprite")

  local fixture_path = vim.fn.tempname() .. ".rgba"
  local function u32(n)
    return string.char(n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256)
  end
  local file = io.open(fixture_path, "wb")
  file:write("DRGB" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(9, 9, 9, 255))
  file:close()

  sources.bind_manifest("native_test_with_native", { spritesheet = { path = "x.gif", native_path = fixture_path } })

  local native_frames = sources.get_pixel_frames("native_test_with_native", { native_resolution = true })
  local standard_frames = sources.get_pixel_frames("native_test_with_native")

  assert.are.same({ 9, 9, 9 }, native_frames[1][1][1])
  assert.are_not.equal(native_frames, standard_frames)

  sources.unbind_manifest("native_test_with_native")
  os.remove(fixture_path)
  native_sprite = nil -- luacheck: ignore
end)

it("halfblock's own request (native_resolution omitted) never sees native frames even when a native_path exists", function()
  -- Regression guard: the two backends must never leak into each other via
  -- the shared per-asset-name sprite_cache.
  local sources = require("distract.sprite_sources")
  local fixture_path = vim.fn.tempname() .. ".rgba"
  local function u32(n)
    return string.char(n % 256, math.floor(n / 256) % 256, math.floor(n / 65536) % 256, math.floor(n / 16777216) % 256)
  end
  local file = io.open(fixture_path, "wb")
  file:write("DRGB" .. string.char(1) .. u32(1) .. u32(1) .. u32(1) .. string.char(9, 9, 9, 255))
  file:close()

  sources.bind_manifest("native_test_order", { spritesheet = { path = "x.gif", native_path = fixture_path } })

  -- Ask for native FIRST, then ask the "halfblock" way -- order must not matter.
  sources.get_pixel_frames("native_test_order", { native_resolution = true })
  local halfblock_frames = sources.get_pixel_frames("native_test_order")

  assert.is_not.same({ 9, 9, 9 }, halfblock_frames[1] and halfblock_frames[1][1])

  sources.unbind_manifest("native_test_order")
  os.remove(fixture_path)
end)
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | grep -B1 -A3 "native_resolution\|native frames"`
Expected: FAIL — `get_pixel_frames` currently takes only one argument and ignores a second.

- [ ] **Step 3: Implement in `sprite_sources.lua`**

Add the require near the top, alongside `gif_sprite`:

```lua
local native_sprite = require("distract.native_sprite")
```

Add a parallel source table near `gif_sources`:

```lua
--- `.rgba` sidecars an asset's manifest points at, for the backends that can
--- show real resolution. Kept separate from `gif_sources` and, critically,
--- from `sprite_cache`: two backends can request different resolutions for
--- the same asset, and a cache keyed only by asset name cannot represent
--- that without leaking one backend's request into the other's.
---@type table<string, DistractNativeSpriteSource>
local native_sources = {}
```

Add a warn-once table and warning function next to `decode_warned`/`warn_decode_failure`:

```lua
local native_warned = {}

local function warn_native_failure(asset_name, native_source, error_message)
  if native_warned[asset_name] then
    return
  end
  native_warned[asset_name] = true
  vim.notify(
    string.format(
      "[Distract] Could not read native sprite '%s' for asset '%s': %s",
      native_source.native_path,
      asset_name,
      error_message
    ),
    vim.log.levels.WARN
  )
end
```

Replace `M.bind_manifest` to resolve both sources:

```lua
function M.bind_manifest(asset_name, manifest)
  local source = gif_sprite.source_of(manifest)
  local native_source = native_sprite.source_of(manifest)
  local changed = false

  if not gif_sprite.same_source(gif_sources[asset_name], source) then
    gif_sources[asset_name] = source
    decode_warned[asset_name] = nil
    changed = true
  end

  if not native_sprite.same_source(native_sources[asset_name], native_source) then
    native_sources[asset_name] = native_source
    native_warned[asset_name] = nil
    changed = true
  end

  if changed then
    announce_change(asset_name)
  end
end
```

Replace `M.get_pixel_frames`:

```lua
--- Frame matrices for an asset. Unknown assets fall back to the cat.
--- Draws the asset on first call.
---@param asset_name string
---@param opts table|nil `{ native_resolution = boolean }` -- the caller's
---  backend capability. When true and the asset's manifest declared a
---  `.rgba` sidecar, native-resolution frames are returned instead, resolved
---  ahead of (never inside) the cache below.
function M.get_pixel_frames(asset_name, opts)
  if opts and opts.native_resolution then
    local native_source = native_sources[asset_name]
    if native_source then
      local frames, err = native_sprite.load(native_source.native_path)
      if frames then
        return frames
      end
      warn_native_failure(asset_name, native_source, err)
    end
  end

  local sprite = load_sprite(asset_name)
  if type(sprite.frames) == "function" then
    return sprite.frames()
  end
  return sprite.frames
end
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | tail -20`
Expected: all pass, including the 3 new ones.

- [ ] **Step 5: `stylua` and `luacheck`**

Run: `stylua --check lua/distract/sprite_sources.lua tests/sprite_assets_spec.lua && luacheck lua/distract/sprite_sources.lua tests/sprite_assets_spec.lua`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add lua/distract/sprite_sources.lua tests/sprite_assets_spec.lua
git commit -m "feat(sprite_sources): resolve native-resolution frames ahead of the shared sprite cache"
```

---

### Task 11: Thread backend capability through the four call sites

**Files:**
- Modify: `lua/distract/renderer.lua:434`
- Modify: `lua/distract/terminal_sprites.lua:179`
- Modify: `lua/distract/kitty/renderer.lua:169`
- Modify: `lua/distract/kitty/frames.lua:110`
- Modify: `tests/kitty_spec.lua` (characterization test)

**Interfaces:**
- Consumes: `sprite_sources.get_pixel_frames(asset_name, opts)` (Task 10).
- Produces: nothing new downstream — this task only changes what these four call sites pass.

**Note on kitty's two call sites:** do **not** reach for `backends.capabilities("kitty")` here. `kitty/init.lua` already `require`s `kitty/renderer.lua` (`kitty/init.lua:15`), so `kitty/renderer.lua`/`kitty/frames.lua` requiring `kitty/init.lua` back would be circular. More fundamentally, these two files *are* the kitty backend's own internals — they don't need to ask a registry what they are. Pass the literal `{ native_resolution = true }`, exactly parallel to how halfblock's own two call sites pass the literal `{ native_resolution = false }` in Step 2 below.

- [ ] **Step 1: Read each call site's surrounding function to find how it already knows which backend it is**

Before editing, run:
```bash
sed -n '420,440p' lua/distract/renderer.lua
sed -n '170,185p' lua/distract/terminal_sprites.lua
sed -n '160,175p' lua/distract/kitty/renderer.lua
sed -n '100,125p' lua/distract/kitty/frames.lua
```
Each module either already has a fixed, known backend identity (halfblock for `renderer.lua`/`terminal_sprites.lua`, kitty for `kitty/renderer.lua`/`kitty/frames.lua`) or reads it from a local constant/require. Confirm which, since the exact edit at each site depends on it.

- [ ] **Step 2: Update `renderer.lua:434` and `terminal_sprites.lua:179` (halfblock — always `native_resolution = false`)**

At `renderer.lua:434`, change:
```lua
local frame_count = #sprites.get_pixel_frames(entity.asset_name)
```
to:
```lua
local frame_count = #sprites.get_pixel_frames(entity.asset_name, { native_resolution = false })
```

At `terminal_sprites.lua:179`, change:
```lua
local frames = M.get_pixel_frames(asset_name)
```
to:
```lua
local frames = M.get_pixel_frames(asset_name, { native_resolution = false })
```

- [ ] **Step 3: Update `kitty/renderer.lua:169` and `kitty/frames.lua:110` (kitty — always `native_resolution = true`)**

At `kitty/renderer.lua:169`, change:
```lua
local frame_count = #sprites.get_pixel_frames(entity.asset_name)
```
to:
```lua
local frame_count = #sprites.get_pixel_frames(entity.asset_name, { native_resolution = true })
```

At `kitty/frames.lua:110`, change:
```lua
local pixel_frames = sprites.get_pixel_frames(asset_name)
```
to:
```lua
local pixel_frames = sprites.get_pixel_frames(asset_name, { native_resolution = true })
```

- [ ] **Step 4: Write the characterization test for `kitty/frames.lua` with a native-resolution-sized frame (spec § 3.4/§ 5)**

Add to `tests/kitty_spec.lua`, near existing `frames`/`describe` tests:

```lua
it("encodes and places a native-resolution frame the same way as a tiny one", function()
  -- This pins today's assumption that kitty/frames.lua and protocol.lua are
  -- resolution-agnostic (spec: docs/superpowers/specs/2026-08-16-sprite-import-pipeline-design.md
  -- section 3.4). If this test needs a code change to pass, protocol.lua's
  -- placement math assumed a small frame somewhere -- fix it there, not here.
  --
  -- `kitty/frames.lua:97`'s M.describe(asset_name, frame_idx, flip_x) takes a
  -- 1-based frame_idx and calls sprites.get_pixel_frames(asset_name) itself
  -- (that call is exactly what Step 3 above changes to pass a capability
  -- table) -- this test exercises the whole path, not a mock.
  local frames = require("distract.kitty.frames")
  local sources = require("distract.sprite_sources")

  -- 24x16, well above the ~2-pixel procedural test fixtures elsewhere in this
  -- spec file, to actually exercise a "larger than the tiny grid" frame.
  local big_row = {}
  for column = 1, 24 do
    big_row[column] = { 10, 20, 30 }
  end
  local big_matrix = {}
  for row = 1, 16 do
    big_matrix[row] = vim.deepcopy(big_row)
  end

  sources.register("native_res_characterization_test", {
    frames = { big_matrix },
    layout = { idle = { 0 } },
    width = 24,
    height = 16,
  })

  local ok, described = pcall(frames.describe, "native_res_characterization_test", 1, false)

  assert.is_true(ok, tostring(described))
  assert.is_not_nil(described)
  assert.are.equal(24, described.pixel_w)
end)
```

(`sprite_sources.lua` has no unregister/reset for `registered` — `M.register(name, nil)` would itself error, since it requires a table. The asset name is unique to this test and simply stays registered for the rest of the process, matching how other fixture assets in this test suite are already handled — see the "tiny" fixture asset in `tests/sprite_assets_spec.lua`.)

- [ ] **Step 5: Run the full Lua suite**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua 2>&1 | tail -20`
Expected: all pass. If the characterization test fails, that means `kitty/frames.lua` or `protocol.lua` genuinely assumed a small frame somewhere — stop, report exactly what broke, and treat fixing it as new work (the spec flagged this as the one open question in this design).

- [ ] **Step 6: `stylua` and `luacheck`**

Run: `stylua --check lua plugin tests && luacheck lua/distract/renderer.lua lua/distract/terminal_sprites.lua lua/distract/kitty/renderer.lua lua/distract/kitty/frames.lua tests/kitty_spec.lua`

- [ ] **Step 7: Commit**

```bash
git add lua/distract/renderer.lua lua/distract/terminal_sprites.lua lua/distract/kitty/renderer.lua lua/distract/kitty/frames.lua tests/kitty_spec.lua
git commit -m "feat(kitty): request native-resolution frames via backend capability"
```

---

## Part C — Validation

### Task 12: Regenerate `cat_walking`'s assets through the new pipeline and verify full green

**Files:**
- Modify: `lua/distract/manifests/cat_walking.lua` (regenerated by the tool, then hand-reviewed)
- No new source files.

**Interfaces:**
- Consumes: the finished `import_sprite` binary (Part A) and the finished runtime wiring (Part B).

- [ ] **Step 1: Locate or recreate a source GIF for `cat_walking`**

`assets/cat_walking_1.gif` and `assets/cat_walking_2.gif` already exist in the repo (referenced in HANDOFF.md). Use one as the `--gif` input.

- [ ] **Step 2: Run the CLI from the repository root**

```bash
cargo run --manifest-path engine/Cargo.toml --bin import_sprite -- \
  --gif assets/cat_walking_1.gif \
  --name cat_walking \
  --out assets/cat_walking \
  --manifest-out /tmp/cat_walking_generated.lua
```

(Manifest output is redirected to `/tmp` first, not straight over the hand-authored manifest — the existing `lua/distract/manifests/cat_walking.lua` has real game-design tuning in it that a fresh scaffold would clobber.)

- [ ] **Step 3: Diff the generated scaffold against the existing manifest**

```bash
diff /tmp/cat_walking_generated.lua lua/distract/manifests/cat_walking.lua
```

Manually add only the new `native_path` field (from the generated scaffold's `spritesheet` block) into the real `lua/distract/manifests/cat_walking.lua`, alongside the existing `path`. Leave every hand-tuned field (physics, transitions, states) exactly as it is.

- [ ] **Step 4: Confirm the new `_frames.rgba` file exists and is readable**

```bash
ls -la assets/cat_walking/cat_walking_frames.rgba
```

- [ ] **Step 5: Run the entire Lua test suite**

Run: `nvim --headless --noplugin -u tests/minimal_init.lua -l tests/run_tests.lua`
Expected: all tests pass (the running total from before this plan, plus every test added in Tasks 8–11).

- [ ] **Step 6: Run the entire Rust test suite**

Run: `cargo test --manifest-path engine/Cargo.toml --all-targets --all-features`
Expected: all tests pass (137 previous + everything added in Tasks 1–7).

- [ ] **Step 7: `cargo fmt`, `cargo clippy`, `stylua`, `luacheck` over the whole tree**

```bash
cargo fmt --manifest-path engine/Cargo.toml --all -- --check
cargo clippy --manifest-path engine/Cargo.toml --all-targets --all-features -- -D warnings
stylua --check lua plugin tests
luacheck lua plugin tests
```

- [ ] **Step 8: Note what's still unverified**

Kitty's actual on-screen appearance with a real spritesheet — like every other kitty-backend claim in this codebase (HANDOFF.md) — needs a human on a terminal that speaks the kitty graphics protocol. Record this plainly rather than claiming visual success from automated tests alone.

- [ ] **Step 9: Commit**

```bash
git add lua/distract/manifests/cat_walking.lua assets/cat_walking/cat_walking_sheet.png assets/cat_walking/cat_walking_frames.rgba
git commit -m "feat(cat_walking): regenerate spritesheet and native sidecar through import_sprite"
```

---

## Self-Review Notes

- **Spec coverage:** § 2 (CLI) → Tasks 1–7. § 2.5 (`.rgba` format) → Task 5, byte-compatible reader in Task 9. § 3.1 (capability) → Task 8. § 3.2 (cache-safety fix) → Task 10. § 3.3 (`native_sprite.lua`) → Task 9. § 3.4 (characterization test) → Task 11 Step 4. § 4 (error handling) → Task 9's `nil, err` contract, Task 10's `warn_native_failure`. § 5 (testing) → covered per-task. File inventory → every listed file has a task.
- **Type consistency check:** `DecodedFrame`, `StateSpec`, `ManifestParams` field names match across Tasks 2/6/7. `native_sprite.source_of`/`same_source`/`load` signatures match between Task 9's implementation and Task 10's call sites. `get_pixel_frames(asset_name, opts)` signature matches between Task 10's implementation and Task 11's four call sites.
- **Known open item carried forward, not hidden:** Task 11 Step 4's characterization test is the one place this plan cannot guarantee the outcome in advance — it is explicitly designed to fail loudly and cheaply if `kitty/frames.lua`/`protocol.lua` need real changes, rather than assuming the spec's "expected no changes" guess is correct.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process;

use distract_engine::sprites::{self, SpriteSet};
use image::RgbaImage;
use image::imageops::FilterType;

mod gallery;
mod manifest;
mod svg;

const SCALE_FACTOR: u32 = 8;
const BUILTIN_NAMES: [&str; 3] = ["cat", "crab", "sun"];

pub(crate) struct AssetFrames {
    pub(crate) name: String,
    pub(crate) set: &'static SpriteSet,
    pub(crate) sorted_states: Vec<(String, Vec<usize>)>,
}

fn collect_assets() -> Vec<AssetFrames> {
    BUILTIN_NAMES
        .iter()
        .map(|name| {
            let set = sprites::get(name);
            let mut sorted_states: Vec<(String, Vec<usize>)> = set
                .layout
                .iter()
                .map(|(state, indices)| (state.clone(), indices.clone()))
                .collect();
            sorted_states.sort_by(|left, right| left.1[0].cmp(&right.1[0]));
            AssetFrames {
                name: name.to_string(),
                set,
                sorted_states,
            }
        })
        .collect()
}

fn ensure_dirs(base: &Path, asset_name: &str) -> Result<(), std::io::Error> {
    for sub in ["png_1x", "png_8x", "svg", "sheets"] {
        std::fs::create_dir_all(base.join(asset_name).join(sub))?;
    }
    Ok(())
}

fn write_png_1x(base: &Path, asset: &AssetFrames) -> Result<u32, String> {
    let mut count = 0u32;
    for (state, indices) in &asset.sorted_states {
        for (frame_offset, &frame_index) in indices.iter().enumerate() {
            let filename = format!("{}_{}.png", state, frame_offset);
            let path = base.join(&asset.name).join("png_1x").join(&filename);
            asset.set.frames[frame_index]
                .save(&path)
                .map_err(|err| format!("{}: {}", path.display(), err))?;
            count += 1;
        }
    }
    Ok(count)
}

fn scale_nearest(source: &RgbaImage, factor: u32) -> RgbaImage {
    let (width, height) = source.dimensions();
    image::imageops::resize(source, width * factor, height * factor, FilterType::Nearest)
}

fn write_png_8x(base: &Path, asset: &AssetFrames) -> Result<u32, String> {
    let mut count = 0u32;
    for (state, indices) in &asset.sorted_states {
        for (frame_offset, &frame_index) in indices.iter().enumerate() {
            let filename = format!("{}_{}.png", state, frame_offset);
            let path = base.join(&asset.name).join("png_8x").join(&filename);
            let scaled = scale_nearest(&asset.set.frames[frame_index], SCALE_FACTOR);
            scaled
                .save(&path)
                .map_err(|err| format!("{}: {}", path.display(), err))?;
            count += 1;
        }
    }
    Ok(count)
}

fn write_svgs(base: &Path, asset: &AssetFrames) -> Result<u32, String> {
    let mut count = 0u32;
    for (state, indices) in &asset.sorted_states {
        for (frame_offset, &frame_index) in indices.iter().enumerate() {
            let filename = format!("{}_{}.svg", state, frame_offset);
            let path = base.join(&asset.name).join("svg").join(&filename);
            let svg_content = svg::render_svg(
                &asset.set.frames[frame_index],
                asset.set.width,
                asset.set.height,
            );
            std::fs::write(&path, svg_content)
                .map_err(|err| format!("{}: {}", path.display(), err))?;
            count += 1;
        }
    }
    Ok(count)
}

fn write_spritesheet(base: &Path, asset: &AssetFrames) -> Result<(), String> {
    let total_frames = asset.set.frames.len() as u32;
    let columns = 8u32.min(total_frames);
    let rows = total_frames.div_ceil(columns);
    let cell_width = asset.set.width * SCALE_FACTOR;
    let cell_height = asset.set.height * SCALE_FACTOR;
    let sheet_width = columns * cell_width;
    let sheet_height = rows * cell_height;

    let mut sheet = RgbaImage::new(sheet_width, sheet_height);

    let ordered_frames: Vec<usize> = asset
        .sorted_states
        .iter()
        .flat_map(|(_, indices)| indices.iter().copied())
        .collect();

    for (slot, &frame_index) in ordered_frames.iter().enumerate() {
        let scaled = scale_nearest(&asset.set.frames[frame_index], SCALE_FACTOR);
        let col = (slot as u32) % columns;
        let row = (slot as u32) / columns;
        image::imageops::overlay(
            &mut sheet,
            &scaled,
            (col * cell_width).into(),
            (row * cell_height).into(),
        );
    }

    let filename = format!("{}_sheet_8x.png", asset.name);
    let path = base.join(&asset.name).join("sheets").join(&filename);
    sheet
        .save(&path)
        .map_err(|err| format!("{}: {}", path.display(), err))?;
    Ok(())
}

fn build_manifest(assets: &[AssetFrames]) -> BTreeMap<String, serde_json::Value> {
    let mut manifest = BTreeMap::new();
    for asset in assets {
        manifest.insert(
            asset.name.clone(),
            manifest::asset_entry(asset.set, &asset.sorted_states),
        );
    }
    manifest
}

fn run(output_dir: PathBuf) -> Result<(), String> {
    let assets = collect_assets();
    let mut total_files = 0u32;

    for asset in &assets {
        ensure_dirs(&output_dir, &asset.name)
            .map_err(|err| format!("creating dirs for {}: {}", asset.name, err))?;

        total_files += write_png_1x(&output_dir, asset)?;
        total_files += write_png_8x(&output_dir, asset)?;
        total_files += write_svgs(&output_dir, asset)?;
        write_spritesheet(&output_dir, asset)?;
        total_files += 1;
    }

    let manifest = build_manifest(&assets);
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("serializing manifest: {}", err))?;
    let manifest_path = output_dir.join("manifest.json");
    std::fs::write(&manifest_path, &manifest_json)
        .map_err(|err| format!("{}: {}", manifest_path.display(), err))?;
    total_files += 1;

    let gallery_html = gallery::render_gallery(&assets, &manifest_json);
    let gallery_path = output_dir.join("index.html");
    std::fs::write(&gallery_path, gallery_html)
        .map_err(|err| format!("{}: {}", gallery_path.display(), err))?;
    total_files += 1;

    eprintln!("Exported {} files to {}", total_files, output_dir.display());
    Ok(())
}

fn main() {
    let output_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("assets/sprites"));

    if let Err(message) = run(output_dir) {
        eprintln!("export_sprites: {}", message);
        process::exit(1);
    }
}

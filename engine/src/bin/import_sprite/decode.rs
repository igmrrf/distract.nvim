use std::fs;
use std::path::Path;
use std::time::Duration;

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage, imageops};

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
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("png"))
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
            Ok(DecodedFrame {
                image,
                delay_ms: None,
            })
        })
        .collect()
}

pub struct GridSpec<'a> {
    pub cell_width: u32,
    pub cell_height: u32,
    pub row_counts: &'a [usize],
}

fn validate_grid(image: &RgbaImage, grid: &GridSpec, path: &Path) -> Result<(u32, u32), String> {
    if grid.cell_width == 0 || grid.cell_height == 0 {
        return Err("--cell needs a non-zero width and height".to_string());
    }
    let (width, height) = image.dimensions();
    if width % grid.cell_width != 0 || height % grid.cell_height != 0 {
        return Err(format!(
            "{}: {width}x{height} is not a whole number of {}x{} cells",
            path.display(),
            grid.cell_width,
            grid.cell_height
        ));
    }

    let columns = width / grid.cell_width;
    let rows = height / grid.cell_height;
    if grid.row_counts.len() as u32 != rows {
        return Err(format!(
            "{}: --row-counts has {} entries but the image has {rows} rows",
            path.display(),
            grid.row_counts.len()
        ));
    }
    for (row, count) in grid.row_counts.iter().enumerate() {
        if *count as u32 > columns {
            return Err(format!(
                "{}: row {row} claims {count} frames but there are only {columns} columns",
                path.display()
            ));
        }
    }

    Ok((columns, rows))
}

/// Slices a pre-packed atlas into frames, row-major, dropping the unused
/// trailing cells of any row shorter than the grid is wide.
///
/// # Errors
///
/// Returns `Err` when the file cannot be decoded, when its dimensions are not a
/// whole number of cells, or when `row_counts` disagrees with the grid.
pub fn decode_spritesheet_grid(path: &Path, grid: &GridSpec) -> Result<Vec<DecodedFrame>, String> {
    let image = image::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .to_rgba8();
    validate_grid(&image, grid, path)?;

    let mut frames = Vec::new();
    for (row, count) in grid.row_counts.iter().enumerate() {
        for column in 0..*count {
            let cropped = imageops::crop_imm(
                &image,
                column as u32 * grid.cell_width,
                row as u32 * grid.cell_height,
                grid.cell_width,
                grid.cell_height,
            )
            .to_image();
            frames.push(DecodedFrame {
                image: cropped,
                delay_ms: None,
            });
        }
    }

    if frames.is_empty() {
        return Err(format!("{}: no frames selected", path.display()));
    }
    Ok(frames)
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
                let frame =
                    image::Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1));
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

    const CELL: u32 = 4;

    /// A 2x2 grid of 4x4 cells, each a distinct solid color.
    fn write_fixture_atlas(path: &Path) {
        let colors = [
            [10u8, 0, 0, 255],
            [0, 20, 0, 255],
            [0, 0, 30, 255],
            [40, 40, 40, 255],
        ];
        let mut atlas = RgbaImage::new(CELL * 2, CELL * 2);
        for (index, color) in colors.iter().enumerate() {
            let origin_x = (index as u32 % 2) * CELL;
            let origin_y = (index as u32 / 2) * CELL;
            for y in 0..CELL {
                for x in 0..CELL {
                    atlas.put_pixel(origin_x + x, origin_y + y, Rgba(*color));
                }
            }
        }
        atlas.save(path).expect("write fixture atlas");
    }

    fn grid(row_counts: &[usize]) -> GridSpec<'_> {
        GridSpec {
            cell_width: CELL,
            cell_height: CELL,
            row_counts,
        }
    }

    #[test]
    fn decode_spritesheet_grid_slices_row_major_and_drops_unused_trailing_cells() {
        let path = std::env::temp_dir().join("distract_decode_atlas_test.png");
        write_fixture_atlas(&path);

        let frames = decode_spritesheet_grid(&path, &grid(&[2, 1])).expect("decode");

        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].delay_ms, None);
        assert_eq!(frames[0].image.dimensions(), (CELL, CELL));
        assert_eq!(frames[0].image.get_pixel(0, 0), &Rgba([10, 0, 0, 255]));
        assert_eq!(frames[1].image.get_pixel(0, 0), &Rgba([0, 20, 0, 255]));
        assert_eq!(frames[2].image.get_pixel(0, 0), &Rgba([0, 0, 30, 255]));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_spritesheet_grid_rejects_a_cell_size_the_image_is_not_a_multiple_of() {
        let path = std::env::temp_dir().join("distract_decode_atlas_bad_cell_test.png");
        write_fixture_atlas(&path);

        let spec = GridSpec {
            cell_width: 3,
            cell_height: CELL,
            row_counts: &[2, 1],
        };
        assert!(decode_spritesheet_grid(&path, &spec).is_err());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_spritesheet_grid_rejects_a_row_count_list_of_the_wrong_length() {
        let path = std::env::temp_dir().join("distract_decode_atlas_bad_rows_test.png");
        write_fixture_atlas(&path);

        assert!(decode_spritesheet_grid(&path, &grid(&[2, 2, 2])).is_err());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_spritesheet_grid_rejects_a_row_claiming_more_frames_than_columns() {
        let path = std::env::temp_dir().join("distract_decode_atlas_wide_row_test.png");
        write_fixture_atlas(&path);

        assert!(decode_spritesheet_grid(&path, &grid(&[5, 1])).is_err());

        fs::remove_file(&path).ok();
    }
}

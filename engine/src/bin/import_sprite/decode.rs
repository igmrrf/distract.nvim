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
}

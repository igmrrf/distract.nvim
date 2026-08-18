use std::fs;
use std::path::Path;

use image::RgbaImage;

const MAGIC: &[u8; 4] = b"DRGB";
const VERSION: u8 = 1;
const HEADER_SIZE: usize = 17;
const BYTES_PER_PIXEL: usize = 4;

pub fn write_rgba_sidecar(
    path: &Path,
    frame_width: u32,
    frame_height: u32,
    frames: &[RgbaImage],
) -> Result<(), String> {
    let frame_byte_len = frame_width as usize * frame_height as usize * BYTES_PER_PIXEL;
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

#[cfg(test)]
fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated header".to_string())?;
    let mut quad = [0u8; 4];
    quad.copy_from_slice(slice);
    Ok(u32::from_le_bytes(quad))
}

#[cfg(test)]
pub fn read_rgba_sidecar(path: &Path) -> Result<(u32, u32, Vec<RgbaImage>), String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;

    if bytes.len() < HEADER_SIZE {
        return Err(format!("{}: truncated header", path.display()));
    }
    if bytes.get(0..4) != Some(&MAGIC[..]) {
        return Err(format!("{}: bad magic", path.display()));
    }
    let version = bytes[4];
    if version != VERSION {
        return Err(format!("{}: unsupported version {version}", path.display()));
    }

    let frame_width =
        read_u32_le(&bytes, 5).map_err(|error| format!("{}: {error}", path.display()))?;
    let frame_height =
        read_u32_le(&bytes, 9).map_err(|error| format!("{}: {error}", path.display()))?;
    let frame_count =
        read_u32_le(&bytes, 13).map_err(|error| format!("{}: {error}", path.display()))?;

    let frame_byte_len = (frame_width as usize)
        .checked_mul(frame_height as usize)
        .and_then(|pixels| pixels.checked_mul(BYTES_PER_PIXEL))
        .ok_or_else(|| format!("{}: declared frame size overflows", path.display()))?;
    let expected_len = (frame_count as usize)
        .checked_mul(frame_byte_len)
        .and_then(|body| body.checked_add(HEADER_SIZE))
        .ok_or_else(|| format!("{}: declared frame count overflows", path.display()))?;
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
        let raw = bytes
            .get(start..start + frame_byte_len)
            .ok_or_else(|| format!("{}: frame {index} is truncated", path.display()))?
            .to_vec();
        let image = RgbaImage::from_raw(frame_width, frame_height, raw).ok_or_else(|| {
            format!(
                "{}: frame {index} has the wrong byte length",
                path.display()
            )
        })?;
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
        let frames = vec![frame_one, frame_two];

        let path = std::env::temp_dir().join("distract_rgba_sidecar_round_trip_test.rgba");
        write_rgba_sidecar(&path, 2, 2, &frames).expect("write");
        let (width, height, read_frames) = read_rgba_sidecar(&path).expect("read");

        assert_eq!((width, height), (2, 2));
        assert_eq!(read_frames, frames);

        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_truncated_file_is_rejected_not_panicked_on() {
        let path = std::env::temp_dir().join("distract_rgba_sidecar_truncated_test.rgba");
        fs::write(&path, b"DRGB").expect("write fixture");

        assert!(read_rgba_sidecar(&path).is_err());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn bad_magic_is_rejected() {
        let path = std::env::temp_dir().join("distract_rgba_sidecar_bad_magic_test.rgba");
        fs::write(&path, [0u8; 20]).expect("write fixture");

        assert!(read_rgba_sidecar(&path).is_err());

        fs::remove_file(&path).ok();
    }
}

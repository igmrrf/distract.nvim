use image::{RgbaImage, imageops};

const MAX_COLUMNS: u32 = 8;

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
    let columns = (frame_count as u32).clamp(1, MAX_COLUMNS);
    let rows = (frame_count as u32).div_ceil(columns);
    (columns, rows)
}

pub fn pack_spritesheet(
    frames: &[RgbaImage],
    columns: u32,
    frame_width: u32,
    frame_height: u32,
) -> RgbaImage {
    let rows = (frames.len() as u32).div_ceil(columns);
    let mut sheet = RgbaImage::new(columns * frame_width, rows * frame_height);

    for (index, frame) in frames.iter().enumerate() {
        let column = index as u32 % columns;
        let row = index as u32 / columns;
        imageops::overlay(
            &mut sheet,
            frame,
            (column * frame_width).into(),
            (row * frame_height).into(),
        );
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

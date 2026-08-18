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

        for (neighbor_x, neighbor_y) in neighbors(x, y, width, height) {
            let index = (neighbor_y * width + neighbor_x) as usize;
            if !visited[index] {
                visited[index] = true;
                queue.push_back((neighbor_x, neighbor_y));
            }
        }
    }

    output
}

/// Whether a frame arrives with its background already cut out.
///
/// `remove_background` recomputes alpha from RGB distance to the corner color
/// and ignores the alpha a pixel already has, so running it over art that is
/// already cutout walks into the antialiased edge halo and overwrites correct
/// edge alpha with a value derived from how close that pixel happens to be to
/// black. Detecting the case is what keeps that from happening silently.
pub fn is_already_cutout(frame: &RgbaImage) -> bool {
    let (width, height) = frame.dimensions();
    corners(width, height)
        .iter()
        .all(|&(x, y)| frame.get_pixel(x, y)[3] == 0)
}

fn corners(width: u32, height: u32) -> [(u32, u32); 4] {
    [
        (0, 0),
        (width - 1, 0),
        (0, height - 1),
        (width - 1, height - 1),
    ]
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
    let red_delta = (pixel[0] as f32 - reference[0]) / 255.0;
    let green_delta = (pixel[1] as f32 - reference[1]) / 255.0;
    let blue_delta = (pixel[2] as f32 - reference[2]) / 255.0;
    (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt()
        / 3.0f32.sqrt()
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
            frame.put_pixel(x, 4, Rgba([10, 10, 10, 255]));
        }
        for y in 0..5 {
            frame.put_pixel(0, y, Rgba([10, 10, 10, 255]));
            frame.put_pixel(4, y, Rgba([10, 10, 10, 255]));
        }
        frame.put_pixel(2, 2, Rgba([10, 10, 10, 255]));

        let output = remove_background(&frame, 0.12, 0.04);

        assert_eq!(output.get_pixel(0, 0)[3], 0);
        assert_eq!(
            output.get_pixel(2, 2)[3],
            255,
            "isolated same-colored pixel must stay opaque"
        );
    }

    #[test]
    fn is_already_cutout_reads_the_four_corners_alpha() {
        let mut opaque = RgbaImage::new(4, 4);
        for pixel in opaque.pixels_mut() {
            *pixel = Rgba([10, 10, 10, 255]);
        }
        assert!(!is_already_cutout(&opaque));

        let mut cutout = RgbaImage::new(4, 4);
        for pixel in cutout.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        cutout.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
        assert!(is_already_cutout(&cutout));

        cutout.put_pixel(0, 0, Rgba([0, 0, 0, 1]));
        assert!(
            !is_already_cutout(&cutout),
            "one non-transparent corner is enough to disqualify a frame"
        );
    }

    #[test]
    fn a_pixel_just_past_tolerance_gets_a_feathered_not_binary_alpha() {
        let mut frame = RgbaImage::new(3, 1);
        frame.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        frame.put_pixel(1, 0, Rgba([15, 15, 15, 255]));
        frame.put_pixel(2, 0, Rgba([0, 0, 0, 255]));

        let output = remove_background(&frame, 0.0, 0.5);

        let middle_alpha = output.get_pixel(1, 0)[3];
        assert!(
            middle_alpha > 0 && middle_alpha < 255,
            "expected a feathered value, got {middle_alpha}"
        );
    }
}

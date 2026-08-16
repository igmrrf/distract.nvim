use image::RgbaImage;

pub fn render_svg(frame: &RgbaImage, width: u32, height: u32) -> String {
    let mut rects = String::new();
    for row in 0..height {
        for col in 0..width {
            let pixel = frame.get_pixel(col, row);
            if pixel[3] == 0 {
                continue;
            }
            rects.push_str(&format!(
                r#"<rect x="{}" y="{}" width="1" height="1" fill="rgba({},{},{},{:.3})"/>"#,
                col,
                row,
                pixel[0],
                pixel[1],
                pixel[2],
                pixel[3] as f32 / 255.0,
            ));
            rects.push('\n');
        }
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" shape-rendering="crispEdges">
{}</svg>
"#,
        width, height, rects,
    )
}

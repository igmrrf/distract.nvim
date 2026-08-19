//! The rectangle entities are allowed to move in.
//!
//! Boundary handling used to measure against the window: `0` on the left and top,
//! `viewport_w`/`viewport_h` on the right and bottom. That is still the default,
//! and it is what an overlay covering a whole display wants — but Neovim can ask
//! for a smaller rectangle so sprites stay inside one buffer's text area rather
//! than roaming the screen. Only the editor can measure that, which is why it
//! arrives over IPC instead of being derived here.
//!
//! In overlay pixels, matching every other coordinate the engine is sent.

/// A left, top, width and height, all in overlay pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Bounds {
    /// The whole window, which is what an unscoped session uses.
    pub fn window(width: f32, height: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width,
            height,
        }
    }

    /// A scope Neovim asked for, clamped to stay inside the window.
    ///
    /// An offscreen or zero-sized rectangle would leave every entity permanently
    /// out of bounds, so a scope that does not intersect the window is refused
    /// and the caller keeps whatever it had.
    pub fn scoped(request: Bounds, window_width: f32, window_height: f32) -> Result<Self, String> {
        if request.width <= 0.0 || request.height <= 0.0 {
            return Err(format!(
                "viewport scope must have a positive size, got {}x{}",
                request.width, request.height
            ));
        }

        let left = request.left.clamp(0.0, window_width);
        let top = request.top.clamp(0.0, window_height);
        let width = (request.left + request.width).min(window_width) - left;
        let height = (request.top + request.height).min(window_height) - top;

        if width <= 0.0 || height <= 0.0 {
            return Err("viewport scope does not intersect the overlay window".to_string());
        }

        Ok(Self {
            left,
            top,
            width,
            height,
        })
    }

    pub fn right(&self) -> f32 {
        self.left + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_bounds_start_at_the_origin() {
        let bounds = Bounds::window(1920.0, 1080.0);
        assert_eq!(bounds.left, 0.0);
        assert_eq!(bounds.top, 0.0);
        assert_eq!(bounds.right(), 1920.0);
        assert_eq!(bounds.bottom(), 1080.0);
    }

    #[test]
    fn a_scope_inside_the_window_is_kept_as_asked_for() {
        let request = Bounds {
            left: 100.0,
            top: 40.0,
            width: 800.0,
            height: 600.0,
        };
        let bounds = Bounds::scoped(request, 1920.0, 1080.0).expect("inside the window");
        assert_eq!(bounds, request);
    }

    #[test]
    fn a_scope_hanging_off_the_edge_is_clipped_rather_than_refused() {
        let request = Bounds {
            left: 1800.0,
            top: 1000.0,
            width: 400.0,
            height: 400.0,
        };
        let bounds = Bounds::scoped(request, 1920.0, 1080.0).expect("it still intersects");
        assert_eq!(bounds.left, 1800.0);
        assert_eq!(bounds.right(), 1920.0);
        assert_eq!(bounds.bottom(), 1080.0);
    }

    #[test]
    fn a_scope_with_no_size_is_refused() {
        let request = Bounds {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 100.0,
        };
        assert!(Bounds::scoped(request, 1920.0, 1080.0).is_err());
    }

    #[test]
    fn a_scope_entirely_offscreen_is_refused() {
        let request = Bounds {
            left: 4000.0,
            top: 0.0,
            width: 200.0,
            height: 200.0,
        };
        assert!(Bounds::scoped(request, 1920.0, 1080.0).is_err());
    }
}

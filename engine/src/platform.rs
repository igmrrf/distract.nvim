use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowLevel},
};

/// Why an overlay window could not be made click-through.
///
/// Click-through is the only thing stopping a fullscreen, always-on-top,
/// borderless window from swallowing every click on the user's desktop. On X11
/// `set_cursor_hittest` returns `Err(NotSupported)` unconditionally, so
/// discarding the result left the overlay as a full-screen input trap with no
/// way out except killing the process.
#[derive(Debug)]
pub struct ClickThroughUnsupported {
    pub reason: String,
}

impl std::fmt::Display for ClickThroughUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for ClickThroughUnsupported {}

#[derive(Debug)]
pub enum OverlayError {
    Os(winit::error::OsError),
    ClickThrough(ClickThroughUnsupported),
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Os(e) => write!(f, "{}", e),
            Self::ClickThrough(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for OverlayError {}

impl From<winit::error::OsError> for OverlayError {
    fn from(e: winit::error::OsError) -> Self {
        Self::Os(e)
    }
}

/// Creates a transparent, borderless, always-on-top overlay window across
/// macOS, Linux and Windows.
///
/// Fails rather than returning a window that cannot be clicked through.
pub fn create_overlay_window<T>(
    target: &EventLoopWindowTarget<T>,
    width: u32,
    height: u32,
) -> Result<Window, OverlayError> {
    #[allow(unused_mut)]
    let mut builder = WindowBuilder::new()
        .with_title("Distract Overlay")
        .with_transparent(true)
        .with_decorations(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_position(PhysicalPosition::new(0, 0))
        .with_inner_size(PhysicalSize::new(width, height));

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowBuilderExtMacOS;
        builder = builder
            .with_has_shadow(false)
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true);
    }

    let window = builder.build(target)?;

    make_click_through(&window).map_err(OverlayError::ClickThrough)?;
    configure_layer_transparency(&window);

    Ok(window)
}

/// Makes the window transparent to mouse input.
///
/// Returns an error, naming the platform and a workaround, when the window
/// would otherwise capture every click on the desktop.
pub fn make_click_through(window: &Window) -> Result<(), ClickThroughUnsupported> {
    // macOS is handled directly through AppKit in `configure_layer_transparency`
    // (`setIgnoresMouseEvents:`), which works regardless of what winit reports.
    #[cfg(target_os = "macos")]
    {
        let _ = window.set_cursor_hittest(false);
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        match window.set_cursor_hittest(false) {
            Ok(()) => Ok(()),
            Err(err) => {
                // X11 is the known case: winit returns NotSupported
                // unconditionally there. Wayland and Windows both succeed.
                Err(ClickThroughUnsupported {
                    reason: format!(
                        "this display server does not support click-through overlays ({}). \
                         A fullscreen always-on-top window without it would capture every \
                         mouse click on your desktop, so the overlay backend refuses to start. \
                         On X11, run a compositing Wayland session, or use the default \
                         in-terminal backend: require('distract').setup({{ backend = 'halfblock' }})",
                        err
                    ),
                })
            }
        }
    }
}

/// Configures OS-level layer and window transparency on platforms like macOS
/// (CAMetalLayer).
pub fn configure_layer_transparency(window: &Window) {
    #[cfg(target_os = "macos")]
    unsafe {
        use objc::{class, msg_send, sel};
        use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
        if let RawWindowHandle::AppKit(handle) = window.raw_window_handle() {
            let ns_window = handle.ns_window as *mut objc::runtime::Object;
            let ns_view = handle.ns_view as *mut objc::runtime::Object;
            if !ns_window.is_null() {
                let () = msg_send![ns_window, setOpaque: objc::runtime::NO];
                let () = msg_send![ns_window, setHasShadow: objc::runtime::NO];
                let clear_color: *mut objc::runtime::Object =
                    msg_send![class!(NSColor), clearColor];
                let () = msg_send![ns_window, setBackgroundColor: clear_color];
                let () = msg_send![ns_window, setIgnoresMouseEvents: objc::runtime::YES];
            }
            if !ns_view.is_null() {
                let () = msg_send![ns_view, setWantsLayer: objc::runtime::YES];
                let layer: *mut objc::runtime::Object = msg_send![ns_view, layer];
                if !layer.is_null() {
                    let () = msg_send![layer, setOpaque: objc::runtime::NO];
                    let clear_color: *mut objc::runtime::Object =
                        msg_send![class!(NSColor), clearColor];
                    if !clear_color.is_null() {
                        let cg_color: *mut objc::runtime::Object = msg_send![clear_color, CGColor];
                        if !cg_color.is_null() {
                            let () = msg_send![layer, setBackgroundColor: cg_color];
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_through_failure_names_the_cause_and_a_way_out() {
        let err = ClickThroughUnsupported {
            reason: "this display server does not support click-through overlays (NotSupported). \
                     Use backend = 'halfblock'"
                .to_string(),
        };
        let text = err.to_string();
        assert!(text.contains("click-through"));
        assert!(
            text.contains("halfblock"),
            "must point at a working backend"
        );
    }

    #[test]
    fn overlay_error_displays_both_variants() {
        let e = OverlayError::ClickThrough(ClickThroughUnsupported {
            reason: "nope".to_string(),
        });
        assert_eq!(e.to_string(), "nope");
    }
}

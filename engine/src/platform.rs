use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::EventLoopWindowTarget,
    monitor::MonitorHandle,
    window::{Window, WindowBuilder, WindowLevel},
};

use crate::overlay_placement::{MonitorGeometry, OverlayPlacement};

fn geometry_of(monitor: &MonitorHandle) -> MonitorGeometry {
    let position = monitor.position();
    let size = monitor.size();
    MonitorGeometry {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

/// Every display the window system reports, primary first.
pub fn monitor_geometries<T>(target: &EventLoopWindowTarget<T>) -> Vec<MonitorGeometry> {
    let primary = target.primary_monitor();
    let mut geometries: Vec<MonitorGeometry> = Vec::new();

    if let Some(ref monitor) = primary {
        geometries.push(geometry_of(monitor));
    }
    for monitor in target.available_monitors() {
        if Some(&monitor) == primary.as_ref() {
            continue;
        }
        geometries.push(geometry_of(&monitor));
    }

    geometries
}

/// The display holding the window with keyboard focus, when the platform can say.
///
/// On macOS this is `NSScreen.mainScreen`, which is the *focused* screen. That is
/// not what winit's `primary_monitor()` returns — that is the screen with the menu
/// bar — and the difference is the whole bug: the overlay starts while the
/// terminal still has focus, so the focused screen is the terminal's screen.
///
/// Matched by `NSScreenNumber` (a `CGDirectDisplayID`) against winit's
/// `native_id()`, so no Cocoa-to-winit coordinate conversion is involved.
///
/// Returns `None` on every other platform. There is no cross-platform way to ask,
/// and a wrong guess here is exactly what the config key exists for.
pub fn focused_monitor<T>(target: &EventLoopWindowTarget<T>) -> Option<MonitorGeometry> {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::MonitorHandleExtMacOS;
        let display_id = focused_display_id()?;
        target
            .available_monitors()
            .find(|monitor| monitor.native_id() == display_id)
            .map(|monitor| geometry_of(&monitor))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = target;
        None
    }
}

#[cfg(target_os = "macos")]
fn focused_display_id() -> Option<u32> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let screen: *mut Object = msg_send![class!(NSScreen), mainScreen];
        if screen.is_null() {
            return None;
        }
        let description: *mut Object = msg_send![screen, deviceDescription];
        if description.is_null() {
            return None;
        }
        let key: *mut Object =
            msg_send![class!(NSString), stringWithUTF8String: c"NSScreenNumber".as_ptr()];
        if key.is_null() {
            return None;
        }
        let number: *mut Object = msg_send![description, objectForKey: key];
        if number.is_null() {
            return None;
        }
        let display_id: u32 = msg_send![number, unsignedIntValue];
        Some(display_id)
    }
}

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

/// Describes the overlay window.
///
/// `with_active(false)` is load-bearing, not cosmetic: winit's default is an
/// active window, which on macOS is created with `makeKeyAndOrderFront` and on
/// every platform pulls keyboard focus off whatever the user was typing into.
/// An inactive window is ordered front instead, so the overlay appears above
/// the editor without the editor losing focus. `ActivationPolicy::Accessory`
/// does not cover this — it keeps the process out of the Dock and menu bar, and
/// has no bearing on which window is key.
fn overlay_window_builder(placement: OverlayPlacement) -> WindowBuilder {
    #[allow(unused_mut)]
    let mut builder = WindowBuilder::new()
        .with_title("Distract Overlay")
        .with_transparent(true)
        .with_decorations(false)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        // Global coordinates, not display-relative: `(0, 0)` is the primary
        // display's top-left, which is why hardcoding it pinned the overlay to
        // the primary display on every multi-monitor desktop.
        .with_position(PhysicalPosition::new(placement.x, placement.y))
        .with_inner_size(PhysicalSize::new(placement.width, placement.height));

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowBuilderExtMacOS;
        builder = builder
            .with_has_shadow(false)
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true);
    }

    builder
}

/// Creates a transparent, borderless, always-on-top overlay window across
/// macOS, Linux and Windows.
///
/// Fails rather than returning a window that cannot be clicked through.
pub fn create_overlay_window<T>(
    target: &EventLoopWindowTarget<T>,
    placement: OverlayPlacement,
) -> Result<Window, OverlayError> {
    let window = overlay_window_builder(placement).build(target)?;

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
    fn the_builder_positions_the_window_where_it_was_told() {
        let described = format!(
            "{:?}",
            overlay_window_builder(OverlayPlacement {
                x: 1512,
                y: 0,
                width: 2560,
                height: 1440,
            })
        );
        assert!(
            described.contains("1512"),
            "the placement's x must reach the builder, or the overlay lands on the \
             primary display whatever was detected; got {described}"
        );
    }

    #[test]
    fn overlay_error_displays_both_variants() {
        let e = OverlayError::ClickThrough(ClickThroughUnsupported {
            reason: "nope".to_string(),
        });
        assert_eq!(e.to_string(), "nope");
    }

    #[test]
    fn the_overlay_window_does_not_take_focus_when_it_appears() {
        let described = format!(
            "{:?}",
            overlay_window_builder(OverlayPlacement {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            })
        );
        assert!(
            described.contains("active: false"),
            "an active window is made key on creation, which pulls focus off the \
             editor and forces the user to click back into it; got {described}"
        );
    }
}

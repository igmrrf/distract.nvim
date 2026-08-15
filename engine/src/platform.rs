use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event_loop::EventLoopWindowTarget,
    window::{Window, WindowBuilder, WindowLevel},
};

/// Creates a transparent, borderless, always-on-top overlay window across macOS, Linux, and Windows.
pub fn create_overlay_window<T>(
    target: &EventLoopWindowTarget<T>,
    width: u32,
    height: u32,
) -> Result<Window, winit::error::OsError> {
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

    // Click-through configuration where supported
    let _ = window.set_cursor_hittest(false);

    configure_layer_transparency(&window);

    Ok(window)
}

/// Configures OS-level layer and window transparency on platforms like macOS (CAMetalLayer).
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
                let clear_color: *mut objc::runtime::Object = msg_send![class!(NSColor), clearColor];
                let () = msg_send![ns_window, setBackgroundColor: clear_color];
                let () = msg_send![ns_window, setIgnoresMouseEvents: objc::runtime::YES];
            }
            if !ns_view.is_null() {
                let () = msg_send![ns_view, setWantsLayer: objc::runtime::YES];
                let layer: *mut objc::runtime::Object = msg_send![ns_view, layer];
                if !layer.is_null() {
                    let () = msg_send![layer, setOpaque: objc::runtime::NO];
                    let clear_color: *mut objc::runtime::Object = msg_send![class!(NSColor), clearColor];
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
}





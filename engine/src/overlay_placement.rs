//! Where the overlay window goes, and why.
//!
//! The overlay used to be created at global `(0, 0)` and sized from
//! `primary_monitor()`, which on a multi-monitor desktop put it on the primary
//! display however far away the terminal was. Nothing detected the mistake and
//! nothing said anything.
//!
//! Neither half of the plugin can work out the right display on its own: Neovim
//! does not know its own screen position, so the Lua side has nothing to send,
//! and the window system only answers if asked from the process that owns a
//! window. So the order is: what the user configured, then what the platform can
//! detect, then the primary display with a warning naming the config key.
//!
//! This module is deliberately free of `winit` types so the precedence is a pure
//! function with unit tests. `platform` converts to and from it.

/// A display's position and size in the window system's global coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorGeometry {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.width as i32)
            && y < self.y.saturating_add(self.height as i32)
    }
}

/// An explicit choice from `require("distract").setup { overlay = ... }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfiguredPlacement {
    /// Index into the window system's monitor list.
    Monitor(usize),
    /// A point in the global coordinate space.
    Position { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct PlacementRequest<'a> {
    pub configured: Option<ConfiguredPlacement>,
    /// The display holding the focused window, when the platform can say.
    pub focused: Option<MonitorGeometry>,
    /// Every display, primary first, as the window system reports them.
    pub monitors: &'a [MonitorGeometry],
}

/// The window is never made smaller than this, even on a display that is.
/// Carried over unchanged from the original sizing.
const MIN_WIDTH: u32 = 800;
const MIN_HEIGHT: u32 = 600;
const MAX_WIDTH: u32 = 3840;
const MAX_HEIGHT: u32 = 2160;

/// Used only when the window system reports no displays at all.
const FALLBACK_WIDTH: u32 = 1920;
const FALLBACK_HEIGHT: u32 = 1080;

const CONFIG_HINT: &str = "set it explicitly with \
     require('distract').setup { overlay = { monitor = <index> } } \
     (0 is the primary display), or overlay = { position = { x, y } }";

fn placement_on(monitor: &MonitorGeometry) -> OverlayPlacement {
    OverlayPlacement {
        x: monitor.x,
        y: monitor.y,
        width: monitor.width.clamp(MIN_WIDTH, MAX_WIDTH),
        height: monitor.height.clamp(MIN_HEIGHT, MAX_HEIGHT),
    }
}

/// Resolves where the overlay goes, plus a warning when the answer was a guess.
///
/// A returned warning is not a failure: the overlay still opens. It says the
/// display was chosen for the user rather than by them, which is the difference
/// between "my overlay is on the wrong screen and I have no idea why" and a line
/// in `:messages` naming the key that fixes it.
pub fn resolve(request: &PlacementRequest) -> (OverlayPlacement, Option<String>) {
    match request.configured {
        Some(ConfiguredPlacement::Position { x, y }) => {
            let host = request
                .monitors
                .iter()
                .find(|monitor| monitor.contains(x, y))
                .or_else(|| request.monitors.first());
            let mut placement = match host {
                Some(monitor) => placement_on(monitor),
                None => OverlayPlacement {
                    x,
                    y,
                    width: FALLBACK_WIDTH,
                    height: FALLBACK_HEIGHT,
                },
            };
            placement.x = x;
            placement.y = y;
            (placement, None)
        }

        Some(ConfiguredPlacement::Monitor(index)) => match request.monitors.get(index) {
            Some(monitor) => (placement_on(monitor), None),
            None => {
                let warning = format!(
                    "overlay.monitor is {index} but this system reports {} display(s); \
                     falling back to the primary one. Valid indices are 0..{}",
                    request.monitors.len(),
                    request.monitors.len().saturating_sub(1)
                );
                (fall_back_to_primary(request), Some(warning))
            }
        },

        None => match request.focused {
            Some(monitor) => (placement_on(&monitor), None),
            None => {
                let warning = format!(
                    "could not detect which display the terminal is on, so the overlay \
                     opened on the primary one. If that is the wrong screen, {CONFIG_HINT}"
                );
                (fall_back_to_primary(request), Some(warning))
            }
        },
    }
}

fn fall_back_to_primary(request: &PlacementRequest) -> OverlayPlacement {
    match request.monitors.first() {
        Some(monitor) => placement_on(monitor),
        None => OverlayPlacement {
            x: 0,
            y: 0,
            width: FALLBACK_WIDTH,
            height: FALLBACK_HEIGHT,
        },
    }
}

/// Parses `--overlay-monitor <index>` / `--overlay-position <x>,<y>` from argv.
///
/// Returns `Err` rather than ignoring a malformed value: a typo that silently
/// reverted to the old behaviour is the failure this whole module exists to stop.
pub fn from_args<I>(args: I) -> Result<Option<ConfiguredPlacement>, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut configured = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--overlay-monitor" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--overlay-monitor needs an index".to_string())?;
                let index = raw
                    .parse::<usize>()
                    .map_err(|_| format!("--overlay-monitor expects an index, got '{raw}'"))?;
                configured = Some(ConfiguredPlacement::Monitor(index));
            }
            "--overlay-position" => {
                let raw = args
                    .next()
                    .ok_or_else(|| "--overlay-position needs <x>,<y>".to_string())?;
                let (x, y) = raw
                    .split_once(',')
                    .ok_or_else(|| format!("--overlay-position expects <x>,<y>, got '{raw}'"))?;
                let x = x
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| format!("--overlay-position x is not a number: '{x}'"))?;
                let y = y
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| format!("--overlay-position y is not a number: '{y}'"))?;
                configured = Some(ConfiguredPlacement::Position { x, y });
            }
            _ => {}
        }
    }

    Ok(configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAPTOP: MonitorGeometry = MonitorGeometry {
        x: 0,
        y: 0,
        width: 1512,
        height: 982,
    };
    /// To the right of the laptop, as macOS reports a second display.
    const EXTERNAL: MonitorGeometry = MonitorGeometry {
        x: 1512,
        y: 0,
        width: 2560,
        height: 1440,
    };

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_detected_display_wins_when_nothing_is_configured() {
        let monitors = [LAPTOP, EXTERNAL];
        let (placement, warning) = resolve(&PlacementRequest {
            configured: None,
            focused: Some(EXTERNAL),
            monitors: &monitors,
        });

        assert_eq!(placement.x, 1512, "the overlay must follow the terminal");
        assert_eq!(placement.width, 2560);
        assert!(warning.is_none(), "detection succeeded, so nothing to warn");
    }

    #[test]
    fn undetectable_falls_back_to_primary_and_names_the_config_key() {
        let monitors = [LAPTOP, EXTERNAL];
        let (placement, warning) = resolve(&PlacementRequest {
            configured: None,
            focused: None,
            monitors: &monitors,
        });

        assert_eq!(placement.x, 0, "primary is the documented fallback");
        let warning = warning.expect("a guessed display must warn");
        assert!(
            warning.contains("overlay = { monitor"),
            "the warning must name the key that fixes it; got {warning}"
        );
    }

    #[test]
    fn configured_monitor_overrides_detection() {
        let monitors = [LAPTOP, EXTERNAL];
        let (placement, warning) = resolve(&PlacementRequest {
            configured: Some(ConfiguredPlacement::Monitor(0)),
            focused: Some(EXTERNAL),
            monitors: &monitors,
        });

        assert_eq!(placement.x, 0, "the user's choice beats detection");
        assert!(warning.is_none());
    }

    #[test]
    fn an_out_of_range_monitor_warns_with_the_valid_range() {
        let monitors = [LAPTOP];
        let (placement, warning) = resolve(&PlacementRequest {
            configured: Some(ConfiguredPlacement::Monitor(4)),
            focused: None,
            monitors: &monitors,
        });

        assert_eq!(placement.x, 0);
        let warning = warning.expect("an impossible index must warn");
        assert!(warning.contains("1 display"), "got {warning}");
        assert!(warning.contains("0..0"), "got {warning}");
    }

    #[test]
    fn an_explicit_position_is_used_verbatim_and_sized_from_its_host_display() {
        let monitors = [LAPTOP, EXTERNAL];
        let (placement, warning) = resolve(&PlacementRequest {
            configured: Some(ConfiguredPlacement::Position { x: 1600, y: 40 }),
            focused: Some(LAPTOP),
            monitors: &monitors,
        });

        assert_eq!((placement.x, placement.y), (1600, 40));
        assert_eq!(
            placement.width, 2560,
            "the size must come from the display the point lands on"
        );
        assert!(warning.is_none());
    }

    #[test]
    fn a_headless_system_with_no_displays_still_yields_a_window() {
        let (placement, warning) = resolve(&PlacementRequest {
            configured: None,
            focused: None,
            monitors: &[],
        });

        assert_eq!(placement.width, FALLBACK_WIDTH);
        assert!(warning.is_some());
    }

    #[test]
    fn a_display_smaller_than_the_minimum_is_clamped_up() {
        let tiny = MonitorGeometry {
            x: 10,
            y: 20,
            width: 640,
            height: 480,
        };
        let monitors = [tiny];
        let (placement, _) = resolve(&PlacementRequest {
            configured: None,
            focused: Some(tiny),
            monitors: &monitors,
        });

        assert_eq!((placement.width, placement.height), (MIN_WIDTH, MIN_HEIGHT));
        assert_eq!((placement.x, placement.y), (10, 20));
    }

    #[test]
    fn args_parse_both_forms() {
        assert_eq!(
            from_args(args(&["--overlay-monitor", "2"])).unwrap(),
            Some(ConfiguredPlacement::Monitor(2))
        );
        assert_eq!(
            from_args(args(&["--overlay-position", "100,-40"])).unwrap(),
            Some(ConfiguredPlacement::Position { x: 100, y: -40 })
        );
        assert_eq!(from_args(args(&["--unrelated"])).unwrap(), None);
    }

    #[test]
    fn a_malformed_argument_is_an_error_not_a_silent_default() {
        for bad in [
            args(&["--overlay-monitor", "left"]),
            args(&["--overlay-monitor"]),
            args(&["--overlay-position", "100"]),
            args(&["--overlay-position", "100,up"]),
        ] {
            let described = format!("{bad:?}");
            assert!(
                from_args(bad).is_err(),
                "{described} must be rejected, not ignored"
            );
        }
    }
}

//! # Screen Snap — the desktop app
//!
//! agg-gui-shell owns the winit window, wgpu and the event loop; this crate is
//! the OS side (PLAN.md "Architecture"): screen capture ([`capture`]), the
//! global hotkey ([`hotkey`]), the menu-bar icon ([`tray`]), clipboard / files
//! / permission ([`platform`]), reshaping the one window per mode
//! ([`window`]), the settings file ([`persist`]) and the idle-time glue that
//! ties them to the core's `AppState` ([`host`]). Every widget is the core's.
//!
//! Start-up (inside the shell's builder closure, on the main thread): read the
//! OS light / dark appearance, install the theme and fonts, load the settings,
//! put the icon in the menu bar, register the hotkey, then build the app. The
//! window starts invisible; the first idle pass hides it — or, on the first
//! run, opens Settings as the welcome.
//!
//! Command line (see [`USAGE`]): `--settings` opens Settings at start-up,
//! `--capture-now` captures right after start-up (for trying the overlay
//! without pressing the hotkey), `--write-icon PATH SIZE` writes the app icon
//! as a PNG and exits (used by `scripts/bundle-mac.sh`). Environment:
//! `SCREENSNAP_SETTINGS_DIR` moves the settings file; `SCREENSNAP_SCREENSHOT=out.png`
//! writes the window's frame after `SCREENSNAP_SCREENSHOT_FRAMES` (default 10)
//! painted frames and exits — with `--capture-now` that is the overlay, with
//! `--settings` the settings window; `SCREENSNAP_LOG_TIMING=1` prints capture
//! times.

mod capture;
mod events;
mod host;
mod hotkey;
mod persist;
mod platform;
mod tray;
mod window;

use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use agg_gui_shell::winit::window::Theme;
use agg_gui_shell::{RedrawPolicy, ShellConfig, ShellError};
use screensnap_core::capture::RgbaImage;
use screensnap_core::{build_app, install_theme_and_fonts, AppState, APP_NAME};

use host::AppHost;

const USAGE: &str = "\
Usage: screensnap [--settings] [--capture-now]
       screensnap --write-icon PATH SIZE

  --settings        open the settings window at start-up
  --capture-now     freeze the screen right after start-up, as the hotkey does
  --write-icon      write the app icon as a SIZE x SIZE PNG to PATH and exit
  --help            show this text

Environment:
  SCREENSNAP_SETTINGS_DIR      folder for settings.json (default: the OS config dir)
  SCREENSNAP_SCREENSHOT        write the window's frame to this PNG and exit
  SCREENSNAP_SCREENSHOT_FRAMES painted frames before that screenshot (default 10)
  SCREENSNAP_LOG_TIMING        print how long each screen capture took";

/// Side of the window icon (Windows / Linux title bar and taskbar).
const WINDOW_ICON_SIZE: u32 = 256;

/// What the command line asked for.
#[derive(Debug, PartialEq)]
enum Command {
    Help,
    WriteIcon { path: String, size: u32 },
    Run { settings: bool, capture_now: bool },
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut settings = false;
    let mut capture_now = false;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--settings" => settings = true,
            "--capture-now" => capture_now = true,
            "--write-icon" => {
                let path = iter.next().ok_or("--write-icon needs a PATH")?.clone();
                let size = iter
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|s| (1..=4096).contains(s))
                    .ok_or("--write-icon needs a SIZE from 1 to 4096")?;
                return Ok(Command::WriteIcon { path, size });
            }
            // macOS passes `-psn_…` to apps launched from Finder on old systems.
            other if other.starts_with("-psn_") => {}
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Command::Run {
        settings,
        capture_now,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match parse_args(&args) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Command::WriteIcon { path, size }) => write_icon(&path, size),
        Ok(Command::Run {
            settings,
            capture_now,
        }) => run_app(settings, capture_now).map_err(|e| e.to_string()),
        Err(message) => {
            eprintln!("screensnap: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("screensnap: {message}");
            ExitCode::FAILURE
        }
    }
}

/// `--write-icon`: the app icon as a PNG.
fn write_icon(path: &str, size: u32) -> Result<(), String> {
    let image = RgbaImage::new(size, size, screensnap_core::icon::rgba(size))
        .ok_or("icon has the wrong size")?;
    let png = screensnap_core::output::encode_png(&image, 1.0)?;
    std::fs::write(path, png).map_err(|e| format!("could not write {path}: {e}"))
}

/// Run the menu-bar app until Quit.
fn run_app(open_settings: bool, capture_now: bool) -> Result<(), ShellError> {
    let mut config = ShellConfig::new(APP_NAME)
        .with_logical_size(window::SETTINGS_SIZE.0, window::SETTINGS_SIZE.1)
        .with_min_logical_size(420.0, 360.0)
        // Nothing animates on its own; paint when input or state asks.
        .with_redraw_policy(RedrawPolicy::Reactive)
        .with_device_label("screensnap-native")
        .with_icon_rgba(
            screensnap_core::icon::rgba(WINDOW_ICON_SIZE),
            WINDOW_ICON_SIZE,
            WINDOW_ICON_SIZE,
        );
    let screenshot = std::env::var_os("SCREENSNAP_SCREENSHOT");
    if let Some(path) = &screenshot {
        let frames = std::env::var("SCREENSNAP_SCREENSHOT_FRAMES")
            .ok()
            .and_then(|f| f.parse().ok())
            .unwrap_or(10);
        config = config.with_screenshot(path, frames);
    }
    let settings_dir = persist::settings_dir(screenshot.is_some());

    agg_gui_shell::run(config, move |init| {
        let window = Arc::clone(init.window());
        let dark = window.theme() == Some(Theme::Dark);
        let fonts = install_theme_and_fonts(init.device_scale(), dark).map_err(ShellError::app)?;

        let state = AppState::new(Rc::new(platform::NativePlatform));
        if let Some(dir) = &settings_dir {
            state.use_settings_store(Box::new(persist::FileSettings::new(dir)));
        }
        platform::request_capture_permission_once();

        let host_shaper = window::WindowShaper::new(Arc::clone(&window));
        host_shaper.prepare_start();
        // The hidden-start app must not show a Dock icon while it launches.
        host_shaper.on_launched();

        let settings = state.settings();
        let tray = match tray::Tray::new(Some(&settings.hotkey)) {
            Ok(tray) => Some(tray),
            Err(message) => {
                // Still usable through the hotkey; say why the icon is missing.
                eprintln!("screensnap: {message}");
                state.set_status(Some(message));
                None
            }
        };
        let hotkeys = hotkey::HotkeyRegistrar::new();
        if open_settings || !settings.welcomed {
            state.show_settings();
        }
        let host = AppHost::new(state.clone(), window, hotkeys, tray, capture_now);
        Ok((build_app(state, fonts), host))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn command_line() {
        assert_eq!(
            parse_args(&[]),
            Ok(Command::Run {
                settings: false,
                capture_now: false
            })
        );
        assert_eq!(
            parse_args(&args(&["--capture-now", "--settings"])),
            Ok(Command::Run {
                settings: true,
                capture_now: true
            })
        );
        assert_eq!(parse_args(&args(&["--help"])), Ok(Command::Help));
        assert_eq!(
            parse_args(&args(&["--write-icon", "a.png", "512"])),
            Ok(Command::WriteIcon {
                path: "a.png".into(),
                size: 512
            })
        );
        assert!(parse_args(&args(&["--write-icon", "a.png", "0"])).is_err());
        assert!(parse_args(&args(&["--write-icon"])).is_err());
        assert!(parse_args(&args(&["--bogus"])).is_err());
    }

    #[test]
    fn icon_png_is_written() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("icon.png");
        write_icon(path.to_str().expect("utf-8 path"), 32).expect("write icon");
        let bytes = std::fs::read(&path).expect("read back");
        assert_eq!(&bytes[1..4], b"PNG");
    }
}

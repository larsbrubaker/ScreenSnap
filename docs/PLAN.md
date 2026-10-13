# Screen Snap — plan

Screen Snap is a Greenshot-style region screenshot tool for macOS first, then Windows and
Linux, built on agg-gui. Press a user-defined global hotkey and the screen **freezes** —
open menus, tooltips and the mouse pointer included, because the pixels are grabbed the
instant the key fires, before Screen Snap's own window appears. Drag a marquee over the
frozen screen; on release the region goes to the clipboard and/or a PNG file.

## User-visible behaviour (v0.1)

- **Lives in the menu bar** (tray icon; no Dock icon on macOS). Its menu:
  `Capture Region  <hotkey>` · `Settings…` · `Open Captures Folder` · `Quit Screen Snap`.
- **First run** opens the Settings window (welcome); later runs start hidden.
- **Hotkey** (default ⇧⌘2 on macOS, Ctrl+Shift+2 elsewhere) captures the monitor under the
  pointer and shows it full-screen, borderless, above everything (menu bar, Dock, other
  apps' menus).
- **Overlay**:
  - the frozen screen at exactly 1:1 physical pixels, dimmed outside the selection
    (whole screen dimmed until a drag starts);
  - full-height/width crosshair lines through the pointer (1 px, dark with a light halo);
  - the **magnifier** (if enabled): a square loupe next to the pointer (flips to stay on
    screen) showing ~15×15 capture pixels at 8×, nearest-neighbour, with a pixel grid and
    the centre pixel outlined; below it the pointer's pixel position and its colour as hex;
  - drag with the left button to draw the selection, snapped to whole capture pixels
    (inclusive of start and end pixel); a label shows `W × H` in pixels;
  - release with a non-empty selection → `AppState::finish_capture(rect)`;
  - `Escape` or right click → `AppState::cancel_capture()` (Escape during a drag cancels
    the whole capture too, like Greenshot);
  - `Space` or `Enter` with no drag in progress → capture the whole screen;
  - the OS cursor is a crosshair.
- **Delivery** (settings): copy to clipboard (default on), save PNG to a folder (default on,
  `~/Pictures/Screen Snap`), file name `Screen Snap 2026-10-12 at 18.07.33.png`, Retina
  density written into the PNG. The pointer is drawn into the frozen screen when "Include
  mouse pointer" is on (default on).
- **Settings window**: hotkey recorder; Copy to clipboard; Save to folder + path +
  Choose… + Show; Include mouse pointer; Show magnifier; Screen Recording permission
  warning with "Open System Settings" when not granted; hotkey registration error
  (taken by another app); last capture's outcome; Close and Quit buttons.

## Architecture

```
screensnap-core/   rlib, platform-free, all tests
  hotkey.rs        Hotkey (mods + W3C key code), accelerator text, display, key mapping
  settings.rs      Settings (serde JSON) + SettingsStore trait
  capture.rs       RgbaImage, CursorSnapshot, Capture (+ pointer compositing)
  selection.rs     PxRect / PxPoint / CaptureSpace (widget Y-up points <-> capture px)
  output.rs        crop, encode_png (pHYs), file_name, unique_path, deliver
  platform.rs      SnapPlatform trait (clipboard, folders, reveal, permission)
  state.rs         AppState: Mode {Hidden, Settings, Overlay}, capture, settings, status
  fonts.rs         theme (light/dark) + fonts, Font Awesome code points (fa::*)
  icon.rs          the app / menu-bar icon drawn in code (RGBA)
  ui/mod.rs        root Rebuilder keyed on AppState::mode_version
  ui/overlay/      CaptureOverlay widget (+ loupe, drawing helpers)
  ui/settings_view.rs, ui/hotkey_recorder.rs
screensnap-native/ bin `screensnap` (CLI: --settings, --capture-now, --write-icon PATH SIZE)
  main.rs          agg_gui_shell::run, start-up (theme, settings, permission, tray, hotkey)
  host.rs          the ShellHost: mode -> window shape, reveal after a painted frame,
                   capture, deferred delivery, settings save, hotkey sync, quit
  events.rs        hotkey / tray events queue + loop wake
  platform.rs      NativePlatform: SnapPlatform via arboard / rfd / open -R / CG permission
  capture/         monitor under the pointer (xcap, all OSes) + pointer image per OS
                   (pointer_macos.rs via NSCursor + CGEvent; pointer_other.rs: none yet)
  hotkey.rs        global-hotkey registration, re-registration on settings change
  tray.rs          tray-icon menu-bar item and its menu
  window/          per-OS window shaping: macos.rs (NSWindow level, frame, collection
                   behaviour, activation policy), macos_constrain.rs (see below),
                   other.rs (winit-only fallback)
  persist.rs       settings.json in the OS config dir (SCREENSNAP_SETTINGS_DIR overrides)
packaging/macos/Info.plist, scripts/bundle-mac.sh, run-mac.command
```

### The one-window model

agg-gui-shell owns exactly one winit window. Screen Snap reuses it for every mode and
reshapes it when `AppState::mode_version` changes (checked in `ShellHost::on_idle`):

| Mode     | Window                                                                    |
|----------|---------------------------------------------------------------------------|
| Hidden   | ordered out (invisible), app inactive                                      |
| Settings | titled, resizable, ~560×520 logical, centred, normal level, activated      |
| Overlay  | borderless, frame = the captured monitor's frame (covering the menu bar), level above the menu bar and pop-up menus (`NSScreenSaverWindowLevel`), joins all Spaces / full-screen auxiliary, activated and key |

To avoid a flash of stale content, the overlay is shown nearly transparent (alpha 1/255 —
wgpu's Metal backend will not paint a window at alpha 0, which macOS reports as occluded),
painted once at its new size, then set to alpha 1.

**AppKit frame clamp workaround** (`window/macos_constrain.rs`): agg-gui-shell calls
`window.is_maximized()` on every `Resized`; winit's macOS implementation briefly toggles a
borderless window's style mask, and each `setStyleMask` makes AppKit fit the window inside
the visible area (below the menu bar, above the Dock). While the overlay is up we override
`constrainFrameRect:toScreen:` on winit's NSWindow subclass to return the rect unchanged.
Delete it once agg-gui-shell stops calling `is_maximized()` per resize. The red close button of the
Settings window hides it (`on_close_requested` returns `false` and calls
`AppState::hide_settings`); only Quit exits.

### Events into the loop

global-hotkey and tray-icon deliver events through callbacks on the main thread; the
callbacks push into a `Mutex<VecDeque<NativeEvent>>` and call
`agg_gui::animation::signal_async_state_change()` to wake the loop. `on_idle` drains them.
The capture itself happens in `on_idle` as the first thing after draining, while the window
is still hidden.

### Platform roadmap

- **macOS** (v0.1): everything above. Needs the Screen Recording permission
  (`CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess`); without it captures
  show only the wallpaper. `scripts/bundle-mac.sh` builds `Screen Snap.app`
  (`LSUIElement`, bundle id `com.larsbrubaker.screensnap`, ad-hoc signed).
- **Windows** (next): xcap capture already works; pointer via `GetCursorInfo` +
  `DrawIconEx`; window shaping via winit `WindowLevel::AlwaysOnTop` + borderless
  monitor-sized window; default hotkey Ctrl+Shift+2 or PrintScreen.
- **Linux** (later): X11 via xcap + XFixes pointer; Wayland needs the portal (no global
  hotkeys without the GlobalShortcuts portal) — documented limitation.

### Known limitations (v0.1)

- One monitor per capture (the one under the pointer).
- A click without a drag selects nothing (Greenshot would capture the window under it).
- Unbundled `cargo run` briefly shows a Dock tile (agg-gui-shell has no activation-policy
  hook; winit sets Regular at launch); the app bundle does not.
- Hotkey punctuation keys follow the US layout (letters and digits are layout-proof).
- An ad-hoc signed bundle loses its Screen Recording grant whenever it is rebuilt.

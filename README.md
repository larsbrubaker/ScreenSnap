# Screen Snap

A Greenshot-style screenshot tool for the Mac (Windows and Linux on the way), written in
pure Rust on [agg-gui](https://github.com/larsbrubaker/agg-gui).

Press your capture shortcut and the screen **freezes** — open menus, tooltips, hover states
and the mouse pointer included — so you can take pictures of things that vanish the moment
you reach for a screenshot tool. Drag a marquee around what you want and it is copied to the
clipboard, saved as a PNG, or both.

## Features

- **Global hotkey you choose** (default ⇧⌘2 on macOS, Ctrl+Shift+2 elsewhere), recorded in
  Settings by just pressing it.
- **Frozen screen**: the pixels are grabbed the instant the key fires, before Screen Snap's
  own window appears, so menus and tooltips stay in the picture.
- **Mouse pointer capture**: the pointer is drawn into the frozen screen (optional).
- **Pixel-exact marquee** with crosshair guides, a size readout and a magnifier showing the
  pixels and colour under the pointer.
- **Clipboard and / or disk**: PNGs go to `~/Pictures/Screen Snap` (or any folder), named
  `Screen Snap 2026-10-12 at 18.07.33.png`, with Retina density recorded.
- **Lives in the menu bar**, no Dock icon.

### Keys while capturing

| Key / mouse            | Action                         |
|------------------------|--------------------------------|
| Drag                   | Select a region                |
| Space or Return        | Capture the whole screen       |
| Esc or right click     | Cancel                         |

## Build from source

Screen Snap expects the agg-gui checkout beside it (`../agg-gui`), as in the
[rust-apps](https://github.com/larsbrubaker/rust-apps) superproject.

```bash
cargo run --release -p screensnap-native
```

On macOS, double-click `run-mac.command`, or build an app bundle with
`scripts/bundle-mac.sh`. The first capture asks for the **Screen Recording** permission
(System Settings ▸ Privacy & Security ▸ Screen & System Audio Recording); until it is
allowed, the hotkey opens Settings with a warning instead of capturing. Run from
`run-mac.command` or a terminal, the permission belongs to the terminal app (Terminal,
iTerm…), so allow that app and restart it; the `.app` bundle gets its own entry.

## Status

macOS works today. Windows and Linux are planned — the capture layer (xcap), hotkey
(global-hotkey) and menu-bar icon (tray-icon) are already cross-platform; what remains is
per-OS pointer capture and overlay window shaping. See [docs/PLAN.md](docs/PLAN.md).

## License

MIT — see [LICENSE](LICENSE).

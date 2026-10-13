# Screen Snap — working charter

Screen Snap is a Greenshot-style region screenshot tool (freeze the screen on a global
hotkey, marquee a region, copy / save it), macOS first, Windows and Linux to follow, built
on agg-gui. `docs/PLAN.md` is the spec: behaviour, architecture and the platform roadmap.
Read it before writing code, and keep it current when a decision changes.

## The pillars

**No stubs, no shortcuts.** Every function is complete and production-ready. No `todo!()`,
no `unimplemented!()`, no placeholder screens.

**Test-first bug fixing.** 1) Write a failing test that reproduces the bug. 2) Fix it.
3) Confirm the test passes. Never weaken, `#[ignore]` or delete a test to make it pass.

**All UI through agg-gui.** The native shell owns the OS (capture, hotkey, tray, window
shape) and supplies `SnapPlatform`; it builds no widgets. Everything people see inside the
window is an agg-gui widget in `screensnap-core/src/ui`. The menu-bar (tray) menu is the
one native UI, because it lives outside the window.

**Core stays platform-free.** No `cfg(target_os)` in `screensnap-core` except
`DisplayStyle::native()` / `Hotkey::default_for_platform()`. OS differences live in
`screensnap-native`, split per OS (`window/macos.rs`, …) so Windows and Linux slot in
without touching the core.

**Icons are Font Awesome** (4.7, agg-gui's bundled face) by code point, named in
`screensnap-core/src/fonts.rs` (`fa::*`). No image icons in the UI.

## Coding standards

- **800-line hard limit per file**; split real logic into modules (never by deleting
  comments or blank lines).
- Every file begins with a comment block describing its purpose and how it relates to the
  rest of the project.
- Prefer `Result`/`Option` over `unwrap` outside tests; `expect` only for start-up failures
  with a clear message. Document every `unsafe` block (objc / CoreGraphics FFI).
- Measure before optimising.

## Local development uses agg-gui as a path dep — improve it as you go

`Cargo.toml` patches `agg-gui` and `agg-gui-shell` to the sibling checkout `../agg-gui/`
(the rust-apps superproject layout). When Screen Snap needs a capability agg-gui lacks,
add it to agg-gui itself rather than working around it here.

## Checks

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

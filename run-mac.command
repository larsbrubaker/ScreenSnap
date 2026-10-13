#!/bin/bash
# run-mac.command: double-click in Finder (or run from a terminal) to build and launch
# Screen Snap on macOS. It builds an optimised release binary the first time, which takes
# a few minutes; later launches only rebuild what changed. Screen Snap lives in the menu
# bar (look for the crop-corners icon); the first run opens its settings window.
# Arguments are passed through, e.g. `./run-mac.command --settings`.
#
# Running the bare binary makes macOS ask for Screen Recording on behalf of whatever
# launched it (Terminal); `scripts/bundle-mac.sh` builds a proper "Screen Snap.app".
# The build expects the agg-gui checkout beside this repo (../agg-gui), as in the
# rust-apps superproject.

set -u
cd "$(dirname "$0")" || exit 1

# Finder starts .command files in a fresh login shell, which may not have cargo on PATH.
if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi

fail() {
    echo
    echo "Screen Snap: $1"
    exit 1
}

command -v cargo >/dev/null 2>&1 || fail "cargo was not found. Install Rust from https://rustup.rs and try again."
[ -d ../agg-gui ] || fail "../agg-gui was not found. Clone https://github.com/larsbrubaker/agg-gui beside this folder (or use the rust-apps superproject)."

echo "Building Screen Snap (release)..."
cargo build --release -p screensnap-native || fail "the build failed (see the errors above)."

echo "Starting Screen Snap..."
exec ./target/release/screensnap "$@"

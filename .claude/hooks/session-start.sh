#!/bin/bash
#
# Everything a cloud container needs before `cargo test` or `pnpm test` will
# run. None of it is needed on a developer's own machine, which has its own
# toolchain already, so the whole script is a no-op outside the cloud.

set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

as_root() {
  if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi
}

# ── System packages ───────────────────────────────────────────────────────────
#
# Tauri links against the system GTK and WebKit libraries, and the Rust lib does
# not configure without them — which takes `cargo test` with it, since the test
# binaries link the lib. The error is a pkg-config failure in gdk-sys, a long
# way from anything this repo wrote.
#
# `apt-get update` first is not optional. On a fresh container the package lists
# are stale enough that libgtk-3-dev resolves to "no installation candidate"
# even though it exists.
SYSTEM_PACKAGES=(
  libgtk-3-dev
  libwebkit2gtk-4.1-dev
  libjavascriptcoregtk-4.1-dev
  libsoup-3.0-dev
  librsvg2-dev
  patchelf
  # Mesa's software Vulkan driver (llvmpipe). The L2 suite runs the shipped WGSL
  # through wgpu headlessly, and a cloud container has no GPU, so without a
  # software adapter roughly three quarters of the Rust tests cannot run at all.
  mesa-vulkan-drivers
  # The L3 suite runs Chromium headed, which needs a display to open onto.
  xvfb
)

export DEBIAN_FRONTEND=noninteractive
as_root apt-get update -qq
as_root apt-get install -y -qq "${SYSTEM_PACKAGES[@]}"

# llvmpipe warns on every adapter request without this, which buries real test
# output. The directory has to exist for the variable to be worth setting.
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/xdg-runtime}"
mkdir -p "$RUNTIME_DIR"
chmod 700 "$RUNTIME_DIR"
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export XDG_RUNTIME_DIR=$RUNTIME_DIR" >> "$CLAUDE_ENV_FILE"
fi

# ── The browser suite ─────────────────────────────────────────────────────────
#
# The L3 suite drives the real pipeline through Chromium's WebGPU, and a cloud
# container is missing all three things that takes. Each is exported rather than
# wrapped around the test command so that `pnpm test:browser` works unchanged.

# A display. The suite runs headed deliberately — headless Chromium offers no
# adapter — so with no DISPLAY it cannot launch at all.
if [ -z "${DISPLAY:-}" ] && command -v Xvfb >/dev/null 2>&1; then
  for n in 99 98 97; do
    if [ ! -e "/tmp/.X11-unix/X$n" ]; then
      nohup Xvfb ":$n" -screen 0 1280x1024x24 >/dev/null 2>&1 &
      disown || true
      if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
        echo "export DISPLAY=:$n" >> "$CLAUDE_ENV_FILE"
      fi
      break
    fi
  done
fi

# A Chromium that Playwright will accept. The container ships its own build
# under PLAYWRIGHT_BROWSERS_PATH, and it is rarely the revision the installed
# Playwright asks for — which it reports as a missing executable, suggesting an
# install that the container has no network path for.
PW_BROWSERS="${PLAYWRIGHT_BROWSERS_PATH:-/opt/pw-browsers}"
CHROMIUM_EXECUTABLE="$(
  ls -d "$PW_BROWSERS"/chromium-*/chrome-linux*/chrome 2>/dev/null | sort -V | tail -1
)"
if [ -n "$CHROMIUM_EXECUTABLE" ] && [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export CHROMIUM_EXECUTABLE=$CHROMIUM_EXECUTABLE" >> "$CLAUDE_ENV_FILE"
fi

# An adapter. Chromium defines navigator.gpu here but returns nothing from
# requestAdapter without this flag, which is the one that lets it fall back to
# the software driver installed above.
if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  echo "export CHROMIUM_FLAGS=--enable-unsafe-webgpu" >> "$CLAUDE_ENV_FILE"
fi

# ── Project dependencies ──────────────────────────────────────────────────────

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

pnpm install

# svelte-check reads a tsconfig that SvelteKit generates into .svelte-kit, and
# fails before looking at any source if it is missing. `pnpm check` does this
# itself; running it here means `pnpm exec svelte-check` works directly too.
pnpm exec svelte-kit sync

# Warm the crate registry. The compile itself is left to the session: a cold
# build of tauri and wgpu is several minutes, and this hook runs before the
# session starts.
cargo fetch --manifest-path src-tauri/Cargo.toml

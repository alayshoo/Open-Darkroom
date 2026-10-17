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

#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS DMG can only be built on macOS" >&2
  exit 2
fi

profile="release"
tauri_args=(build --bundles dmg --ci)

case "${1:-}" in
  "")
    ;;
  --debug)
    profile="debug"
    tauri_args+=(--debug)
    ;;
  *)
    echo "usage: $0 [--debug]" >&2
    exit 2
    ;;
esac

# Tauri's DMG helper otherwise asks Finder to arrange the disk image window.
# Finder AppleEvents can time out on locked, headless, or automation-controlled
# Macs. CI=true makes the helper use its supported --skip-jenkins path while
# preserving the app bundle, Applications symlink, volume icon, and compression.
CI=true npx tauri "${tauri_args[@]}"

target_dir="$(
  cargo metadata \
    --format-version 1 \
    --no-deps \
    --manifest-path src-tauri/Cargo.toml |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
)"
bundle_dir="${target_dir}/${profile}/bundle/dmg"
shopt -s nullglob
dmg_files=("${bundle_dir}"/JiaofuSuite_*.dmg)
shopt -u nullglob

if [[ ${#dmg_files[@]} -ne 1 ]]; then
  echo "error: expected exactly one JiaofuSuite DMG in ${bundle_dir}, found ${#dmg_files[@]}" >&2
  exit 1
fi

bash scripts/verify_macos_dmg.sh "${dmg_files[0]}"

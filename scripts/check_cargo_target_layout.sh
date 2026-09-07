#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
expected_target="${repo_root}/target"

metadata_target() {
  local manifest_path="$1"

  cargo metadata \
    --format-version 1 \
    --no-deps \
    --manifest-path "${manifest_path}" |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

workspace_target="$(metadata_target "${repo_root}/Cargo.toml")"
tauri_target="$(metadata_target "${repo_root}/src-tauri/Cargo.toml")"

if [[ "${workspace_target}" != "${expected_target}" ]]; then
  echo "error: workspace target is ${workspace_target}, expected ${expected_target}" >&2
  exit 1
fi

if [[ "${tauri_target}" != "${expected_target}" ]]; then
  echo "error: Tauri target is ${tauri_target}, expected ${expected_target}" >&2
  exit 1
fi

if [[ -d "${repo_root}/src-tauri/target" && ! -L "${repo_root}/src-tauri/target" ]]; then
  echo "error: duplicate Cargo cache exists at ${repo_root}/src-tauri/target" >&2
  exit 1
fi

echo "Cargo target layout OK: ${expected_target}"

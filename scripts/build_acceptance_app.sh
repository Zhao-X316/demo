#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ $# -ne 1 ]]; then
  echo "usage: $0 src-tauri/tauri.<name>-acceptance.conf.json" >&2
  exit 2
fi

config_arg="$1"
config_path="${repo_root}/${config_arg}"

case "${config_path}" in
  "${repo_root}"/src-tauri/tauri.*.conf.json)
    ;;
  *)
    echo "error: acceptance config must be a tauri.*.conf.json file under src-tauri" >&2
    exit 2
    ;;
esac

if [[ ! -f "${config_path}" ]]; then
  echo "error: acceptance config not found: ${config_arg}" >&2
  exit 1
fi

tauri_bin="${repo_root}/node_modules/.bin/tauri"
if [[ ! -x "${tauri_bin}" ]]; then
  echo "error: local Tauri CLI not found; run npm install first" >&2
  exit 1
fi

cd "${repo_root}"
exec "${tauri_bin}" build --debug --bundles app --config "${config_arg}"

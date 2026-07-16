#!/usr/bin/env bash

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: macOS DMG verification can only run on macOS" >&2
  exit 2
fi

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <path-to-dmg>" >&2
  exit 2
fi

dmg_path="$1"
expected_identifier="${JIAOFU_EXPECTED_BUNDLE_ID:-com.jiaofu.suite}"
expected_version="${JIAOFU_EXPECTED_VERSION:-0.1.0}"
expected_app_name="${JIAOFU_EXPECTED_APP_NAME:-JiaofuSuite.app}"

if [[ ! -f "${dmg_path}" ]]; then
  echo "error: DMG not found: ${dmg_path}" >&2
  exit 1
fi

hdiutil verify "${dmg_path}" >/dev/null

mount_root="$(mktemp -d "${TMPDIR:-/tmp}/jiaofu-dmg-verify.XXXXXX")"
device=""
mount_dir=""

cleanup() {
  if [[ -n "${device}" ]]; then
    hdiutil detach "${device}" >/dev/null 2>&1 || true
  fi
  rm -rf "${mount_root}"
}
trap cleanup EXIT

attach_output="$(hdiutil attach -readonly -noverify -noautoopen -nobrowse -mountrandom "${mount_root}" "${dmg_path}")"
device="$(printf '%s\n' "${attach_output}" | awk '/^\/dev\// {print $1; exit}')"
mount_dir="$(printf '%s\n' "${attach_output}" | awk '/Apple_(HFS|APFS)/ {print $NF; exit}')"

if [[ -z "${device}" || -z "${mount_dir}" || ! -d "${mount_dir}" ]]; then
  echo "error: failed to identify mounted DMG device or mount point" >&2
  exit 1
fi

app_path="${mount_dir}/${expected_app_name}"
info_plist="${app_path}/Contents/Info.plist"

if [[ ! -d "${app_path}" || ! -f "${info_plist}" ]]; then
  echo "error: expected app bundle is missing: ${app_path}" >&2
  exit 1
fi

if [[ ! -L "${mount_dir}/Applications" || "$(readlink "${mount_dir}/Applications")" != "/Applications" ]]; then
  echo "error: DMG Applications link is missing or has the wrong target" >&2
  exit 1
fi

unexpected_entries="$(find "${mount_dir}" -mindepth 1 -maxdepth 1 \
  ! -name "${expected_app_name}" \
  ! -name 'Applications' \
  ! -name '.VolumeIcon.icns' \
  ! -name '.DS_Store' \
  -print)"

if [[ -n "${unexpected_entries}" ]]; then
  echo "error: unexpected top-level files found in DMG" >&2
  printf '%s\n' "${unexpected_entries}" >&2
  exit 1
fi

bundle_identifier="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "${info_plist}")"
bundle_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "${info_plist}")"
bundle_executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "${info_plist}")"

if [[ "${bundle_identifier}" != "${expected_identifier}" ]]; then
  echo "error: bundle identifier ${bundle_identifier} != ${expected_identifier}" >&2
  exit 1
fi

if [[ "${bundle_version}" != "${expected_version}" ]]; then
  echo "error: bundle version ${bundle_version} != ${expected_version}" >&2
  exit 1
fi

if [[ -z "${bundle_executable}" || ! -x "${app_path}/Contents/MacOS/${bundle_executable}" ]]; then
  echo "error: bundle executable is missing or not executable" >&2
  exit 1
fi

forbidden_paths="$(find "${app_path}" \( -type f -o -type l \) \( \
  -name 'secrets.json' -o \
  -name '.env' -o \
  -name '.env.*' -o \
  -name '*.db' -o \
  -name '*.db-*' -o \
  -name '*.sqlite' -o \
  -name '*.sqlite-*' -o \
  -name '*.sqlite3' -o \
  -name '*.sqlite3-*' \
\) -print)"

if [[ -n "${forbidden_paths}" ]]; then
  echo "error: forbidden credential or database files found in app bundle" >&2
  printf '%s\n' "${forbidden_paths}" >&2
  exit 1
fi

dmg_size="$(stat -f '%z' "${dmg_path}")"
dmg_sha256="$(shasum -a 256 "${dmg_path}" | awk '{print $1}')"

printf 'DMG verification passed\n'
printf '  path: %s\n' "${dmg_path}"
printf '  bytes: %s\n' "${dmg_size}"
printf '  sha256: %s\n' "${dmg_sha256}"
printf '  bundle_id: %s\n' "${bundle_identifier}"
printf '  version: %s\n' "${bundle_version}"
printf '  executable: %s\n' "${bundle_executable}"
printf '  unexpected_top_level_files: 0\n'
printf '  forbidden_files: 0\n'
